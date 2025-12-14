use super::models::{
    AddDomainRequest, AddDomainResponse, AcmeChallenge as ApiAcmeChallenge, CnameRecord,
    CustomDomain as ApiCustomDomain, DomainSetupInstructions, VerificationResult,
    VerifyDomainResponse,
};
use crate::db::models::User;
use crate::db::{acme_challenges, custom_domains};
use crate::server::project::handlers::{check_write_permission, resolve_project};
use crate::server::project::models::GetProjectParams;
use crate::server::state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use tracing::info;

/// Add a custom domain to a project
pub async fn add_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
    Json(payload): Json<AddDomainRequest>,
) -> Result<Json<AddDomainResponse>, (StatusCode, String)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &project_id_or_name, params.by_id)
        .await
        .map_err(|(status, json_err)| (status, json_err.error.clone()))?;

    // Check write permission
    let can_write = check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check permissions: {}", e),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to add domains to this project".to_string(),
        ));
    }

    // Validate domain name format
    let domain_name = payload.domain_name.trim().to_lowercase();
    if domain_name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Domain name is required".to_string()));
    }

    // Basic domain validation (allow alphanumeric, dots, and hyphens)
    if !domain_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Domain name contains invalid characters".to_string(),
        ));
    }

    // Determine CNAME target based on project URL
    let cname_target = match &project.project_url {
        Some(url) => {
            // Extract hostname from project_url (e.g., "https://myapp.rise.dev" -> "myapp.rise.dev")
            url.trim_start_matches("http://")
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or(&project.name)
                .to_string()
        }
        None => {
            // Fall back to project name + default domain (this is a placeholder)
            format!("{}.rise.dev", project.name)
        }
    };

    info!(
        "Adding custom domain '{}' to project '{}' (user: {})",
        domain_name, project.name, user.email
    );

    // Create domain in database
    let domain = custom_domains::create(&state.db_pool, project.id, &domain_name, &cname_target)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key")
                || e.to_string().contains("unique constraint")
            {
                (
                    StatusCode::CONFLICT,
                    format!("Domain '{}' is already registered", domain_name),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to add domain: {}", e),
                )
            }
        })?;

    let api_domain = ApiCustomDomain::from(domain);

    let instructions = DomainSetupInstructions {
        cname_record: CnameRecord {
            name: domain_name.clone(),
            value: cname_target,
        },
        message: format!(
            "Please configure a CNAME record for '{}' pointing to the target. \
             Once configured, use the verify endpoint to validate the domain.",
            domain_name
        ),
    };

    Ok(Json(AddDomainResponse {
        domain: api_domain,
        instructions,
    }))
}

/// List all custom domains for a project
pub async fn list_domains(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_id_or_name): Path<String>,
    Query(params): Query<GetProjectParams>,
) -> Result<Json<Vec<ApiCustomDomain>>, (StatusCode, String)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &project_id_or_name, params.by_id)
        .await
        .map_err(|(status, json_err)| (status, json_err.error.clone()))?;

    // Check write permission (only project members can see domains)
    let can_write = check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check permissions: {}", e),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to view domains for this project".to_string(),
        ));
    }

    let domains = custom_domains::list_by_project(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list domains: {}", e),
            )
        })?;

    let api_domains: Vec<ApiCustomDomain> = domains.into_iter().map(ApiCustomDomain::from).collect();

    Ok(Json(api_domains))
}

/// Delete a custom domain
pub async fn delete_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain_name)): Path<(String, String)>,
    Query(params): Query<GetProjectParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &project_id_or_name, params.by_id)
        .await
        .map_err(|(status, json_err)| (status, json_err.error.clone()))?;

    // Check write permission
    let can_write = check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check permissions: {}", e),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to delete domains from this project".to_string(),
        ));
    }

    info!(
        "Deleting custom domain '{}' from project '{}' (user: {})",
        domain_name, project.name, user.email
    );

    // Get domain to verify it belongs to this project
    let domain = custom_domains::get_by_domain_name(&state.db_pool, &domain_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get domain: {}", e),
            )
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Domain not found".to_string()))?;

    // Verify domain belongs to this project
    if domain.project_id != project.id {
        return Err((
            StatusCode::FORBIDDEN,
            "Domain does not belong to this project".to_string(),
        ));
    }

    // Delete domain (cascades to challenges)
    custom_domains::delete(&state.db_pool, domain.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete domain: {}", e),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Verify domain ownership (check CNAME configuration)
pub async fn verify_domain(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain_name)): Path<(String, String)>,
    Query(params): Query<GetProjectParams>,
) -> Result<Json<VerifyDomainResponse>, (StatusCode, String)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &project_id_or_name, params.by_id)
        .await
        .map_err(|(status, json_err)| (status, json_err.error.clone()))?;

    // Check write permission
    let can_write = check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check permissions: {}", e),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to verify domains for this project".to_string(),
        ));
    }

    info!(
        "Verifying custom domain '{}' for project '{}' (user: {})",
        domain_name, project.name, user.email
    );

    // Get domain to verify it belongs to this project
    let domain = custom_domains::get_by_domain_name(&state.db_pool, &domain_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get domain: {}", e),
            )
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Domain not found".to_string()))?;

    // Verify domain belongs to this project
    if domain.project_id != project.id {
        return Err((
            StatusCode::FORBIDDEN,
            "Domain does not belong to this project".to_string(),
        ));
    }

    // Perform DNS lookup to verify CNAME
    let verification_result = verify_cname(&domain_name, &domain.cname_target).await;

    // Update domain verification status
    let new_status = if verification_result.success {
        crate::db::models::DomainVerificationStatus::Verified
    } else {
        crate::db::models::DomainVerificationStatus::Failed
    };

    custom_domains::update_verification_status(&state.db_pool, domain.id, new_status)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update verification status: {}", e),
            )
        })?;

    // Fetch updated domain
    let updated_domain = custom_domains::get_by_id(&state.db_pool, domain.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get updated domain: {}", e),
            )
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Domain not found".to_string()))?;

    Ok(Json(VerifyDomainResponse {
        domain: ApiCustomDomain::from(updated_domain),
        verification_result,
    }))
}

