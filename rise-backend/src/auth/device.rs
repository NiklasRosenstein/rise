use axum::{
    Json,
    extract::{State, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};
use crate::state::AppState;

// In-memory store for device codes (in production, use Redis or database)
type DeviceStore = Arc<RwLock<HashMap<String, DeviceAuth>>>;

#[derive(Clone, Debug)]
struct DeviceAuth {
    user_code: String,
    expires_at: DateTime<Utc>,
    status: DeviceAuthStatus,
    token: Option<String>,
    username: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum DeviceAuthStatus {
    Pending,
    Authorized,
    Expired,
}

lazy_static::lazy_static! {
    static ref DEVICE_STORE: DeviceStore = Arc::new(RwLock::new(HashMap::new()));
}

#[derive(Debug, Serialize)]
pub struct DeviceInitResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Deserialize)]
pub struct DevicePollQuery {
    device_code: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum DevicePollResponse {
    #[serde(rename = "pending")]
    Pending { message: String },
    #[serde(rename = "authorized")]
    Authorized { token: String, username: String },
    #[serde(rename = "expired")]
    Expired { message: String },
}

#[derive(Debug, Deserialize)]
pub struct DeviceAuthorizeQuery {
    code: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceAuthorizeRequest {
    identity: String,
    password: String,
}

// Initialize device flow
pub async fn device_init(
    State(state): State<AppState>,
) -> Result<Json<DeviceInitResponse>, (StatusCode, String)> {
    // Generate codes
    let device_code = generate_code(32);
    let user_code = generate_user_code();
    let expires_at = Utc::now() + Duration::minutes(10);

    // Store in memory
    let mut store = DEVICE_STORE.write().await;
    store.insert(device_code.clone(), DeviceAuth {
        user_code: user_code.clone(),
        expires_at,
        status: DeviceAuthStatus::Pending,
        token: None,
        username: None,
    });

    // Cleanup expired entries
    cleanup_expired(&mut store).await;

    Ok(Json(DeviceInitResponse {
        device_code: device_code.clone(),
        user_code: user_code.clone(),
        verification_uri: format!("{}/device-auth", state.settings.server.host),
        expires_in: 600, // 10 minutes
        interval: 5, // Poll every 5 seconds
    }))
}

// Poll for authorization status
pub async fn device_poll(
    Query(query): Query<DevicePollQuery>,
) -> Result<Json<DevicePollResponse>, (StatusCode, String)> {
    let store = DEVICE_STORE.read().await;

    match store.get(&query.device_code) {
        Some(auth) => {
            if Utc::now() > auth.expires_at {
                Ok(Json(DevicePollResponse::Expired {
                    message: "Device code has expired".to_string(),
                }))
            } else if auth.status == DeviceAuthStatus::Authorized {
                Ok(Json(DevicePollResponse::Authorized {
                    token: auth.token.clone().unwrap_or_default(),
                    username: auth.username.clone().unwrap_or_default(),
                }))
            } else {
                Ok(Json(DevicePollResponse::Pending {
                    message: "Waiting for user authorization".to_string(),
                }))
            }
        }
        None => Err((StatusCode::NOT_FOUND, "Device code not found".to_string())),
    }
}

// Show authorization page
pub async fn device_auth_page(
    Query(query): Query<DeviceAuthorizeQuery>,
) -> Response {
    let store = DEVICE_STORE.read().await;

    // Find device by user code
    let device_exists = store.values().any(|auth| auth.user_code == query.code);

    if !device_exists {
        return (StatusCode::NOT_FOUND, "Invalid or expired code").into_response();
    }

    Html(format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>Rise CLI Authentication</title>
    <style>
        body {{ font-family: system-ui, -apple-system, sans-serif; max-width: 500px; margin: 50px auto; padding: 20px; }}
        h1 {{ color: #333; }}
        form {{ margin-top: 30px; }}
        input {{ display: block; width: 100%; padding: 10px; margin: 10px 0; border: 1px solid #ddd; border-radius: 4px; }}
        button {{ background: #0066cc; color: white; padding: 12px 24px; border: none; border-radius: 4px; cursor: pointer; width: 100%; }}
        button:hover {{ background: #0052a3; }}
        .code {{ background: #f5f5f5; padding: 10px; border-radius: 4px; text-align: center; font-size: 24px; font-weight: bold; letter-spacing: 2px; }}
        .error {{ color: #d32f2f; margin: 10px 0; }}
    </style>
</head>
<body>
    <h1>Rise CLI Authentication</h1>
    <p>Confirm the code matches what you see in your terminal:</p>
    <div class="code">{}</div>
    <form id="loginForm">
        <input type="email" id="identity" name="identity" placeholder="Email" required autofocus />
        <input type="password" id="password" name="password" placeholder="Password" required />
        <button type="submit">Authorize Device</button>
    </form>
    <div id="error" class="error"></div>
    <script>
        document.getElementById('loginForm').addEventListener('submit', async (e) => {{
            e.preventDefault();
            const identity = document.getElementById('identity').value;
            const password = document.getElementById('password').value;
            const errorDiv = document.getElementById('error');

            try {{
                const response = await fetch('/auth/device/authorize?code={}', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ identity, password }})
                }});

                if (response.ok) {{
                    document.body.innerHTML = '<h1>✓ Success!</h1><p>You can close this window and return to your terminal.</p>';
                }} else {{
                    const error = await response.text();
                    errorDiv.textContent = error || 'Authentication failed';
                }}
            }} catch (err) {{
                errorDiv.textContent = 'Network error: ' + err.message;
            }}
        }});
    </script>
</body>
</html>
    "#, query.code, query.code)).into_response()
}

// Authorize device with credentials
pub async fn device_authorize(
    State(state): State<AppState>,
    Query(query): Query<DeviceAuthorizeQuery>,
    Json(payload): Json<DeviceAuthorizeRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Authenticate with PocketBase
    let pb_client = state.pb_client.as_ref();
    let authenticated_client = pb_client
        .auth_with_password("users", &payload.identity, &payload.password)
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e)))?;

    // Get user info to extract username
    let user_email = payload.identity.clone();

    // Get token from authenticated client
    let token = authenticated_client.auth_token.ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to get token from authenticated client".to_string(),
    ))?;

    // Update device authorization
    let mut store = DEVICE_STORE.write().await;

    // Find device by user code
    let device_code = store
        .iter()
        .find(|(_, auth)| auth.user_code == query.code)
        .map(|(code, _)| code.clone());

    if let Some(device_code) = device_code {
        if let Some(auth) = store.get_mut(&device_code) {
            auth.status = DeviceAuthStatus::Authorized;
            auth.token = Some(token);
            auth.username = Some(user_email);
            return Ok(StatusCode::OK);
        }
    }

    Err((StatusCode::NOT_FOUND, "Device code not found".to_string()))
}

// Helper functions
fn generate_code(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

fn generate_user_code() -> String {
    // Generate a readable code like "ABC-DEF"
    format!("{}-{}",
        generate_code(3),
        generate_code(3)
    )
}

async fn cleanup_expired(store: &mut HashMap<String, DeviceAuth>) {
    let now = Utc::now();
    store.retain(|_, auth| auth.expires_at > now);
}
