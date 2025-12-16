use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use serde_json::json;
use tera::Tera;

use crate::server::state::AppState;

use super::StaticAssets;

pub fn frontend_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(serve_index))
        .route("/{*path}", get(serve_static))
}

async fn serve_index(State(state): State<AppState>) -> Response {
    render_index(&state)
}

async fn serve_static(Path(path): Path<String>, State(state): State<AppState>) -> Response {
    serve_file(&path, &state)
}

fn render_index(state: &AppState) -> Response {
    // Load template from embedded assets
    let template_content = match StaticAssets::get("index.html.tera") {
        Some(content) => match std::str::from_utf8(&content.data) {
            Ok(s) => s.to_string(),
            Err(e) => {
                tracing::error!("Failed to parse index.html.tera as UTF-8: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Template encoding error")
                    .into_response();
            }
        },
        None => {
            tracing::error!("index.html.tera template not found in embedded assets");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Template not found").into_response();
        }
    };

    // Create Tera instance and add template
    let mut tera = Tera::default();
    if let Err(e) = tera.add_raw_template("index.html.tera", &template_content) {
        tracing::error!("Failed to parse index.html.tera template: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response();
    }

    // Build config object from backend settings
    let config = json!({
        "backendUrl": state.server_settings.public_url.trim_end_matches('/'),
        "issuerUrl": state.auth_settings.issuer,
        "authorizeUrl": state.oauth_client.authorize_url(),
        "clientId": state.auth_settings.client_id,
        "redirectUri": format!("{}/", state.server_settings.public_url.trim_end_matches('/')),
    });

    // Render template with config
    let mut context = tera::Context::new();
    context.insert("config", &config.to_string());

    match tera.render("index.html.tera", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Failed to render index.html template: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template rendering error",
            )
                .into_response()
        }
    }
}

fn serve_file(path: &str, state: &AppState) -> Response {
    let path = path.trim_start_matches('/');

    // Try to get the file from embedded assets
    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // Check if this is an API route (all API routes now under /api/v1)
            if path.starts_with("api/v1/") {
                // Let it 404 as an API route
                return (StatusCode::NOT_FOUND, "Not found").into_response();
            }

            // For all other routes, render index.html with config (SPA fallback)
            render_index(state)
        }
    }
}
