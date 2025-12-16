use chrono::Utc;

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
