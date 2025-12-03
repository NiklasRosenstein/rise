use chrono::Utc;

/// Generate deployment ID in format YYYYMMDD-HHMMSS
/// Note: Could have collisions if multiple deployments in same second
/// Enhancement: Add milliseconds for uniqueness
pub fn generate_deployment_id() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Construct image tag from components
/// Format: {registry_url}/{namespace}/{project}:{deployment_id}
pub fn construct_image_tag(
    registry_url: &str,
    namespace: &str,
    project_name: &str,
    deployment_id: &str,
) -> String {
    format!("{}/{}/{}:{}", registry_url, namespace, project_name, deployment_id)
}
