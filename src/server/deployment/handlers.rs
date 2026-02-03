use anyhow::Context;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use regex::Regex;
use tracing::{debug, error, info};

use super::models::{self, *};
use super::state_machine;
use super::utils::{create_deployment_with_hooks, generate_deployment_id};
use crate::db::models::{DeploymentStatus as DbDeploymentStatus, User};
use crate::db::{
    deployments as db_deployments, projects, service_accounts, teams as db_teams, users,
};
use crate::server::registry::ImageTagType;
use crate::server::state::AppState;

/// Check if a user is an admin (based on email in config)
fn is_admin(state: &AppState, user_email: &str) -> bool {
    state.admin_users.contains(&user_email.to_string())
}

/// Check if user has permission to deploy to the project
async fn check_deploy_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user: &User,
) -> Result<(), String> {
    // Admins have full access
    if is_admin(state, &user.email) {
        return Ok(());
    }

    // Check if user is a service account for this project
    let is_sa = service_accounts::find_by_user_and_project(&state.db_pool, user.id, project.id)
        .await
        .map_err(|e| format!("Failed to check service account status: {}", e))?
        .is_some();

    if is_sa {
        return Ok(());
    }

    // If project is owned by the user directly, allow
    if let Some(owner_user_id) = project.owner_user_id {
        if owner_user_id == user.id {
            return Ok(());
        }
    }

    // If project is owned by a team, check if user is a member of that team
    if let Some(team_id) = project.owner_team_id {
        let is_member = db_teams::is_member(&state.db_pool, team_id, user.id)
            .await
            .map_err(|e| format!("Failed to check team membership: {}", e))?;

        if is_member {
            return Ok(());
        }

        let team = db_teams::find_by_id(&state.db_pool, team_id)
            .await
            .map_err(|e| format!("Failed to fetch team: {}", e))?
            .ok_or_else(|| "Team not found".to_string())?;

        return Err(format!(
            "You must be a member of team '{}' to deploy to this project",
            team.name
        ));
    }

    Err("You do not have permission to deploy to this project".to_string())
}

/// Validate group name format: must be 'default' or match [a-z0-9][a-z0-9/-]*[a-z0-9]
fn is_valid_group_name(name: &str) -> bool {
    if name == models::DEFAULT_DEPLOYMENT_GROUP {
        return true;
    }

    if name.len() > 100 {
        return false;
    }

    Regex::new(r"^[a-z0-9][a-z0-9/-]*[a-z0-9]$")
        .unwrap()
        .is_match(name)
}

/// Parse expiration duration string (e.g., "7d", "2h", "30m") to DateTime
fn parse_expiration(expires_in: &str) -> Result<DateTime<Utc>, String> {
    let s = expires_in.trim();
    let (num_str, unit) = if let Some(num_str) = s.strip_suffix('d') {
        (num_str, "d")
    } else if let Some(num_str) = s.strip_suffix('h') {
        (num_str, "h")
    } else if let Some(num_str) = s.strip_suffix('m') {
        (num_str, "m")
    } else {
        return Err("Duration must end with d, h, or m".to_string());
    };

    let num: i64 = num_str
        .parse()
        .map_err(|_| "Invalid number in duration".to_string())?;

    if num <= 0 {
        return Err("Duration must be positive".to_string());
    }

    let duration = match unit {
        "d" => chrono::Duration::days(num),
        "h" => chrono::Duration::hours(num),
        "m" => chrono::Duration::minutes(num),
        _ => return Err("Invalid duration unit".to_string()),
    };

    Ok(Utc::now() + duration)
}

/// Normalize image reference by adding registry hostname and namespace if missing
///
/// # Examples
/// - `nginx` → `docker.io/library/nginx`
/// - `nginx:latest` → `docker.io/library/nginx:latest`
/// - `myorg/app:v1` → `docker.io/myorg/app:v1`
/// - `quay.io/nginx:latest` → `quay.io/nginx:latest` (unchanged)
fn normalize_image_reference(image: &str) -> String {
    // Check if image already has a registry hostname (contains '.' or ':' before first '/')
    let has_registry = image
        .split('/')
        .next()
        .map(|first_part| first_part.contains('.') || first_part.contains(':'))
        .unwrap_or(false);

    if has_registry {
        // Already has registry, return as-is
        return image.to_string();
    }

    // No registry specified, default to docker.io
    // Check if image has a namespace (contains '/')
    if image.contains('/') {
        // Has namespace: myorg/app:v1 → docker.io/myorg/app:v1
        format!("docker.io/{}", image)
    } else {
        // No namespace: nginx → docker.io/library/nginx
        format!("docker.io/library/{}", image)
    }
}

