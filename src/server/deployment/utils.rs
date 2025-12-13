use chrono::Utc;

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
///
/// # Arguments
/// * `state` - AppState containing registry provider configuration
/// * `deployment` - The deployment record
/// * `project` - The project record
///
/// # Returns
/// The fully-qualified image tag to use for docker pull
pub fn get_deployment_image_tag(
    state: &AppState,
    deployment: &Deployment,
    project: &Project,
) -> String {
    // Pre-built images use the pinned digest
    if let Some(ref digest) = deployment.image_digest {
        return digest.clone();
    }

    // Build-from-source: construct from registry config
    if let Some(ref registry_provider) = state.registry_provider {
        let registry_url = registry_provider.registry_url();
        format!(
            "{}/{}:{}",
            registry_url.trim_end_matches('/'),
            project.name,
            deployment.deployment_id
        )
    } else {
        // Fallback if no registry configured (shouldn't happen in practice)
        format!("{}:{}", project.name, deployment.deployment_id)
    }
}
