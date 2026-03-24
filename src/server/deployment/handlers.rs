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
use tracing::{debug, error, info, warn};

use super::models::{self, *};
use super::state_machine;
use super::utils::{create_deployment_with_hooks, generate_deployment_id};
use crate::db::models::{DeploymentStatus as DbDeploymentStatus, User};
use crate::db::{
    deployments as db_deployments, projects, service_accounts, teams as db_teams, users,
};
use crate::server::registry::ImageTagType;
use crate::server::state::AppState;

/// Check if user has permission to deploy to the project
async fn check_deploy_permission(
    state: &AppState,
    project: &crate::db::models::Project,
    user: &User,
) -> Result<(), String> {
    // Admins have full access
    if state.is_admin(&user.email) {
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
/// with additional constraints:
/// - No consecutive hyphens (`--`) to avoid collisions with normalized names
///   (e.g. `mr/123` normalizes to `mr--123`, so `mr--123` as a raw group name is disallowed)
/// - Normalized result must be <= 63 chars (Kubernetes label value limit)
fn is_valid_group_name(name: &str) -> bool {
    if name == models::DEFAULT_DEPLOYMENT_GROUP {
        return true;
    }

    if name.len() > 100 {
        return false;
    }

    // Disallow consecutive hyphens to prevent collisions with normalized names
    if name.contains("--") {
        return false;
    }

    let valid_pattern = Regex::new(r"^[a-z0-9][a-z0-9/-]*[a-z0-9]$")
        .unwrap()
        .is_match(name);

    if !valid_pattern {
        return false;
    }

    // Ensure the normalized result fits within Kubernetes label value limit (63 chars)
    let normalized = models::normalize_deployment_group(name);
    if normalized.len() > 63 {
        return false;
    }

    true
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

/// Insert Rise-provided environment variables into a deployment.
///
/// Inserts the system env vars generated by [`models::rise_system_env_vars`]:
/// RISE_ISSUER, RISE_APP_URL, RISE_APP_URLS, RISE_DEPLOYMENT_GROUP, RISE_DEPLOYMENT_GROUP_NORMALIZED.
///
/// These environment variables are visible in the Rise UI and allow deployed applications
/// to validate Rise-issued JWTs (via /.well-known/openid-configuration), call Rise APIs,
/// and know their own URLs and deployment context.
async fn insert_rise_env_vars(
    state: &AppState,
    deployment: &crate::db::models::Deployment,
    project: &crate::db::models::Project,
) -> Result<(), (StatusCode, String)> {
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

    let vars = models::rise_system_env_vars(
        &state.public_url,
        &deployment.deployment_group,
        &deployment_urls,
    );

    for (key, value) in &vars {
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            deployment.id,
            key,
            value,
            false, // Not a secret
            false, // is_protected
        )
        .await
        .map_err(|e| {
            error!("Failed to insert {} env var: {}", key, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to insert {}: {}", key, e),
            )
        })?;
    }

    info!(
        "Inserted Rise environment variables for deployment {}",
        deployment.id
    );
    Ok(())
}

/// Apply env var overrides from the deployment request.
///
/// Encrypts secret values and upserts each override into the deployment's env vars.
/// Called after copying project/source env vars and before upserting PORT.
async fn apply_env_overrides(
    state: &AppState,
    deployment_id: uuid::Uuid,
    overrides: &[models::EnvOverride],
) -> Result<(), (StatusCode, String)> {
    if overrides.is_empty() {
        return Ok(());
    }

    info!(
        "Applying {} env override(s) to deployment {}",
        overrides.len(),
        deployment_id
    );

    for env_override in overrides {
        let is_protected = validate_env_override(env_override)?;

        // Encrypt if secret
        let value_to_store = if env_override.is_secret {
            let provider = state.encryption_provider.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Cannot store secret variables: no encryption provider configured".to_string(),
                )
            })?;
            provider.encrypt(&env_override.value).await.map_err(|e| {
                error!(
                    "Failed to encrypt env override '{}': {:?}",
                    env_override.key, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to encrypt secret '{}': {}", env_override.key, e),
                )
            })?
        } else {
            env_override.value.clone()
        };

        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            deployment_id,
            &env_override.key,
            &value_to_store,
            env_override.is_secret,
            is_protected,
        )
        .await
        .map_err(|e| {
            error!(
                "Failed to upsert env override '{}': {}",
                env_override.key, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set env override '{}': {}", env_override.key, e),
            )
        })?;
    }

    Ok(())
}