/// Resolve image tag to digest by contacting OCI registry directly
///
/// This function uses the OCI Distribution API to fetch the image manifest
/// (without pulling the entire image) and returns the digest-pinned reference.
///
/// # Arguments
/// * `oci_client` - OCI client for registry interaction
/// * `registry_provider` - Registry provider for credentials
/// * `normalized_image` - Normalized image reference (e.g., "docker.io/library/nginx:latest")
///
/// # Returns
/// Fully-qualified digest reference (e.g., "docker.io/library/nginx@sha256:abc123...")
///
/// # Errors
/// Returns error if image doesn't exist, requires authentication, or registry is unreachable
async fn resolve_image_digest(
    oci_client: &crate::server::oci::OciClient,
    registry_provider: &std::sync::Arc<dyn crate::server::registry::RegistryProvider>,
    normalized_image: &str,
) -> anyhow::Result<String> {
    // Build credentials map from registry provider
    let mut credentials = crate::server::oci::RegistryCredentialsMap::new();

    match registry_provider.get_pull_credentials().await {
        Ok((user, pass)) if !user.is_empty() => {
            debug!(
                "Adding credentials for registry host: {}",
                registry_provider.registry_host()
            );
            credentials.insert(registry_provider.registry_host().to_string(), (user, pass));
        }
        Ok(_) => {
            debug!("Registry provider returned empty credentials, using anonymous auth");
        }
        Err(e) => {
            error!(
                "Failed to get pull credentials from registry provider: {}",
                e
            );
            // Continue with anonymous auth
        }
    }

    let digest_ref = oci_client
        .resolve_image_digest(normalized_image, &credentials)
        .await
        .context(format!("Failed to resolve image '{}'", normalized_image))?;

    info!("Resolved '{}' to digest '{}'", normalized_image, digest_ref);
    Ok(digest_ref)
}

/// Convert API DeploymentStatus to DB DeploymentStatus
fn convert_status_to_db(status: DeploymentStatus) -> DbDeploymentStatus {
    match status {
        DeploymentStatus::Pending => DbDeploymentStatus::Pending,
        DeploymentStatus::Building => DbDeploymentStatus::Building,
        DeploymentStatus::Pushing => DbDeploymentStatus::Pushing,
        DeploymentStatus::Pushed => DbDeploymentStatus::Pushed,
        DeploymentStatus::Deploying => DbDeploymentStatus::Deploying,
        DeploymentStatus::Healthy => DbDeploymentStatus::Healthy,
        DeploymentStatus::Unhealthy => DbDeploymentStatus::Unhealthy,
        DeploymentStatus::Cancelling => DbDeploymentStatus::Cancelling,
        DeploymentStatus::Cancelled => DbDeploymentStatus::Cancelled,
        DeploymentStatus::Terminating => DbDeploymentStatus::Terminating,
        DeploymentStatus::Stopped => DbDeploymentStatus::Stopped,
        DeploymentStatus::Superseded => DbDeploymentStatus::Superseded,
        DeploymentStatus::Failed => DbDeploymentStatus::Failed,
        DeploymentStatus::Expired => DbDeploymentStatus::Expired,
    }
}

/// Insert Rise-provided environment variables into a deployment
///
/// This function adds the following environment variables:
/// - RISE_PUBLIC_URL: Rise server URL (for API calls and browser redirects)
/// - RISE_ISSUER: The Rise backend URL (issuer of JWTs)
/// - RISE_APP_URL: Canonical URL where the app is accessible
/// - RISE_APP_URLS: JSON array of all URLs where the app can be accessed
///
/// These environment variables are visible in the Rise UI and allow deployed applications
/// to validate Rise-issued JWTs (via /.well-known/openid-configuration), call Rise APIs,
/// and know their own URLs.
async fn insert_rise_env_vars(
    state: &AppState,
    deployment: &crate::db::models::Deployment,
    project: &crate::db::models::Project,
) -> Result<(), (StatusCode, String)> {
    // 1. Insert RISE_PUBLIC_URL (Rise server URL for API calls and browser redirects)
    crate::db::env_vars::upsert_deployment_env_var(
        &state.db_pool,
        deployment.id,
        "RISE_PUBLIC_URL",
        &state.public_url,
        false, // Not a secret
        false, // is_retrievable
    )
    .await
    .map_err(|e| {
        error!("Failed to insert RISE_PUBLIC_URL env var: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to insert RISE_PUBLIC_URL: {}", e),
        )
    })?;

    // 2. Insert RISE_ISSUER
    crate::db::env_vars::upsert_deployment_env_var(
        &state.db_pool,
        deployment.id,
        "RISE_ISSUER",
        &state.public_url,
        false, // Not a secret
        false, // is_retrievable
    )
    .await
    .map_err(|e| {
        error!("Failed to insert RISE_ISSUER env var: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to insert RISE_ISSUER: {}", e),
        )
    })?;

    // 3. Generate RISE_APP_URL and RISE_APP_URLS
    let deployment_urls = state
        .deployment_backend
        .get_deployment_urls(deployment, project)
        .await
        .map_err(|e| {
            error!("Failed to get deployment URLs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get deployment URLs: {}", e),
            )
        })?;

    // First, determine RISE_APP_URL (canonical URL)
    // Use primary custom domain if set, otherwise use default project URL
    let canonical_url =
        match crate::db::custom_domains::get_primary_domain(&state.db_pool, project.id)
            .await
            .map_err(|e| {
                error!("Failed to get primary custom domain: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get primary custom domain: {}", e),
                )
            })? {
            Some(primary_domain) => {
                // Find the URL for this domain in custom_domain_urls
                deployment_urls
                    .custom_domain_urls
                    .iter()
                    .find(|url| url.contains(&primary_domain.domain))
                    .cloned()
                    .unwrap_or_else(|| deployment_urls.primary_url.clone())
            }
            None => deployment_urls.primary_url.clone(),
        };

    // Then build RISE_APP_URLS (all URLs)
    let mut app_urls = vec![deployment_urls.primary_url];
    app_urls.extend(deployment_urls.custom_domain_urls);

    let app_urls_json = serde_json::to_string(&app_urls).map_err(|e| {
        error!("Failed to serialize RISE_APP_URLS: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize RISE_APP_URLS: {}", e),
        )
    })?;

    // Insert RISE_APP_URLS
    crate::db::env_vars::upsert_deployment_env_var(
        &state.db_pool,
        deployment.id,
        "RISE_APP_URLS",
        &app_urls_json,
        false, // Not a secret
        false, // is_retrievable
    )
    .await
    .map_err(|e| {
        error!("Failed to insert RISE_APP_URLS env var: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to insert RISE_APP_URLS: {}", e),
        )
    })?;

    // Insert RISE_APP_URL
    crate::db::env_vars::upsert_deployment_env_var(
        &state.db_pool,
        deployment.id,
        "RISE_APP_URL",
        &canonical_url,
        false, // Not a secret
        false, // is_retrievable
    )
    .await
    .map_err(|e| {
        error!("Failed to insert RISE_APP_URL env var: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to insert RISE_APP_URL: {}", e),
        )
    })?;

    info!(
        "Inserted Rise environment variables for deployment {}",
        deployment.id
    );
    Ok(())
}

