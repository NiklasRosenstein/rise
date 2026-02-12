use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, HeaderMap, Method, StatusCode, Uri},
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
    if state.server_settings.frontend_dev_proxy_url.is_some() {
        return proxy_to_vite(
            &state,
            Method::GET,
            Uri::from_static("/"),
            HeaderMap::new(),
            Body::empty(),
        )
        .await;
    }
    render_index(&state)
}

async fn fallback_handler(State(state): State<AppState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().trim_start_matches('/');

    // API route that wasn't matched - return 404
    if path == "api" || path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

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

    // In development, proxy frontend requests to Vite dev server.
    if state.server_settings.frontend_dev_proxy_url.is_some() {
        return proxy_to_vite(&state, parts.method, parts.uri, parts.headers, body).await;
    }

    // If not a static file, serve SPA index.html
    render_index(&state)
}

fn render_index(state: &AppState) -> Response {
    // Load template from embedded assets
    let template_content = match StaticAssets::get("index.html.tera") {
        Some(content) => match std::str::from_utf8(&content.data) {
            Ok(s) => s.to_string(),
            Err(e) => {
                tracing::error!("Failed to parse index.html.tera as UTF-8: {:#}", e);
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
        tracing::error!("Failed to parse index.html.tera template: {:#}", e);
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
            tracing::error!("Failed to render index.html template: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template rendering error",
            )
                .into_response()
        }
    }
}

async fn proxy_to_vite(
    state: &AppState,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let vite_base = match state.server_settings.frontend_dev_proxy_url.as_deref() {
        Some(url) => url.trim_end_matches('/'),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Frontend dev proxy is not configured",
            )
                .into_response();
        }
    };

    let mut target_url = format!("{vite_base}{}", uri.path());
    if let Some(query) = uri.query() {
        target_url.push('?');
        target_url.push_str(query);
    }

    let client = reqwest::Client::new();
    let mut upstream = client.request(method, target_url);

    for (name, value) in &headers {
        let name_str = name.as_str();
        if is_hop_by_hop_header(name_str) || name == header::HOST {
            continue;
        }
        upstream = upstream.header(name, value);
    }

    let body_bytes = match to_bytes(body, 10 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("Failed to read proxied request body: {:#}", e);
            return (
                StatusCode::BAD_REQUEST,
                "Invalid request body for frontend proxy",
            )
                .into_response();
        }
    };
    upstream = upstream.body(body_bytes);

    let upstream_response = match upstream.send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!("Failed to reach Vite dev server: {:#}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Vite dev server is not reachable. Start it with `mise frontend:dev`.",
            )
                .into_response();
        }
    };

    let status = StatusCode::from_u16(upstream_response.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let response_headers = upstream_response.headers().clone();
    let response_body = match upstream_response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("Failed to read Vite proxy response body: {:#}", e);
            return (
                StatusCode::BAD_GATEWAY,
                "Invalid response from Vite dev server",
            )
                .into_response();
        }
    };

    let mut response_builder = Response::builder().status(status);
    for (name, value) in &response_headers {
        let name_str = name.as_str();
        if is_hop_by_hop_header(name_str) {
            continue;
        }
        response_builder = response_builder.header(name, value);
    }

    response_builder
        .body(Body::from(response_body))
        .unwrap_or_else(|_| {
            (
                StatusCode::BAD_GATEWAY,
                "Failed to build Vite proxy response",
            )
                .into_response()
        })
}

fn is_hop_by_hop_header(header_name: &str) -> bool {
    matches!(
        header_name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