fn validate_env_override_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn normalize_env_override_is_protected(env_override: &models::EnvOverride) -> bool {
    env_override.is_protected.unwrap_or(env_override.is_secret)
}

fn validate_env_override(env_override: &models::EnvOverride) -> Result<bool, (StatusCode, String)> {
    if !validate_env_override_key(&env_override.key) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid env var key '{}' (must be alphanumeric with underscores)",
                env_override.key
            ),
        ));
    }

    if env_override.key == "PORT" {
        return Err((
            StatusCode::BAD_REQUEST,
            "PORT cannot be set via env overrides. Use http_port/--http-port instead.".to_string(),
        ));
    }

    let is_protected = normalize_env_override_is_protected(env_override);
    if is_protected && !env_override.is_secret {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Env override '{}' cannot be protected unless it is also secret.",
                env_override.key
            ),
        ));
    }

    Ok(is_protected)
}

fn validate_env_overrides(overrides: &[models::EnvOverride]) -> Result<(), (StatusCode, String)> {
    for env_override in overrides {
        validate_env_override(env_override)?;
    }

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
    let can_rollback = state_machine::can_create_from(&deployment);

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
        can_rollback,
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
                "Invalid group name '{}'. Must be 'default' or match pattern [a-z0-9][a-z0-9/-]*[a-z0-9] (no consecutive hyphens, normalized length max 63 chars)",
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

    validate_env_overrides(&payload.env_overrides)?;

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

        // Verify the source deployment already has a reusable image.
        if !state_machine::can_create_from(&source_deployment) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Cannot create deployment from '{}' because its image is not available yet (status '{}').",
                    from_deployment_id, source_deployment.status
                ),
            ));
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

        // Apply env overrides from the request
        apply_env_overrides(&state, new_deployment.id, &payload.env_overrides).await?;

        // Upsert PORT env var with the final http_port value
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            new_deployment.id,
            "PORT",
            &final_http_port.to_string(),
            false, // not a secret
            false, // is_protected
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
                auth_method: Default::default(),
            },
        }));
    }

    // Branch based on whether user provided a pre-built image
    if let Some(ref user_image) = payload.image {
        if payload.push_image {
            // Path 1a: Push-image deployment — CLI will pull and push to Rise registry
            // Skip digest resolution: the image will be pushed to the internal registry by tag,
            // and the controller uses get_image_tag() (the no-digest path) to pull from there.
            info!("Creating push-image deployment with image: {}", user_image);

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
            let image_tag = state.registry_provider.get_image_tag(
                &payload.project,
                &deployment_id,
                ImageTagType::ClientFacing,
            );

            let deployment = create_deployment_with_hooks(
                &state,
                db_deployments::CreateDeploymentParams {
                    deployment_id: &deployment_id,
                    project_id: project.id,
                    created_by_id: user.id,
                    status: DbDeploymentStatus::Pending,
                    image: Some(user_image), // Store original user input for display
                    image_digest: None, // No digest — controller will use internal registry tag
                    rolled_back_from_deployment_id: None,
                    deployment_group: &payload.group,
                    expires_at,
                    http_port: effective_http_port as i32,
                    is_active: false,
                },
                &project,
            )
            .await?;

            info!(
                "Created push-image deployment {} for project {}",
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

            // Apply env overrides from the request
            apply_env_overrides(&state, deployment.id, &payload.env_overrides).await?;

            crate::db::env_vars::upsert_deployment_env_var(
                &state.db_pool,
                deployment.id,
                "PORT",
                &effective_http_port.to_string(),
                false,
                false,
            )
            .await
            .map_err(|e| {
                error!("Failed to insert PORT env var: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to insert PORT env var: {}", e),
                )
            })?;

            insert_rise_env_vars(&state, &deployment, &project).await?;

            return Ok(Json(CreateDeploymentResponse {
                deployment_id,
                image_tag,
                credentials,
            }));
        }

        // Path 1b: Direct pre-built image deployment (no push)
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
                status: DbDeploymentStatus::Pushed,
                image: Some(user_image),
                image_digest: Some(&image_digest),
                rolled_back_from_deployment_id: None,
                deployment_group: &payload.group,
                expires_at,
                http_port: effective_http_port as i32,
                is_active: false,
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

        // Apply env overrides from the request
        apply_env_overrides(&state, deployment.id, &payload.env_overrides).await?;

        // Upsert PORT env var with the resolved effective value
        // This overwrites any user-set PORT with the resolved value (which may be the same)
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            deployment.id,
            "PORT",
            &effective_http_port.to_string(),
            false, // not a secret
            false, // is_protected
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
            image_tag: image_digest,
            credentials: crate::server::registry::models::RegistryCredentials {
                registry_url: String::new(),
                username: String::new(),
                password: String::new(),
                expires_in: None,
                auth_method: Default::default(),
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

        // Apply env overrides from the request
        apply_env_overrides(&state, deployment.id, &payload.env_overrides).await?;

        // Upsert PORT env var with the resolved effective value
        // This overwrites any user-set PORT with the resolved value (which may be the same)
        crate::db::env_vars::upsert_deployment_env_var(
            &state.db_pool,
            deployment.id,
            "PORT",
            &effective_http_port.to_string(),
            false, // not a secret
            false, // is_protected
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

/// Shared logic for updating a deployment's status.
///
/// Given a resolved deployment and project, validates permissions and applies the status update.
async fn perform_status_update(
    state: &AppState,
    user: &User,
    deployment: crate::db::models::Deployment,
    project: &crate::db::models::Project,
    deployment_id: &str,
    payload: UpdateDeploymentStatusRequest,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    // Check if user has permission (owns the project)
    // Return 404 instead of 403 to avoid revealing project existence
    check_deploy_permission(state, project, user)
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
            (None, vec![])
        } else {
            match state
                .deployment_backend
                .get_deployment_urls(&updated_deployment, project)
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
            state,
            updated_deployment,
            project,
            created_by_email,
            primary_url,
            custom_domain_urls,
        )
        .await,
    ))
}