/// Convert DB DeploymentStatus to API DeploymentStatus
fn convert_status_from_db(status: DbDeploymentStatus) -> DeploymentStatus {
    match status {
        DbDeploymentStatus::Pending => DeploymentStatus::Pending,
        DbDeploymentStatus::Building => DeploymentStatus::Building,
        DbDeploymentStatus::Pushing => DeploymentStatus::Pushing,
        DbDeploymentStatus::Pushed => DeploymentStatus::Pushed,
        DbDeploymentStatus::Deploying => DeploymentStatus::Deploying,
        DbDeploymentStatus::Healthy => DeploymentStatus::Healthy,
        DbDeploymentStatus::Unhealthy => DeploymentStatus::Unhealthy,
        DbDeploymentStatus::Cancelling => DeploymentStatus::Cancelling,
        DbDeploymentStatus::Cancelled => DeploymentStatus::Cancelled,
        DbDeploymentStatus::Terminating => DeploymentStatus::Terminating,
        DbDeploymentStatus::Stopped => DeploymentStatus::Stopped,
        DbDeploymentStatus::Superseded => DeploymentStatus::Superseded,
        DbDeploymentStatus::Failed => DeploymentStatus::Failed,
        DbDeploymentStatus::Expired => DeploymentStatus::Expired,
    }
}

/// Fetch the creator email for a deployment
async fn get_creator_email(pool: &sqlx::PgPool, created_by_id: uuid::Uuid) -> String {
    match users::find_by_id(pool, created_by_id).await {
        Ok(Some(user)) => user.email,
        _ => "unknown".to_string(),
    }
}

/// Convert DB Deployment to API Deployment
///
/// Dynamically calculates the image tag when not stored in the database,
/// using the registry provider's configuration to construct the full image reference.
async fn convert_deployment(
    state: &AppState,
    deployment: crate::db::models::Deployment,
    project: &crate::db::models::Project,
    created_by_email: String,
    primary_url: Option<String>,
    custom_domain_urls: Vec<String>,
) -> Deployment {
    // Backfill image field for locally-built deployments
    // For pre-built images, deployment.image is already set
    // For build-from-source, calculate the internal registry tag
    let image = if deployment.image.is_some() {
        deployment.image.clone()
    } else {
        Some(super::utils::get_deployment_image_tag(state, &deployment, project).await)
    };

    Deployment {
        id: deployment.id.to_string(),
        deployment_id: deployment.deployment_id,
        project: deployment.project_id.to_string(),
        created_by: deployment.created_by_id.to_string(),
        created_by_email,
        status: convert_status_from_db(deployment.status),
        deployment_group: deployment.deployment_group,
        expires_at: deployment.expires_at.map(|dt| dt.to_rfc3339()),
        error_message: deployment.error_message,
        completed_at: deployment.completed_at.map(|dt| dt.to_rfc3339()),
        build_logs: deployment.build_logs,
        controller_metadata: deployment.controller_metadata,
        primary_url,
        custom_domain_urls,
        image,
        image_digest: deployment.image_digest,
        http_port: deployment.http_port as u16,
        is_active: deployment.is_active,
        created: deployment.created_at.to_rfc3339(),
        updated: deployment.updated_at.to_rfc3339(),
    }
}

/// Resolve the effective HTTP port for a deployment.
///
/// Priority:
/// 1. Explicit http_port from request (if provided)
/// 2. PORT env var from project (if set and valid)
/// 3. Default: 8080
async fn resolve_effective_http_port(
    state: &AppState,
    project_id: uuid::Uuid,
    explicit_port: Option<u16>,
) -> Result<u16, (StatusCode, String)> {
    // 1. Explicit port takes precedence
    if let Some(port) = explicit_port {
        return Ok(port);
    }

    // 2. Check project's PORT env var
    let project_env_vars = crate::db::env_vars::list_project_env_vars(&state.db_pool, project_id)
        .await
        .map_err(|e| {
            error!("Failed to list project env vars: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list project environment variables: {}", e),
            )
        })?;

    if let Some(port_var) = project_env_vars.iter().find(|v| v.key == "PORT") {
        if let Ok(port) = port_var.value.parse::<u16>() {
            if port > 0 {
                debug!("Using PORT {} from project environment variable", port);
                return Ok(port);
            }
        }
        // Invalid PORT value - warn but fall through to default
        debug!(
            "Project PORT env var '{}' is not a valid port number, using default",
            port_var.value
        );
    }

    // 3. Default to 8080
    debug!("No explicit port or PORT env var, defaulting to 8080");
    Ok(8080)
}

