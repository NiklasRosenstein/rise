pub mod auth;
pub mod db;
pub mod deployment;
pub mod ecr;
pub mod frontend;
pub mod oci;
pub mod project;
pub mod registry;
pub mod settings;
pub mod state;
pub mod team;
pub mod workload_identity;

#[cfg(test)]
mod lib_tests;

use anyhow::Result;
use axum::{middleware, Router};
use state::{AppState, ControllerState};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Run the HTTP server process
pub async fn run_server(settings: settings::Settings) -> Result<()> {
    let state = AppState::new_for_server(&settings).await?;

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", axum::routing::get(health_check))
        .merge(auth::routes::public_routes());

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        .merge(auth::routes::protected_routes())
        .merge(project::routes::routes())
        .merge(team::routes::team_routes())
        .merge(registry::routes::routes())
        .merge(deployment::routes::deployment_routes())
        .merge(workload_identity::routes::routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::auth_middleware,
        ));

    let app = public_routes
        .merge(protected_routes)
        .merge(frontend::routes::frontend_routes())
        .with_state(state.clone())
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    info!("HTTP server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Run the deployment controller process
pub async fn run_deployment_controller(settings: settings::Settings) -> Result<()> {
    let app_state = state::AppState::new_for_controller(&settings).await?;

    // Create minimal controller state for the base controller
    let controller_state = ControllerState {
        db_pool: app_state.db_pool.clone(),
    };

    // Wrap registry provider in credentials adapter
    let credentials_provider = app_state.registry_provider.as_ref().map(|p| {
        std::sync::Arc::new(registry::RegistryCredentialsAdapter::new(p.clone()))
            as std::sync::Arc<dyn registry::CredentialsProvider>
    });

    // Extract registry URL for image tag construction
    let registry_url = app_state
        .registry_provider
        .as_ref()
        .map(|p| p.registry_url().to_string());

    let backend = Arc::new(deployment::controller::DockerController::new(
        controller_state.clone(),
        credentials_provider,
        registry_url,
    )?);

    let controller = Arc::new(deployment::controller::DeploymentController::new(
        Arc::new(controller_state),
        backend,
    )?);
    controller.start();
    info!("Deployment controller started");

    // Block forever
    std::future::pending::<()>().await;
    Ok(())
}

/// Run the project controller process
pub async fn run_project_controller(settings: settings::Settings) -> Result<()> {
    let state = ControllerState::new(&settings.database.url, 2).await?;

    let controller = Arc::new(project::ProjectController::new(Arc::new(state)));
    controller.start();
    info!("Project controller started");

    // Block forever
    std::future::pending::<()>().await;
    Ok(())
}

/// Run the ECR controller process
///
/// Manages ECR repository lifecycle:
/// - Creates repositories for new projects
/// - Cleans up repositories when projects are deleted
pub async fn run_ecr_controller(settings: settings::Settings) -> Result<()> {
    use crate::registry::models::EcrConfig;
    use crate::settings::RegistrySettings;

    // Extract ECR config from registry settings
    let ecr_config = match &settings.registry {
        Some(RegistrySettings::Ecr {
            region,
            account_id,
            repo_prefix,
            role_arn,
            push_role_arn,
            auto_remove,
            access_key_id,
            secret_access_key,
        }) => EcrConfig {
            region: region.clone(),
            account_id: account_id.clone(),
            repo_prefix: repo_prefix.clone(),
            role_arn: role_arn.clone(),
            push_role_arn: push_role_arn.clone(),
            auto_remove: *auto_remove,
            access_key_id: access_key_id.clone(),
            secret_access_key: secret_access_key.clone(),
        },
        _ => {
            anyhow::bail!("ECR controller requires ECR registry configuration");
        }
    };

    let state = ControllerState::new(&settings.database.url, 2).await?;
    let manager = Arc::new(ecr::EcrRepoManager::new(ecr_config).await?);

    let controller = Arc::new(ecr::EcrController::new(Arc::new(state), manager));
    controller.start();
    info!("ECR controller started");

    // Block forever
    std::future::pending::<()>().await;
    Ok(())
}

/// Run the Kubernetes deployment controller process
pub async fn run_kubernetes_controller(settings: settings::Settings) -> Result<()> {
    // Install default CryptoProvider for rustls (required for kube-rs HTTPS connections)
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let k8s_settings = settings
        .kubernetes
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Kubernetes settings required"))?;

    let app_state = state::AppState::new_for_controller(&settings).await?;
    let controller_state = ControllerState {
        db_pool: app_state.db_pool.clone(),
    };

    // Create kube client
    let kube_config = if k8s_settings.kubeconfig.is_some() {
        // Use explicit kubeconfig if provided
        kube::Config::from_kubeconfig(&kube::config::KubeConfigOptions {
            context: None,
            cluster: None,
            user: None,
        })
        .await?
    } else {
        kube::Config::infer().await? // In-cluster or ~/.kube/config
    };
    let kube_client = kube::Client::try_from(kube_config)?;

    // Get registry provider
    let registry_provider = app_state.registry_provider.clone();

    let registry_url = registry_provider
        .as_ref()
        .map(|p| p.registry_url().to_string());

    let backend = Arc::new(deployment::controller::KubernetesController::new(
        controller_state.clone(),
        kube_client,
        k8s_settings.ingress_class,
        k8s_settings.domain_suffix,
        registry_provider,
        registry_url,
    )?);

    let controller = Arc::new(deployment::controller::DeploymentController::new(
        Arc::new(controller_state),
        backend.clone(),
    )?);
    controller.start();
    info!("Kubernetes deployment controller started");

    // Start Kubernetes-specific secret refresh loop
    backend.start_secret_refresh_loop();
    info!("Kubernetes secret refresh loop started");

    // Block forever
    std::future::pending::<()>().await;
    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
