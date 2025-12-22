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
        assert!(
            StaticAssets::get("assets/favicon.ico").is_some(),
            "assets/favicon.ico should be embedded"
        );
        assert!(
            StaticAssets::get("assets/favicon-16x16.png").is_some(),
            "assets/favicon-16x16.png should be embedded"
        );
        assert!(
            StaticAssets::get("assets/favicon-32x32.png").is_some(),
            "assets/favicon-32x32.png should be embedded"
        );
        assert!(
            StaticAssets::get("assets/logo.svg").is_some(),
            "assets/logo.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/theme-system.svg").is_some(),
            "assets/theme-system.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/theme-light.svg").is_some(),
            "assets/theme-light.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/theme-dark.svg").is_some(),
            "assets/theme-dark.svg should be embedded"
        );
    }
}