/// POST /deployments - Create a new deployment
pub async fn create_deployment(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Json(payload): Json<CreateDeploymentRequest>,
) -> Result<Json<CreateDeploymentResponse>, (StatusCode, String)> {
    info!("Creating deployment for project '{}'", payload.project);

    // Validate deployment group name
    if !is_valid_group_name(&payload.group) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid group name '{}'. Must be 'default' or match pattern [a-z0-9][a-z0-9/-]*[a-z0-9] (max 100 chars)",
                payload.group
            ),
        ));
    }

    // Validate http_port if provided (should be 1-65535)
    if let Some(port) = payload.http_port {
        if port == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                "HTTP port must be between 1 and 65535".to_string(),
            ));
        }
    }

    // Parse expiration duration if provided
    let expires_at = if let Some(ref expires_in) = payload.expires_in {
        Some(parse_expiration(expires_in).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid expiration duration '{}': {}", expires_in, e),
            )
        })?)
    } else {
        None
    };

    // Query project by name
    let project = projects::find_by_name(&state.db_pool, &payload.project)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", payload.project),
            )
        })?;

    // Prevent deployments on projects in deletion lifecycle
    // Projects in Deleting or Terminated status should not accept new deployments
    if matches!(
        project.status,
        crate::db::models::ProjectStatus::Deleting | crate::db::models::ProjectStatus::Terminated
    ) {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot create deployment for project in {:?} state",
                project.status
            ),
        ));
    }

    // Check deployment permissions
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", payload.project),
            )
        })?;

    // Generate deployment ID
    let deployment_id = generate_deployment_id();
    debug!("Generated deployment ID: {}", deployment_id);

    // Resolve effective http_port:
    // 1. Explicit http_port from request (if provided)
    // 2. Source deployment's http_port (if --from is used, handled below)
    // 3. PORT env var from project (if set and valid)
    // 4. Default: 8080
    let effective_http_port =
        resolve_effective_http_port(&state, project.id, payload.http_port).await?;
    info!(
        "Using http_port {} for deployment {}",
        effective_http_port, deployment_id
    );

    // Handle deployment creation from an existing deployment (redeploy/rollback)
    if let Some(ref from_deployment_id) = payload.from_deployment {
        info!(
            "Creating deployment from existing deployment '{}'",
            from_deployment_id
        );

        // Find the source deployment
        let source_deployment = db_deployments::find_by_project_and_deployment_id(
            &state.db_pool,
            project.id,
            from_deployment_id,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find source deployment: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!(
                    "Source deployment '{}' not found for project '{}'",
                    from_deployment_id, payload.project
                ),
            )
        })?;

        // Verify source deployment is in a valid state for creating from
        // Allow Healthy (currently active), Superseded (previously active), and Stopped
        if !state_machine::is_rollbackable(&source_deployment.status) {
            return Err((StatusCode::BAD_REQUEST, format!("Cannot create deployment from '{}' with status '{:?}'. Only Healthy, Superseded, or Stopped deployments can be used as source.", from_deployment_id, source_deployment.status)));
        }

        // For chained redeployments, follow the chain to find the original source
        // This ensures we use the correct image tag from the original deployment that built the image
        let original_source_id =
            if let Some(chained_source) = source_deployment.rolled_back_from_deployment_id {
                // Source is itself a redeploy - use its source instead
                debug!(
                    "Source deployment {} is a redeploy, following chain to original source {}",
                    from_deployment_id, chained_source
                );
                chained_source
            } else {
                // Source is the original - use it directly
                source_deployment.id
            };

        // Determine http_port for the new deployment:
        // - If explicit http_port was provided in request, use it (already in effective_http_port)
        // - If no explicit port, inherit from source deployment
        let final_http_port = if payload.http_port.is_some() {
            effective_http_port
        } else {
            source_deployment.http_port as u16
        };

        // Create new deployment with Pushed status and invoke extension hooks
        // Copy image and image_digest from source - the helper function will determine the tag
        // For pre-built images: image_digest is copied, helper returns it
        // For build-from-source: rolled_back_from_deployment_id is used to find the original source deployment's image
        let new_deployment = create_deployment_with_hooks(
            &state,
            db_deployments::CreateDeploymentParams {
                deployment_id: &deployment_id,
                project_id: project.id,
                created_by_id: user.id,
                status: DbDeploymentStatus::Pushed, // Start in Pushed state so controller picks it up
                image: source_deployment.image.as_deref(), // Copy image from source if present
                image_digest: source_deployment.image_digest.as_deref(), // Copy digest from source if present
                rolled_back_from_deployment_id: Some(original_source_id), // Track original source for image tag calculation
                deployment_group: &payload.group, // Use requested group (may be different from source)
                expires_at,                       // expires_at
                http_port: final_http_port as i32, // Use determined http_port
                is_active: false,                 // Deployments start as inactive
            },
            &project,
        )
        .await?;

        // Handle environment variables based on use_source_env_vars flag
        if payload.use_source_env_vars {
            // Copy environment variables from source deployment
            info!(
                "Copying environment variables from source deployment '{}'",
                from_deployment_id
            );
            crate::db::env_vars::copy_deployment_env_vars_to_deployment(
                &state.db_pool,
                source_deployment.id,
                new_deployment.id,
            )
            .await
            .map_err(|e| {
                error!(
                    "Failed to copy environment variables from deployment {}: {}",
                    from_deployment_id, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to copy environment variables: {}", e),
                )
            })?;
        } else {
            // Copy current project environment variables to deployment
            info!("Using current project environment variables");
            crate::db::env_vars::copy_project_env_vars_to_deployment(
                &state.db_pool,
                project.id,
                new_deployment.id,
            )
            .await
            .map_err(|e| {
                error!(
                    "Failed to copy environment variables for deployment {}: {}",
                    deployment_id, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to copy environment variables: {}", e),
                )
            })?;
        }

        // Upsert PORT env var with the final http_port value
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            new_deployment.id,
            "PORT",
            &final_http_port.to_string(),
            false, // not a secret
            false, // is_retrievable
        )
        .await
        .map_err(|e| {
            error!("Failed to insert PORT env var: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to insert PORT env var: {}", e),
            )
        })?;

        // Insert Rise-provided environment variables
        insert_rise_env_vars(&state, &new_deployment, &project).await?;

        // Use helper to determine image tag (for logging/response only)
        let image_tag = crate::server::deployment::utils::get_deployment_image_tag(
            &state,
            &new_deployment,
            &project,
        )
        .await;

        info!(
            "Created deployment {} from {} (image: {}, env vars: {})",
            deployment_id,
            from_deployment_id,
            image_tag,
            if payload.use_source_env_vars {
                "from source"
            } else {
                "from project"
            }
        );

        // Return response with image tag and empty credentials (no push needed)
        return Ok(Json(CreateDeploymentResponse {
            deployment_id,
            image_tag,
            credentials: crate::server::registry::models::RegistryCredentials {
                registry_url: String::new(),
                username: String::new(),
                password: String::new(),
                expires_in: None,
            },
        }));
    }

    // Branch based on whether user provided a pre-built image
    if let Some(ref user_image) = payload.image {
        // Path 1: Pre-built image deployment
        info!("Creating deployment with pre-built image: {}", user_image);

        // Normalize image reference (add registry and namespace if missing)
        let normalized_image = normalize_image_reference(user_image);
        info!(
            "Normalized image reference: {} -> {}",
            user_image, normalized_image
        );

        // Resolve image to digest
        info!("Resolving image '{}' to digest...", normalized_image);
        let image_digest = resolve_image_digest(
            &state.oci_client,
            &state.registry_provider,
            &normalized_image,
        )
        .await
        .map_err(|e| {
            error!(
                "Failed to resolve image '{}' (normalized from '{}'): {}",
                normalized_image, user_image, e
            );
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to resolve image '{}': {}", user_image, e),
            )
        })?;

        info!("Successfully resolved image to digest: {}", image_digest);

        // Create deployment record with image fields set and invoke extension hooks
        let deployment = create_deployment_with_hooks(
            &state,
            db_deployments::CreateDeploymentParams {
                deployment_id: &deployment_id,
                project_id: project.id,
                created_by_id: user.id,
                status: DbDeploymentStatus::Pushed, // Pre-built images skip build/push, go straight to Pushed
                image: Some(user_image),            // Store original user input
                image_digest: Some(&image_digest),  // Store resolved digest
                rolled_back_from_deployment_id: None, // Not a rollback
                deployment_group: &payload.group,   // deployment_group
                expires_at,                         // expires_at
                http_port: effective_http_port as i32, // http_port
                is_active: false,                   // Deployments start as inactive
            },
            &project,
        )
        .await?;

        info!(
            "Created pre-built image deployment {} for project {}",
            deployment_id, payload.project
        );

        // Copy project environment variables to deployment
        crate::db::env_vars::copy_project_env_vars_to_deployment(
            &state.db_pool,
            project.id,
            deployment.id,
        )
        .await
        .map_err(|e| {
            error!(
                "Failed to copy environment variables for deployment {}: {}",
                deployment_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to copy environment variables: {}", e),
            )
        })?;

        // Upsert PORT env var with the resolved effective value
        // This overwrites any user-set PORT with the resolved value (which may be the same)
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            deployment.id,
            "PORT",
            &effective_http_port.to_string(),
            false, // not a secret
            false, // is_retrievable
        )
        .await
        .map_err(|e| {
            error!("Failed to insert PORT env var: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to insert PORT env var: {}", e),
            )
        })?;

        // Insert Rise-provided environment variables
        insert_rise_env_vars(&state, &deployment, &project).await?;

        // Return response with digest as image_tag and empty credentials
        Ok(Json(CreateDeploymentResponse {
            deployment_id,
            image_tag: image_digest, // Return digest for consistency
            credentials: crate::server::registry::models::RegistryCredentials {
                registry_url: String::new(),
                username: String::new(),
                password: String::new(),
                expires_in: None,
            },
        }))
    } else {
        // Path 2: Build from source (current behavior)
        // Get registry credentials
        let credentials = state
            .registry_provider
            .get_credentials(&payload.project)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get credentials: {}", e),
                )
            })?;

        // Get full image tag from provider for CLI client (uses client_registry_url if configured)
        let image_tag = state.registry_provider.get_image_tag(
            &payload.project,
            &deployment_id,
            ImageTagType::ClientFacing,
        );

        debug!("Image tag: {}", image_tag);

        // Create deployment record in database and invoke extension hooks
        let deployment = create_deployment_with_hooks(
            &state,
            db_deployments::CreateDeploymentParams {
                deployment_id: &deployment_id,
                project_id: project.id,
                created_by_id: user.id,
                status: DbDeploymentStatus::Pending,
                image: None,        // image - NULL for build-from-source
                image_digest: None, // image_digest - NULL for build-from-source
                rolled_back_from_deployment_id: None, // Not a rollback
                deployment_group: &payload.group, // deployment_group
                expires_at,         // expires_at
                http_port: effective_http_port as i32, // http_port
                is_active: false,   // Deployments start as inactive
            },
            &project,
        )
        .await?;

        info!(
            "Created build-from-source deployment {} for project {}",
            deployment_id, payload.project
        );

        // Copy project environment variables to deployment
        crate::db::env_vars::copy_project_env_vars_to_deployment(
            &state.db_pool,
            project.id,
            deployment.id,
        )
        .await
        .map_err(|e| {
            error!(
                "Failed to copy environment variables for deployment {}: {}",
                deployment_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to copy environment variables: {}", e),
            )
        })?;

        // Upsert PORT env var with the resolved effective value
        // This overwrites any user-set PORT with the resolved value (which may be the same)
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            deployment.id,
            "PORT",
            &effective_http_port.to_string(),
            false, // not a secret
            false, // is_retrievable
        )
        .await
        .map_err(|e| {
            error!("Failed to insert PORT env var: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to insert PORT env var: {}", e),
            )
        })?;

        // Insert Rise-provided environment variables
        insert_rise_env_vars(&state, &deployment, &project).await?;

        // Return response
        Ok(Json(CreateDeploymentResponse {
            deployment_id,
            image_tag,
            credentials,
        }))
    }
}