/// PATCH /projects/{project_name}/deployments/{deployment_id}/status - Update deployment status (project-scoped)
pub async fn update_deployment_status_by_project(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((project_name, deployment_id)): Path<(String, String)>,
    Json(payload): Json<UpdateDeploymentStatusRequest>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    info!(
        "Updating deployment {} status to {:?} for project {}",
        deployment_id, payload.status, project_name
    );

    // Find project by name
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

    // Find deployment by deployment_id + project_id
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
                    format!(
                        "Deployment '{}' not found for project '{}'",
                        deployment_id, project_name
                    ),
                )
            })?;

    perform_status_update(&state, &user, deployment, &project, &deployment_id, payload).await
}

/// PATCH /deployments/{deployment_id}/status - Update deployment status (deprecated, unscoped)
///
/// This endpoint is deprecated. Use `PATCH /projects/{project_name}/deployments/{deployment_id}/status` instead.
/// Kept for backward compatibility with older CLI versions.
pub async fn update_deployment_status(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(deployment_id): Path<String>,
    Json(payload): Json<UpdateDeploymentStatusRequest>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    warn!(
        "Deprecated endpoint called: PATCH /deployments/{}/status — use PATCH /projects/{{project_name}}/deployments/{}/status instead",
        deployment_id, deployment_id
    );

    // Scan all projects to find the deployment (the original, buggy approach)
    let all_projects = projects::list(&state.db_pool, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list projects: {}", e),
        )
    })?;

    let mut matching_projects: Vec<(crate::db::models::Project, crate::db::models::Deployment)> =
        Vec::new();

    for project in all_projects {
        if let Ok(Some(deployment)) =
            db_deployments::find_by_deployment_id(&state.db_pool, &deployment_id, project.id).await
        {
            matching_projects.push((project, deployment));
        }
    }

    if matching_projects.len() > 1 {
        let project_names: Vec<&str> = matching_projects
            .iter()
            .map(|(p, _)| p.name.as_str())
            .collect();
        warn!(
            "Deployment ID collision on deprecated endpoint: deployment_id={} matches {} projects: {:?}. Using first match. \
             Clients should migrate to the project-scoped endpoint.",
            deployment_id,
            matching_projects.len(),
            project_names,
        );
    }

    let (project, deployment) = matching_projects.into_iter().next().ok_or((
        StatusCode::NOT_FOUND,
        format!("Deployment '{}' not found", deployment_id),
    ))?;

    perform_status_update(&state, &user, deployment, &project, &deployment_id, payload).await
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
                "Invalid group name '{}'. Must be 'default' or match pattern [a-z0-9][a-z0-9/-]*[a-z0-9] (no consecutive hyphens, normalized length max 63 chars)",
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
            if error_msg.contains("Pod not found")
                || error_msg.contains("not ready yet")
                || error_msg.contains("waiting to start")
                || error_msg.contains("ContainerCreating")
                || error_msg.contains("PodInitializing")
            {
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

#[cfg(test)]
mod tests {
    use super::{
        normalize_env_override_is_protected, validate_env_override, validate_env_override_key,
    };
    use crate::server::deployment::models::EnvOverride;
    use axum::http::StatusCode;

    #[test]
    fn env_override_key_validation_rejects_empty_keys() {
        assert!(!validate_env_override_key(""));
        assert!(validate_env_override_key("VALID_KEY_123"));
    }

    #[test]
    fn env_override_validation_rejects_port_overrides() {
        let err = validate_env_override(&EnvOverride {
            key: "PORT".to_string(),
            value: "3000".to_string(),
            is_secret: false,
            is_protected: Some(false),
        })
        .unwrap_err();

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.1,
            "PORT cannot be set via env overrides. Use http_port/--http-port instead."
        );
    }

    #[test]
    fn env_override_validation_rejects_protected_non_secrets() {
        let err = validate_env_override(&EnvOverride {
            key: "API_KEY".to_string(),
            value: "value".to_string(),
            is_secret: false,
            is_protected: Some(true),
        })
        .unwrap_err();

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.1,
            "Env override 'API_KEY' cannot be protected unless it is also secret."
        );
    }

    #[test]
    fn env_override_validation_rejects_empty_keys() {
        let err = validate_env_override(&EnvOverride {
            key: String::new(),
            value: "value".to_string(),
            is_secret: false,
            is_protected: Some(false),
        })
        .unwrap_err();

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.1,
            "Invalid env var key '' (must be alphanumeric with underscores)"
        );
    }

    #[test]
    fn env_override_validation_defaults_secret_overrides_to_protected() {
        let is_protected = validate_env_override(&EnvOverride {
            key: "API_KEY".to_string(),
            value: "secret".to_string(),
            is_secret: true,
            is_protected: None,
        })
        .unwrap();

        assert!(is_protected);
    }

    #[test]
    fn env_override_normalization_preserves_explicit_unprotected_secret() {
        let is_protected = normalize_env_override_is_protected(&EnvOverride {
            key: "API_KEY".to_string(),
            value: "secret".to_string(),
            is_secret: true,
            is_protected: Some(false),
        });

        assert!(!is_protected);
    }
}
