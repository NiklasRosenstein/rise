use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::state::AppState;

use super::StaticAssets;

pub fn frontend_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(serve_index))
        .route("/{*path}", get(serve_static))
}

async fn serve_index() -> Response {
    serve_file("index.html")
}

async fn serve_static(Path(path): Path<String>) -> Response {
    serve_file(&path)
}

fn serve_file(path: &str) -> Response {
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
            // Check if this is an API route
            if path.starts_with("api/")
                || path.starts_with("auth/")
                || path.starts_with("projects/")
                || path.starts_with("teams/")
                || path.starts_with("deployments/")
                || path == "health"
                || path == "me"
            {
                // Let it 404 as an API route
                return (StatusCode::NOT_FOUND, "Not found").into_response();
            }

            // For all other routes, serve index.html (SPA fallback)
            match StaticAssets::get("index.html") {
                Some(content) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data))
                    .unwrap(),
                None => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
            }
        }
    }
}
