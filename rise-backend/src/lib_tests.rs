#[cfg(test)]
mod tests {
    use crate::settings::{AuthSettings, DatabaseSettings, ServerSettings, Settings};

    #[tokio::test]
    async fn test_router_builds_without_panic() {
        // This test ensures the router can be built without panicking
        // Catches issues like invalid path syntax (e.g., :id instead of {id})

        // Load settings with test defaults
        let settings = Settings {
            server: ServerSettings {
                host: "127.0.0.1".to_string(),
                port: 0, // Use port 0 for testing
                public_url: "http://localhost:3000".to_string(),
            },
            auth: AuthSettings {
                issuer: "http://localhost:5556/dex".to_string(),
                client_id: "rise-backend".to_string(),
                client_secret: "test-secret".to_string(),
                admin_users: vec![],
            },
            database: DatabaseSettings {
                url: "postgres://rise:rise123@localhost:5432/rise".to_string(),
            },
            registry: None,
            kubernetes: None,
        };

        // This test requires PostgreSQL and Dex to be running
        // Skip if database is not available
        let state = match crate::state::AppState::new_for_server(&settings).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Skipping test - Database/Auth not available: {}", e);
                return;
            }
        };

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