/// PATCH /deployments/{deployment_id}/status - Update deployment status
pub async fn update_deployment_status(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(deployment_id): Path<String>,
    Json(payload): Json<UpdateDeploymentStatusRequest>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    info!(
        "Updating deployment {} status to {:?}",
        deployment_id, payload.status
    );

    // Find all deployments with this deployment_id (there should only be one)
    // We need to find the project first to check the deployment_id
    // For now, let's query by deployment_id across all projects
    // We'll need to add a function to find by deployment_id only

    // Query all projects to find the one with this deployment
    let all_projects = projects::list(&state.db_pool, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list projects: {}", e),
        )
    })?;

    let mut found_deployment: Option<crate::db::models::Deployment> = None;
    let mut found_project: Option<crate::db::models::Project> = None;

    for project in all_projects {
        if let Ok(Some(deployment)) =
            db_deployments::find_by_deployment_id(&state.db_pool, &deployment_id, project.id).await
        {
            found_deployment = Some(deployment);
            found_project = Some(project);
            break;
        }
    }

    let deployment = found_deployment.ok_or((
        StatusCode::NOT_FOUND,
        format!("Deployment '{}' not found", deployment_id),
    ))?;

    let project = found_project.ok_or((
        StatusCode::NOT_FOUND,
        format!("Project for deployment '{}' not found", deployment_id),
    ))?;

    // Check if user has permission (owns the project)
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Deployment '{}' not found", deployment_id),
            )
        })?;

    // Update status in database
    let status_copy = payload.status.clone();
    let updated_deployment = match payload.status {
        DeploymentStatus::Failed => {
            let error_msg = payload.error_message.as_deref().unwrap_or("Unknown error");
            let deployment = db_deployments::mark_failed(&state.db_pool, deployment.id, error_msg)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to update deployment: {}", e),
                    )
                })?;

            // Update project status to Failed
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to update project status: {}", e),
                    )
                })?;

            deployment
        }
        _ => {
            // update_status will validate the state transition
            let db_status = convert_status_to_db(payload.status);
            let deployment =
                db_deployments::update_status(&state.db_pool, deployment.id, db_status)
                    .await
                    .map_err(|e| {
                        // State transition validation errors are returned as anyhow errors
                        // Return BAD_REQUEST for validation errors, INTERNAL_SERVER_ERROR otherwise
                        let error_msg = e.to_string();
                        if error_msg.contains("Invalid deployment state transition") {
                            (StatusCode::BAD_REQUEST, error_msg)
                        } else {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Failed to update deployment: {}", e),
                            )
                        }
                    })?;

            // Update project status (e.g., to Deploying)
            projects::update_calculated_status(&state.db_pool, project.id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to update project status: {}", e),
                    )
                })?;

            deployment
        }
    };

    info!(
        "Updated deployment {} to status {:?}",
        deployment_id, status_copy
    );

    // Only calculate URLs for non-terminal deployments that could receive traffic
    let (primary_url, custom_domain_urls) =
        if state_machine::is_terminal(&updated_deployment.status) {
            // Terminal deployments (Failed, Stopped, Cancelled, Superseded, Expired) cannot receive traffic
            (None, vec![])
        } else {
            // Calculate deployment URLs dynamically for active deployments
            match state
                .deployment_backend
                .get_deployment_urls(&updated_deployment, &project)
                .await
            {
                Ok(urls) => (Some(urls.primary_url), urls.custom_domain_urls),
                Err(e) => {
                    error!(
                        "Failed to calculate URLs for deployment {}: {}",
                        deployment_id, e
                    );
                    (None, vec![])
                }
            }
        };

    let created_by_email =
        get_creator_email(&state.db_pool, updated_deployment.created_by_id).await;
    Ok(Json(
        convert_deployment(
            &state,
            updated_deployment,
            &project,
            created_by_email,
            primary_url,
            custom_domain_urls,
        )
        .await,
    ))
}

