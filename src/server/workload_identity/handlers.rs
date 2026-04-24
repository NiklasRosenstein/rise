use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::db::{projects, service_accounts, users};
use crate::server::auth::context::AuthContext;
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::project::handlers::{check_read_permission, check_write_permission};
use crate::server::ssrf;
use crate::server::state::AppState;
use crate::server::workload_identity::models::{
    CreateWorkloadIdentityRequest, ListWorkloadIdentitiesResponse, UpdateWorkloadIdentityRequest,
    WorkloadIdentityResponse,
};

/// Verify that an OIDC issuer is reachable and has valid configuration.
///
/// Includes SSRF protections: requires HTTPS, blocks private/internal IPs,
/// enforces request timeout and redirect limits.
async fn verify_oidc_issuer(
    issuer_url: &str,
    ssrf_config: &ssrf::SsrfConfig,
) -> Result<(), ServerError> {
    // Validate the issuer URL against SSRF (requires HTTPS, blocks private IPs)
    ssrf::validate_url(issuer_url, ssrf_config)
        .await
        .map_err(|e| {
            tracing::warn!(
                "SSRF validation failed for issuer URL '{}': {}",
                issuer_url,
                e
            );
            ServerError::bad_request(format!("Invalid OIDC issuer URL: {}", e))
        })?;

    // Construct the OIDC discovery URL
    let discovery_url = if issuer_url.ends_with('/') {
        format!("{}well-known/openid-configuration", issuer_url)
    } else {
        format!("{}/.well-known/openid-configuration", issuer_url)
    };

    tracing::debug!("Verifying OIDC issuer at: {}", discovery_url);

    // Use SSRF-safe client (timeout + redirect limits)
    let client = ssrf::safe_client(ssrf_config);

    // Attempt to fetch the OIDC configuration
    let response = client.get(&discovery_url)
        .send()
        .await
        .server_err(
            StatusCode::BAD_REQUEST,
            format!("Failed to reach OIDC issuer: please verify the issuer URL '{}' is correct and accessible", issuer_url),
        )?;

    if !response.status().is_success() {
        tracing::warn!(
            "OIDC issuer {} returned non-success status: {}",
            issuer_url,
            response.status()
        );
        return Err(ServerError::bad_request(format!(
            "OIDC issuer returned status {}: {}. Please verify the issuer URL points to a valid OIDC provider.",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        )));
    }

    // Try to parse the response as JSON to verify it's valid OIDC configuration
    let config: serde_json::Value = response.json().await.server_err(
        StatusCode::BAD_REQUEST,
        format!("Invalid OIDC configuration: the issuer URL '{}' does not return valid OIDC discovery metadata", issuer_url),
    )?;

    // Verify required OIDC fields are present and validate issuer match (RFC 8414 Section 3.1)
    let returned_issuer = config
        .get("issuer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ServerError::bad_request(
                "OIDC configuration missing or invalid 'issuer' field (must be a string)",
            )
        })?;

    let expected = issuer_url.trim_end_matches('/');
    let actual = returned_issuer.trim_end_matches('/');
    if expected != actual {
        return Err(ServerError::bad_request(format!(
            "OIDC issuer mismatch: expected '{}', got '{}'",
            issuer_url, returned_issuer
        )));
    }

    if config.get("jwks_uri").is_none() {
        return Err(ServerError::bad_request(
            "OIDC configuration missing required 'jwks_uri' field",
        ));
    }

    tracing::info!("Successfully verified OIDC issuer: {}", issuer_url);
    Ok(())
}

