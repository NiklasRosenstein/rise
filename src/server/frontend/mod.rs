pub mod routes;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct StaticAssets;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_assets_embedded() {
        // Verify critical static assets are embedded
        assert!(
            StaticAssets::get("index.html.tera").is_some(),
            "index.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("auth-signin.html.tera").is_some(),
            "auth-signin.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("auth-success.html.tera").is_some(),
            "auth-success.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("auth-warning.html.tera").is_some(),
            "auth-warning.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("js/auth.js").is_some(),
            "js/auth.js should be embedded"
        );
        assert!(
            StaticAssets::get("js/api.js").is_some(),
            "js/api.js should be embedded"
        );
        assert!(
            StaticAssets::get("js/app.js").is_some(),
            "js/app.js should be embedded"
        );
    }
}