/// Query parameters for listing deployments
#[derive(Debug, serde::Deserialize)]
pub struct ListDeploymentsQuery {
    #[serde(rename = "group")]
    pub deployment_group: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// List deployments for a project
pub async fn list_deployments(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
    Query(query): Query<ListDeploymentsQuery>,
) -> Result<Json<Vec<Deployment>>, (StatusCode, String)> {
    debug!(
        "Listing deployments for project: {} (group: {:?})",
        project_name, query.deployment_group
    );

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to view deployments
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Get deployments from database (optionally filtered by group, with pagination)
    let db_deployments = db_deployments::list_for_project_and_group(
        &state.db_pool,
        project.id,
        query.deployment_group.as_deref(),
        query.limit,
        query.offset,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list deployments: {}", e),
        )
    })?;

    // Convert to API models (fetch creator emails and calculate URLs)
    let mut deployments = Vec::with_capacity(db_deployments.len());
    for db_deployment in db_deployments {
        let created_by_email = get_creator_email(&state.db_pool, db_deployment.created_by_id).await;

        // Only calculate URLs for non-terminal deployments that could receive traffic
        let (primary_url, custom_domain_urls) = if state_machine::is_terminal(&db_deployment.status)
        {
            // Terminal deployments (Failed, Stopped, Cancelled, Superseded, Expired) cannot receive traffic
            (None, vec![])
        } else {
            // Calculate deployment URLs dynamically for active deployments
            match state
                .deployment_backend
                .get_deployment_urls(&db_deployment, &project)
                .await
            {
                Ok(urls) => (Some(urls.primary_url), urls.custom_domain_urls),
                Err(e) => {
                    error!(
                        "Failed to calculate URLs for deployment {}: {}",
                        db_deployment.deployment_id, e
                    );
                    (None, vec![])
                }
            }
        };

        deployments.push(
            convert_deployment(
                &state,
                db_deployment,
                &project,
                created_by_email,
                primary_url,
                custom_domain_urls,
            )
            .await,
        );
    }

    Ok(Json(deployments))
}