/// Create a new service account for a project
pub async fn create_workload_identity(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_name): Path<String>,
    Json(req): Json<CreateWorkloadIdentityRequest>,
) -> Result<Json<WorkloadIdentityResponse>, ServerError> {
    let user = auth.user()?;

    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .internal_err("Failed to find project")?
        .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Check permission: user must be able to write to project
    if !check_write_permission(&state, &project, user)
        .await
        .map_err(ServerError::internal)?
    {
        return Err(ServerError::forbidden(
            "Cannot manage service accounts for this project",
        ));
    }

    // Validate issuer URL
    if req.issuer_url.is_empty() {
        return Err(ServerError::bad_request("Issuer URL cannot be empty"));
    }

    // Validate claims requirements
    if req.claims.is_empty() {
        return Err(ServerError::bad_request("At least one claim is required"));
    }

    // Require 'aud' claim
    if !req.claims.contains_key("aud") {
        return Err(ServerError::bad_request(
            "The 'aud' (audience) claim is required for service accounts",
        ));
    }

    // Require at least one additional claim besides 'aud'
    if req.claims.len() < 2 {
        return Err(ServerError::bad_request(
            "At least one claim in addition to 'aud' is required (e.g., project_path, ref_protected)",
        ));
    }

    // Verify OIDC issuer is reachable and has valid configuration
    // (also validates HTTPS requirement and SSRF protections)
    verify_oidc_issuer(&req.issuer_url, &state.server_settings.ssrf).await?;

    // Create service account
    let sa = service_accounts::create(&state.db_pool, project.id, &req.issuer_url, &req.claims)
        .await
        .internal_err("Failed to create service account")?;

    // Get user for response
    let sa_user = users::find_by_id(&state.db_pool, sa.user_id)
        .await
        .internal_err("Failed to find service account user")?
        .ok_or_else(|| ServerError::internal("Service account user not found"))?;

    // Convert JSONB claims to HashMap for response
    let claims: std::collections::HashMap<String, String> =
        serde_json::from_value(sa.claims).internal_err("Failed to deserialize claims")?;

    Ok(Json(WorkloadIdentityResponse {
        id: sa.id.to_string(),
        email: sa_user.email,
        project_name: project.name,
        issuer_url: sa.issuer_url,
        claims,
        created_at: sa.created_at.to_rfc3339(),
    }))
}

/// List all service accounts for a project
pub async fn list_workload_identities(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(project_name): Path<String>,
) -> Result<Json<ListWorkloadIdentitiesResponse>, ServerError> {
    let user = auth.user()?;

    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .internal_err("Failed to find project")?
        .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Check read permission
    if !check_read_permission(&state, &project, user)
        .await
        .map_err(ServerError::internal)?
    {
        return Err(ServerError::not_found("Project not found"));
    }

    // Get active service accounts
    let sas = service_accounts::list_by_project(&state.db_pool, project.id)
        .await
        .internal_err("Failed to list service accounts")?;

    // Convert to response
    let mut workload_identities = Vec::new();
    for sa in sas {
        let sa_user = users::find_by_id(&state.db_pool, sa.user_id)
            .await
            .internal_err("Failed to find service account user")?
            .ok_or_else(|| ServerError::internal("Service account user not found"))?;

        // Convert JSONB claims to HashMap
        let claims: std::collections::HashMap<String, String> =
            serde_json::from_value(sa.claims.clone())
                .internal_err("Failed to deserialize claims")?;

        workload_identities.push(WorkloadIdentityResponse {
            id: sa.id.to_string(),
            email: sa_user.email,
            project_name: project.name.clone(),
            issuer_url: sa.issuer_url,
            claims,
            created_at: sa.created_at.to_rfc3339(),
        });
    }

    Ok(Json(ListWorkloadIdentitiesResponse {
        workload_identities,
    }))
}

/// Get a specific service account
pub async fn get_workload_identity(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_name, sa_id)): Path<(String, Uuid)>,
) -> Result<Json<WorkloadIdentityResponse>, ServerError> {
    let user = auth.user()?;

    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .internal_err("Failed to find project")?
        .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Check read permission
    if !check_read_permission(&state, &project, user)
        .await
        .map_err(ServerError::internal)?
    {
        return Err(ServerError::not_found("Project not found"));
    }

    // Get service account
    let sa = service_accounts::get_by_id(&state.db_pool, sa_id)
        .await
        .internal_err("Failed to find service account")?
        .ok_or_else(|| ServerError::not_found("Service account not found"))?;

    // Verify SA belongs to this project
    if sa.project_id != project.id {
        return Err(ServerError::not_found("Service account not found"));
    }

    // Check if deleted
    if sa.deleted_at.is_some() {
        return Err(ServerError::not_found("Service account not found"));
    }

    // Get user
    let sa_user = users::find_by_id(&state.db_pool, sa.user_id)
        .await
        .internal_err("Failed to find service account user")?
        .ok_or_else(|| ServerError::internal("Service account user not found"))?;

    // Convert JSONB claims to HashMap
    let claims: std::collections::HashMap<String, String> =
        serde_json::from_value(sa.claims).internal_err("Failed to deserialize claims")?;

    Ok(Json(WorkloadIdentityResponse {
        id: sa.id.to_string(),
        email: sa_user.email,
        project_name: project.name,
        issuer_url: sa.issuer_url,
        claims,
        created_at: sa.created_at.to_rfc3339(),
    }))
}

