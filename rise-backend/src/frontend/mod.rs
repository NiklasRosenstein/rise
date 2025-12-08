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
            StaticAssets::get("dashboard.html").is_some(),
            "dashboard.html should be embedded"
        );
        assert!(
            StaticAssets::get("js/auth.js").is_some(),
            "js/auth.js should be embedded"
        );
    }
}