/// Query parameters for stopping deployments
#[derive(Debug, serde::Deserialize)]
pub struct StopDeploymentsQuery {
    pub group: String,
}

/// Response for stopping deployments
#[derive(Debug, serde::Serialize)]
pub struct StopDeploymentsResponse {
    pub stopped_count: usize,
    pub deployment_ids: Vec<String>,
}

/// POST /projects/{project_name}/deployments/stop - Stop all deployments in a group
pub async fn stop_deployments_by_group(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
    Query(query): Query<StopDeploymentsQuery>,
) -> Result<Json<StopDeploymentsResponse>, (StatusCode, String)> {
    info!(
        "Stopping all deployments in group '{}' for project '{}'",
        query.group, project_name
    );

    // Validate group name
    if !is_valid_group_name(&query.group) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid group name '{}'. Must be 'default' or match pattern [a-z0-9][a-z0-9/-]*[a-z0-9]",
                query.group
            ),
        ));
    }

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to stop deployments (owns the project)
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Find all non-terminal deployments in this group
    let deployments = db_deployments::find_non_terminal_for_project_and_group(
        &state.db_pool,
        project.id,
        &query.group,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to find deployments: {}", e),
        )
    })?;

    let mut stopped_ids = Vec::new();

    // Mark each deployment as Terminating with UserStopped reason
    for deployment in deployments {
        match db_deployments::mark_terminating(
            &state.db_pool,
            deployment.id,
            crate::db::models::TerminationReason::UserStopped,
        )
        .await
        {
            Ok(_) => {
                info!(
                    "Marked deployment {} as Terminating",
                    deployment.deployment_id
                );
                stopped_ids.push(deployment.deployment_id);
            }
            Err(e) => {
                error!(
                    "Failed to mark deployment {} as Terminating: {}",
                    deployment.deployment_id, e
                );
            }
        }
    }

    // Update project status
    projects::update_calculated_status(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update project status: {}", e),
            )
        })?;

    info!(
        "Stopped {} deployments in group '{}' for project '{}'",
        stopped_ids.len(),
        query.group,
        project_name
    );

    Ok(Json(StopDeploymentsResponse {
        stopped_count: stopped_ids.len(),
        deployment_ids: stopped_ids,
    }))
}

/// POST /projects/{project_name}/deployments/{deployment_id}/stop - Stop a specific deployment
pub async fn stop_deployment(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, deployment_id)): Path<(String, String)>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    info!(
        "Stopping deployment '{}' for project '{}'",
        deployment_id, project_name
    );

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to stop deployments
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Find the specific deployment
    let deployment =
        db_deployments::find_by_deployment_id(&state.db_pool, &deployment_id, project.id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to find deployment: {}", e),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    format!("Deployment '{}' not found", deployment_id),
                )
            })?;

    // Check if deployment is already in a terminal state
    if state_machine::is_terminal(&deployment.status) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Deployment '{}' is already in terminal state: {}",
                deployment_id, deployment.status
            ),
        ));
    }

    // Mark deployment as Terminating with UserStopped reason
    let updated_deployment = db_deployments::mark_terminating(
        &state.db_pool,
        deployment.id,
        crate::db::models::TerminationReason::UserStopped,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to stop deployment: {}", e),
        )
    })?;

    info!("Marked deployment {} as Terminating", deployment_id);

    // Update project status
    projects::update_calculated_status(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update project status: {}", e),
            )
        })?;

    // Calculate deployment URLs dynamically
    let (primary_url, custom_domain_urls) = match state
        .deployment_backend
        .get_deployment_urls(&updated_deployment, &project)
        .await
    {
        Ok(urls) => (Some(urls.primary_url), urls.custom_domain_urls),
        Err(e) => {
            error!(
                "Failed to calculate URLs for deployment {}: {}",
                deployment_id, e
            );
            (None, vec![])
        }
    };

    // Get creator email and convert to API model
    let created_by_email =
        get_creator_email(&state.db_pool, updated_deployment.created_by_id).await;
    Ok(Json(
        convert_deployment(
            &state,
            updated_deployment,
            &project,
            created_by_email,
            primary_url,
            custom_domain_urls,
        )
        .await,
    ))
}

