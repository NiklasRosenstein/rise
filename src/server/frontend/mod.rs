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
        // Note: index.html is a Vite build artifact (gitignored) and may not
        // be present in dev/CI environments without a frontend build.
        // Only checked-in static assets are asserted here.
        assert!(
            StaticAssets::get("auth-signin.html.tera").is_some(),
            "auth-signin.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("auth-success.html.tera").is_some(),
            "auth-success.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("cli-auth-success.html.tera").is_some(),
            "cli-auth-success.html.tera should be embedded"
        );
        assert!(
            StaticAssets::get("auth-warning.html.tera").is_some(),
            "auth-warning.html.tera should be embedded"
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
        assert!(
            StaticAssets::get("assets/close-x.svg").is_some(),
            "assets/close-x.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/check.svg").is_some(),
            "assets/check.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/info.svg").is_some(),
            "assets/info.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/lock.svg").is_some(),
            "assets/lock.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/globe.svg").is_some(),
            "assets/globe.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/plus.svg").is_some(),
            "assets/plus.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/lightning.svg").is_some(),
            "assets/lightning.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/user.svg").is_some(),
            "assets/user.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/copy.svg").is_some(),
            "assets/copy.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/logout.svg").is_some(),
            "assets/logout.svg should be embedded"
        );
        assert!(
            StaticAssets::get("assets/arrow-left.svg").is_some(),
            "assets/arrow-left.svg should be embedded"
        );
    }
}
