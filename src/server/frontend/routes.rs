use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode, Uri},
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
        .fallback(fallback_handler)
}

async fn serve_index(State(state): State<AppState>) -> Response {
    render_index(&state)
}

async fn fallback_handler(uri: Uri, State(state): State<AppState>) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try to serve as static file first
    if let Some(content) = StaticAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(content.data))
            .unwrap();
    }

    // If not a static file and not an API route, serve SPA index.html
    if !path.starts_with("api/") {
        return render_index(&state);
    }

    // API route that wasn't matched - return 404
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

fn render_index(state: &AppState) -> Response {
    // Load template from embedded assets
    let template_content = match StaticAssets::get("index.html.tera") {
        Some(content) => match std::str::from_utf8(&content.data) {
            Ok(s) => s.to_string(),
            Err(e) => {
                tracing::error!("Failed to parse index.html.tera as UTF-8: {:?}", e);
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
        tracing::error!("Failed to parse index.html.tera template: {:?}", e);
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
            tracing::error!("Failed to render index.html template: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template rendering error",
            )
                .into_response()
        }
    }
}