/// GET /projects/{project_name}/deployments/{deployment_id} - Get a specific deployment
pub async fn get_deployment_by_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, deployment_id)): Path<(String, String)>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    debug!(
        "Getting deployment {} for project {}",
        deployment_id, project_name
    );

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to view deployments
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Find deployment by project_id and deployment_id
    let deployment = db_deployments::find_by_project_and_deployment_id(
        &state.db_pool,
        project.id,
        &deployment_id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to find deployment: {}", e),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "Deployment '{}' not found for project '{}'",
                deployment_id, project_name
            ),
        )
    })?;

    // Only calculate URLs for non-terminal deployments that could receive traffic
    let (primary_url, custom_domain_urls) = if state_machine::is_terminal(&deployment.status) {
        // Terminal deployments (Failed, Stopped, Cancelled, Superseded, Expired) cannot receive traffic
        (None, vec![])
    } else {
        // Calculate deployment URLs dynamically for active deployments
        match state
            .deployment_backend
            .get_deployment_urls(&deployment, &project)
            .await
        {
            Ok(urls) => (Some(urls.primary_url), urls.custom_domain_urls),
            Err(e) => {
                error!(
                    "Failed to calculate URLs for deployment {}: {}",
                    deployment_id, e
                );
                (None, vec![])
            }
        }
    };

    let created_by_email = get_creator_email(&state.db_pool, deployment.created_by_id).await;
    Ok(Json(
        convert_deployment(
            &state,
            deployment,
            &project,
            created_by_email,
            primary_url,
            custom_domain_urls,
        )
        .await,
    ))
}

/// GET /projects/{project_name}/deployment-groups - List all deployment groups for a project
pub async fn list_deployment_groups(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(project_name): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    debug!("Listing deployment groups for project: {}", project_name);

    // Find the project by name
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to find project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check if user has permission to view deployment groups
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Get deployment groups from database
    let groups = db_deployments::get_all_deployment_groups(&state.db_pool, project.id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list deployment groups: {}", e),
            )
        })?;

    Ok(Json(groups))
}

/// Query parameters for log streaming
#[derive(serde::Deserialize)]
pub struct LogStreamParams {
    /// Follow the logs (stream continuously)
    #[serde(default)]
    pub follow: bool,
    /// Number of lines to show from the end
    pub tail: Option<i64>,
    /// Include timestamps in the output
    #[serde(default)]
    pub timestamps: bool,
    /// Show logs since this many seconds ago
    pub since: Option<i64>,
}

/// Stream logs from a deployment via Server-Sent Events
///
/// GET /projects/{project_name}/deployments/{deployment_id}/logs
pub async fn stream_deployment_logs(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, deployment_id)): Path<(String, String)>,
    Query(params): Query<LogStreamParams>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, anyhow::Error>>>, (StatusCode, String)> {
    // Fetch project
    let project = projects::find_by_name(&state.db_pool, &project_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch project: {}", e),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Project '{}' not found", project_name),
            )
        })?;

    // Check permission
    check_deploy_permission(&state, &project, &user)
        .await
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // Fetch deployment
    let deployment = db_deployments::find_by_project_and_deployment_id(
        &state.db_pool,
        project.id,
        &deployment_id,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to fetch deployment: {}", e),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "Deployment '{}' not found for project '{}'",
                deployment_id, project_name
            ),
        )
    })?;

    // Check if deployment is in a state where logs make sense
    if state_machine::is_terminal(&deployment.status) {
        return Err((
            StatusCode::GONE,
            "Deployment is no longer running - logs may not be available".to_string(),
        ));
    }

    // Don't allow streaming logs from deployments that haven't reached Deploying yet
    if matches!(
        deployment.status,
        DbDeploymentStatus::Pending
            | DbDeploymentStatus::Building
            | DbDeploymentStatus::Pushing
            | DbDeploymentStatus::Pushed
    ) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Deployment not ready yet - no logs available. Try again when deployment is running."
                .to_string(),
        ));
    }

    // Get log stream from deployment backend
    // Default to last 1000 lines if tail not specified
    let tail = params.tail.or(Some(1000));

    let log_stream = state
        .deployment_backend
        .stream_logs(
            &deployment,
            params.follow,
            tail,
            params.timestamps,
            params.since,
        )
        .await
        .map_err(|e| {
            let error_msg = e.to_string();
            if error_msg.contains("Pod not found") || error_msg.contains("not ready yet") {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Deployment pod not ready yet. Please try again in a moment.".to_string(),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to stream logs: {}", e),
                )
            }
        })?;

    // Convert log stream to SSE events
    // We need to flatten the stream since each chunk may contain multiple lines
    use futures::stream;
    let sse_stream = log_stream.flat_map(|result| match result {
        Ok(bytes) => {
            // Convert bytes to string (log lines)
            let log_text = String::from_utf8_lossy(&bytes).to_string();
            // Split into individual lines and create an event for each
            let events: Vec<Result<Event, anyhow::Error>> = log_text
                .lines()
                .filter(|line| !line.is_empty())
                .map(|line| Ok(Event::default().data(line)))
                .collect();
            stream::iter(events)
        }
        Err(e) => {
            // Send error as SSE event
            error!("Log stream error: {}", e);
            stream::iter(vec![Err(e)])
        }
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}