/// Update a service account's issuer_url and/or claims
pub async fn update_workload_identity(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_name, sa_id)): Path<(String, Uuid)>,
    Json(req): Json<UpdateWorkloadIdentityRequest>,
) -> Result<Json<WorkloadIdentityResponse>, ServerError> {
    let user = auth.user()?;

    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .internal_err("Failed to find project")?
        .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Check write permission
    if !check_write_permission(&state, &project, user)
        .await
        .map_err(ServerError::internal)?
    {
        return Err(ServerError::forbidden(
            "Cannot manage service accounts for this project",
        ));
    }

    // Get service account
    let sa = service_accounts::get_by_id(&state.db_pool, sa_id)
        .await
        .internal_err("Failed to find service account")?
        .ok_or_else(|| ServerError::not_found("Service account not found"))?;

    // Verify SA belongs to this project
    if sa.project_id != project.id {
        return Err(ServerError::not_found("Service account not found"));
    }

    // Check if deleted
    if sa.deleted_at.is_some() {
        return Err(ServerError::not_found("Service account not found"));
    }

    // Validate that at least one field is provided
    if req.issuer_url.is_none() && req.claims.is_none() {
        return Err(ServerError::bad_request(
            "At least one field (issuer_url or claims) must be provided for update",
        ));
    }

    // Validate issuer URL if provided
    if let Some(ref issuer_url) = req.issuer_url {
        if issuer_url.is_empty() {
            return Err(ServerError::bad_request("Issuer URL cannot be empty"));
        }

        // Validate SSRF protections (HTTPS requirement + blocks private/internal IPs)
        ssrf::validate_url(issuer_url, &state.server_settings.ssrf)
            .await
            .map_err(|e| {
                tracing::warn!(
                    "SSRF validation failed for issuer URL '{}': {}",
                    issuer_url,
                    e
                );
                ServerError::bad_request(format!("Invalid OIDC issuer URL: {}", e))
            })?;
    }

    // Validate claims if provided
    if let Some(ref claims) = req.claims {
        if claims.is_empty() {
            return Err(ServerError::bad_request("Claims cannot be empty"));
        }

        // Require 'aud' claim
        if !claims.contains_key("aud") {
            return Err(ServerError::bad_request(
                "The 'aud' (audience) claim is required for service accounts",
            ));
        }

        // Require at least one additional claim besides 'aud'
        if claims.len() < 2 {
            return Err(ServerError::bad_request(
                "At least one claim in addition to 'aud' is required (e.g., project_path, ref_protected)",
            ));
        }
    }

    // Update service account
    let updated_sa = service_accounts::update(
        &state.db_pool,
        sa_id,
        req.issuer_url.as_deref(),
        req.claims.as_ref(),
        None, // Don't change allowed_environment_ids
    )
    .await
    .internal_err("Failed to update service account")?;

    // Get user for response
    let sa_user = users::find_by_id(&state.db_pool, updated_sa.user_id)
        .await
        .internal_err("Failed to find service account user")?
        .ok_or_else(|| ServerError::internal("Service account user not found"))?;

    // Convert JSONB claims to HashMap for response
    let claims: std::collections::HashMap<String, String> =
        serde_json::from_value(updated_sa.claims).internal_err("Failed to deserialize claims")?;

    Ok(Json(WorkloadIdentityResponse {
        id: updated_sa.id.to_string(),
        email: sa_user.email,
        project_name: project.name,
        issuer_url: updated_sa.issuer_url,
        claims,
        created_at: updated_sa.created_at.to_rfc3339(),
    }))
}

/// Delete a service account (soft delete)
pub async fn delete_workload_identity(
    State(state): State<AppState>,
    auth: AuthContext,
    Path((project_name, sa_id)): Path<(String, Uuid)>,
) -> Result<StatusCode, ServerError> {
    let user = auth.user()?;

    // Get project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .internal_err("Failed to find project")?
        .ok_or_else(|| ServerError::not_found("Project not found"))?;

    // Check write permission
    if !check_write_permission(&state, &project, user)
        .await
        .map_err(ServerError::internal)?
    {
        return Err(ServerError::forbidden(
            "Cannot manage service accounts for this project",
        ));
    }

    // Get service account
    let sa = service_accounts::get_by_id(&state.db_pool, sa_id)
        .await
        .internal_err("Failed to find service account")?
        .ok_or_else(|| ServerError::not_found("Service account not found"))?;

    // Verify SA belongs to this project
    if sa.project_id != project.id {
        return Err(ServerError::not_found("Service account not found"));
    }

    // Check if already deleted
    if sa.deleted_at.is_some() {
        return Err(ServerError::not_found("Service account not found"));
    }

    // Soft delete
    service_accounts::soft_delete(&state.db_pool, sa_id)
        .await
        .internal_err("Failed to delete service account")?;

    Ok(StatusCode::NO_CONTENT)
}
