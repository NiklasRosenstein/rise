use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, debug};

use crate::state::AppState;
use crate::project::models::Project;
use super::models::*;
use super::utils::{generate_deployment_id, construct_image_tag};

#[derive(Debug, Deserialize)]
struct PbAuthRefreshResponse {
    record: PbUser,
}

#[derive(Debug, Deserialize)]
struct PbUser {
    id: String,
    email: String,
}

#[derive(Debug, Deserialize, Default)]
struct PbTeam {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    owners: Vec<String>,
    #[serde(default)]
    members: Vec<String>,
}

/// Validate JWT token and extract user ID
async fn validate_token_and_get_user_id(
    state: &AppState,
    token: &str,
) -> Result<String, (StatusCode, String)> {
    let user_info_url = format!("{}/api/collections/users/auth-refresh", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(&user_info_url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to verify token: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid or expired token".to_string()));
    }

    let auth_response: PbAuthRefreshResponse = response
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse auth response: {}", e)))?;

    Ok(auth_response.record.id)
}

/// Query project by name
fn query_project_by_name(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    project_name: &str,
) -> Result<Project, String> {
    let escaped_name = project_name.replace("'", "\\'");
    let filter = format!("name='{}'", escaped_name);

    let result = authenticated_client
        .records("projects")
        .list()
        .filter(&filter)
        .call::<Project>()
        .map_err(|e| format!("Failed to query project by name: {}", e))?;

    result
        .items
        .into_iter()
        .next()
        .ok_or_else(|| format!("Project '{}' not found", project_name))
}

/// Check if user has permission to deploy to the project
fn check_deploy_permission(
    authenticated_client: &pocketbase_sdk::client::Client<pocketbase_sdk::client::Auth>,
    project: &Project,
    user_id: &str,
) -> Result<(), String> {
    // If project is owned by the user directly, allow
    if let Some(ref owner_user) = project.owner_user {
        if owner_user == user_id {
            return Ok(());
        }
    }

    // If project is owned by a team, check if user is an owner of that team
    if let Some(ref team_id) = project.owner_team {
        let team = authenticated_client
            .records("teams")
            .view(team_id)
            .call::<PbTeam>()
            .map_err(|e| format!("Failed to fetch team: {}", e))?;

        if team.owners.contains(&user_id.to_string()) {
            return Ok(());
        }

        return Err(format!(
            "You must be an owner of team '{}' to deploy to this project",
            team.name
        ));
    }

    Err("You do not have permission to deploy to this project".to_string())
}

/// POST /deployments - Create a new deployment
pub async fn create_deployment(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CreateDeploymentRequest>,
) -> Result<Json<CreateDeploymentResponse>, (StatusCode, String)> {
    info!("Creating deployment for project '{}'", payload.project);

    // 1. Validate JWT and get user ID
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: No authentication token provided".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid Authorization header format".to_string()))?;

    // 2. Validate token with PocketBase and extract user ID
    let user_id = validate_token_and_get_user_id(&state, token).await?;

    // 3. Authenticate with PocketBase using service account
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password(
            "users",
            &state.settings.pocketbase.service_email,
            &state.settings.pocketbase.service_password,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Service authentication failed: {}", e)))?;

    // 4. Query project by name
    let project = query_project_by_name(&authenticated_client, &payload.project)
        .map_err(|_| (StatusCode::NOT_FOUND, format!("Project '{}' not found", payload.project)))?;

    // 5. Check deployment permissions
    check_deploy_permission(&authenticated_client, &project, &user_id)
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // 6. Generate deployment ID
    let deployment_id = generate_deployment_id();
    debug!("Generated deployment ID: {}", deployment_id);

    // 7. Get registry credentials
    let registry_provider = state.registry_provider.as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "No registry configured".to_string()))?;

    let credentials = registry_provider.get_credentials(&payload.project).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get credentials: {}", e)))?;

    // 8. Construct image tag
    let namespace = if let Some(ref settings) = state.settings.registry {
        match settings {
            crate::settings::RegistrySettings::Docker { namespace, .. } => namespace.clone(),
            crate::settings::RegistrySettings::Ecr { account_id, region, .. } => {
                format!("{}.dkr.ecr.{}.amazonaws.com", account_id, region)
            }
        }
    } else {
        "rise-apps".to_string()
    };

    let image_tag = construct_image_tag(
        &credentials.registry_url,
        &namespace,
        &payload.project,
        &deployment_id
    );

    debug!("Image tag: {}", image_tag);

    // 9. Create deployment record in Pocketbase
    let deployment_data = json!({
        "deployment_id": deployment_id,
        "project": project.id,
        "created_by": user_id,
        "status": "Pending",
    });

    let create_url = format!("{}/api/collections/deployments/records", state.settings.pocketbase.url);
    let http_client = reqwest::Client::new();

    let auth_token = authenticated_client.auth_token
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No auth token available".to_string()))?;

    let response = http_client
        .post(&create_url)
        .header("Authorization", &auth_token)
        .json(&deployment_data)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create deployment record: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create deployment ({}) deployment: {}", status, error_text)));
    }

    info!("Created deployment {} for project {}", deployment_id, payload.project);

    // 10. Return response
    Ok(Json(CreateDeploymentResponse {
        deployment_id,
        image_tag,
        credentials,
    }))
}

