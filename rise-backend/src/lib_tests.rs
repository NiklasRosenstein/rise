#[cfg(test)]
mod tests {
    use crate::settings::{Settings, ServerSettings, AuthSettings, PocketbaseSettings};

    #[tokio::test]
    async fn test_router_builds_without_panic() {
        // This test ensures the router can be built without panicking
        // Catches issues like invalid path syntax (e.g., :id instead of {id})

        // Load settings with test defaults
        let settings = Settings {
            server: ServerSettings {
                host: "127.0.0.1".to_string(),
                port: 0, // Use port 0 for testing
                public_url: "http://localhost:3001".to_string(),
            },
            auth: AuthSettings {
                secret: "test-secret-key".to_string(),
            },
            pocketbase: PocketbaseSettings {
                url: "http://localhost:8090".to_string(),
            },
        };

        let state = crate::state::AppState::new(&settings).await;

        // This should not panic
        let _app: axum::Router = axum::Router::new()
            .route("/health", axum::routing::get(crate::health_check))
            .merge(crate::auth::routes::routes())
            .merge(crate::project::routes::routes())
            .merge(crate::team::routes::team_routes())
            .with_state(state)
            .into();

        // If we get here, the router built successfully
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = crate::health_check().await;
        assert_eq!(response, "OK");
    }
}