/// Get challenges for a domain
pub async fn get_challenges(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_id_or_name, domain_name)): Path<(String, String)>,
    Query(params): Query<GetProjectParams>,
) -> Result<Json<Vec<ApiAcmeChallenge>>, (StatusCode, String)> {
    // Resolve project by ID or name
    let project = resolve_project(&state, &project_id_or_name, params.by_id)
        .await
        .map_err(|(status, json_err)| (status, json_err.error.clone()))?;

    // Check write permission
    let can_write = check_write_permission(&state, &project, &user)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to check permissions: {}", e),
            )
        })?;

    if !can_write {
        return Err((
            StatusCode::FORBIDDEN,
            "You do not have permission to view challenges for this project".to_string(),
        ));
    }

    // Get domain to verify it belongs to this project
    let domain = custom_domains::get_by_domain_name(&state.db_pool, &domain_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get domain: {}", e),
            )
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Domain not found".to_string()))?;

    // Verify domain belongs to this project
    if domain.project_id != project.id {
        return Err((
            StatusCode::FORBIDDEN,
            "Domain does not belong to this project".to_string(),
        ));
    }

    let challenges = acme_challenges::list_by_domain(&state.db_pool, domain.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list challenges: {}", e),
            )
        })?;

    let api_challenges: Vec<ApiAcmeChallenge> =
        challenges.into_iter().map(ApiAcmeChallenge::from).collect();

    Ok(Json(api_challenges))
}

/// Verify CNAME record using DNS lookup
async fn verify_cname(domain_name: &str, expected_target: &str) -> VerificationResult {
    use trust_dns_resolver::TokioAsyncResolver;

    // Create resolver
    let resolver = match TokioAsyncResolver::tokio_from_system_conf() {
        Ok(r) => r,
        Err(e) => {
            return VerificationResult {
                success: false,
                message: format!("Failed to create DNS resolver: {}", e),
                expected_value: Some(expected_target.to_string()),
                actual_value: None,
            };
        }
    };

    // Verify domain resolves to the same IPs as the target
    // This works for both CNAME and A records
    let domain_lookup = match resolver.lookup_ip(domain_name).await {
        Ok(lookup) => lookup,
        Err(e) => {
            return VerificationResult {
                success: false,
                message: format!("DNS lookup failed for '{}': {}", domain_name, e),
                expected_value: Some(expected_target.to_string()),
                actual_value: None,
            };
        }
    };

    let target_lookup = match resolver.lookup_ip(expected_target).await {
        Ok(ips) => ips,
        Err(e) => {
            return VerificationResult {
                success: false,
                message: format!("Failed to resolve target '{}': {}", expected_target, e),
                expected_value: Some(expected_target.to_string()),
                actual_value: None,
            };
        }
    };

    // Compare IP addresses - domain should resolve to same IPs as target
    let domain_ips: Vec<_> = domain_lookup.iter().collect();
    let target_ips: Vec<_> = target_lookup.iter().collect();

    if domain_ips.is_empty() {
        return VerificationResult {
            success: false,
            message: format!("Domain '{}' does not resolve to any IP addresses", domain_name),
            expected_value: Some(format!("{:?}", target_ips)),
            actual_value: Some("[]".to_string()),
        };
    }

    // Check if at least one IP from domain matches target IPs
    let has_matching_ip = domain_ips.iter().any(|ip| target_ips.contains(ip));

    if has_matching_ip {
        VerificationResult {
            success: true,
            message: format!(
                "Domain verification successful - '{}' resolves to same IPs as '{}'",
                domain_name, expected_target
            ),
            expected_value: Some(format!("{:?}", target_ips)),
            actual_value: Some(format!("{:?}", domain_ips)),
        }
    } else {
        VerificationResult {
            success: false,
            message: format!(
                "Domain '{}' does not resolve to target '{}' - IP addresses don't match",
                domain_name, expected_target
            ),
            expected_value: Some(format!("{:?}", target_ips)),
            actual_value: Some(format!("{:?}", domain_ips)),
        }
    }
}