/// PATCH /deployments/{deployment_id}/status - Update deployment status
pub async fn update_deployment_status(
    State(state): State<AppState>,
    Path(deployment_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<UpdateDeploymentStatusRequest>,
) -> Result<Json<Deployment>, (StatusCode, String)> {
    info!("Updating deployment {} status to {:?}", deployment_id, payload.status);

    // 1. Validate auth and get user ID
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: No authentication token provided".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "Unauthorized: Invalid Authorization header format".to_string()))?;

    let user_id = validate_token_and_get_user_id(&state, token).await?;

    // 2. Authenticate with PocketBase using service account
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password(
            "users",
            &state.settings.pocketbase.service_email,
            &state.settings.pocketbase.service_password,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Service authentication failed: {}", e)))?;

    // 3. Query deployment by deployment_id
    let escaped_id = deployment_id.replace("'", "\\'");
    let filter = format!("deployment_id='{}'", escaped_id);

    let result = authenticated_client
        .records("deployments")
        .list()
        .filter(&filter)
        .call::<Deployment>()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to query deployment: {}", e)))?;

    let deployment = result
        .items
        .into_iter()
        .next()
        .ok_or((StatusCode::NOT_FOUND, format!("Deployment '{}' not found", deployment_id)))?;

    // 4. Get the project to check permissions
    let project = authenticated_client
        .records("projects")
        .view(&deployment.project)
        .call::<Project>()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch project: {}", e)))?;

    // 5. Check if user has permission (owns the project)
    check_deploy_permission(&authenticated_client, &project, &user_id)
        .map_err(|e| (StatusCode::FORBIDDEN, e))?;

    // 6. Update status in Pocketbase
    let mut update_data = json!({
        "status": payload.status,
    });

    if let Some(error_msg) = payload.error_message {
        update_data["error_message"] = json!(error_msg);
    }

    // Set completed_at when deployment completes or fails
    match payload.status {
        DeploymentStatus::Completed | DeploymentStatus::Failed => {
            update_data["completed_at"] = json!(chrono::Utc::now().to_rfc3339());
        }
        _ => {}
    }

    let update_url = format!("{}/api/collections/deployments/records/{}", state.settings.pocketbase.url, deployment.id);
    let http_client = reqwest::Client::new();

    let auth_token = authenticated_client.auth_token
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No auth token available".to_string()))?;

    let response = http_client
        .patch(&update_url)
        .header("Authorization", &auth_token)
        .json(&update_data)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update deployment: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update deployment ({}): {}", status, error_text)));
    }

    let updated_deployment: Deployment = response
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse updated deployment: {}", e)))?;

    info!("Updated deployment {} to status {:?}", deployment_id, payload.status);

    Ok(Json(updated_deployment))
}
