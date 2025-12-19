use axum::http::StatusCode;
use chrono::Utc;
use tracing::error;

use crate::db::deployments as db_deployments;
use crate::db::models::{Deployment, Project};
use crate::server::state::AppState;

/// Generate deployment ID in format YYYYMMDD-HHMMSS
/// Note: Could have collisions if multiple deployments in same second
/// Enhancement: Add milliseconds for uniqueness
pub fn generate_deployment_id() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Get the image tag for a deployment
///
/// This is the single source of truth for determining which image to use for a deployment.
/// For pre-built images: returns the digest-pinned reference from image_digest field
/// For build-from-source: constructs the full registry tag using registry configuration
/// For rollback deployments: uses the source deployment's deployment_id for the tag
///
/// # Arguments
/// * `state` - AppState containing registry provider configuration
/// * `deployment` - The deployment record
/// * `project` - The project record
///
/// # Returns
/// The fully-qualified image tag to use for docker pull
pub async fn get_deployment_image_tag(
    state: &AppState,
    deployment: &Deployment,
    project: &Project,
) -> String {
    // Pre-built images use the pinned digest
    if let Some(ref digest) = deployment.image_digest {
        return digest.clone();
    }

    // For rollback deployments, use the source deployment's deployment_id for the image tag
    // This is because rollbacks don't build new images - they reuse the source deployment's image
    let deployment_id_for_tag =
        if let Some(source_deployment_id) = deployment.rolled_back_from_deployment_id {
            // Fetch the source deployment to get its deployment_id
            match db_deployments::find_by_id(&state.db_pool, source_deployment_id).await {
                Ok(Some(source_deployment)) => source_deployment.deployment_id,
                Ok(None) => {
                    tracing::warn!(
                        "Rollback deployment {} references non-existent source deployment {}",
                        deployment.deployment_id,
                        source_deployment_id
                    );
                    // Fallback to current deployment_id if source not found
                    deployment.deployment_id.clone()
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to fetch source deployment {} for rollback {}: {}",
                        source_deployment_id,
                        deployment.deployment_id,
                        e
                    );
                    // Fallback to current deployment_id on error
                    deployment.deployment_id.clone()
                }
            }
        } else {
            // Regular build-from-source deployment
            deployment.deployment_id.clone()
        };

    // Build-from-source: construct from registry config using the appropriate deployment_id
    if let Some(ref registry_provider) = state.registry_provider {
        let registry_url = registry_provider.registry_url();
        format!(
            "{}/{}:{}",
            registry_url.trim_end_matches('/'),
            project.name,
            deployment_id_for_tag
        )
    } else {
        // Fallback if no registry configured (shouldn't happen in practice)
        format!("{}:{}", project.name, deployment_id_for_tag)
    }
}

/// Create a deployment and invoke extension hooks
///
/// This is the single code path for creating deployments. It:
/// 1. Creates the deployment record in the database
/// 2. Invokes before_deployment hooks for all registered extensions
/// 3. Marks the deployment as failed if any extension hook fails
///
/// # Arguments
/// * `state` - AppState containing database pool and extension registry
/// * `params` - Parameters for creating the deployment
/// * `project` - The project this deployment belongs to
///
/// # Returns
/// The created deployment on success, or an error tuple (StatusCode, String)
pub async fn create_deployment_with_hooks(
    state: &AppState,
    params: db_deployments::CreateDeploymentParams<'_>,
    project: &Project,
) -> Result<Deployment, (StatusCode, String)> {
    // Extract deployment_group before moving params (needed for extension hooks)
    let deployment_group = params.deployment_group.to_string();

    // Create the deployment record
    let deployment = db_deployments::create(&state.db_pool, params)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create deployment: {}", e),
            )
        })?;

    // Call before_deployment hooks for all registered extensions
    for (_, extension) in state.extension_registry.iter() {
        if let Err(e) = extension
            .before_deployment(
                deployment.id, // Use the UUID from the database record
                project.id,
                &deployment_group,
            )
            .await
        {
            error!(
                "Extension type '{}' before_deployment hook failed: {:?}",
                extension.extension_type(),
                e
            );

            // Mark deployment as failed
            // Note: The error message from the extension provider should include the specific instance name
            let error_msg = format!("Extension type '{}' failed: {}", extension.extension_type(), e);
            if let Err(mark_err) =
                db_deployments::mark_failed(&state.db_pool, deployment.id, &error_msg).await
            {
                error!(
                    "Failed to mark deployment as failed after extension error: {:?}",
                    mark_err
                );
            }

            return Err((StatusCode::INTERNAL_SERVER_ERROR, error_msg));
        }
    }

    Ok(deployment)
}
