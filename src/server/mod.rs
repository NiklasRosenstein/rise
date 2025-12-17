pub mod auth;
pub mod custom_domains;
pub mod deployment;
#[cfg(feature = "aws")]
pub mod ecr;
pub mod encryption;
pub mod env_vars;
pub mod extensions;
pub mod frontend;
pub mod oci;
pub mod project;
pub mod registry;
pub mod settings;
pub mod state;
pub mod team;
pub mod workload_identity;

use anyhow::Result;
use axum::{middleware, Router};
use state::{AppState, ControllerState};
use std::sync::Arc;
#[cfg(any(feature = "k8s", feature = "aws"))]
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Run the HTTP server process with all enabled controllers
pub async fn run_server(settings: settings::Settings) -> Result<()> {
    let state = AppState::new_for_server(&settings).await?;

    // Spawn enabled controllers as background tasks
    let mut controller_handles = vec![];

    // Start Kubernetes deployment controller
    info!("Starting Kubernetes deployment controller");

    let settings_clone = settings.clone();
    let handle = tokio::spawn(async move {
        #[cfg(feature = "k8s")]
        {
            if let Err(e) = run_kubernetes_controller_loop(settings_clone).await {
                tracing::error!("Deployment controller error: {}", e);
            }
        }
        #[cfg(not(feature = "k8s"))]
        {
            tracing::error!(
                "Kubernetes deployment controller is required but the 'k8s' feature is not enabled. \
                 Please rebuild with --features server,k8s"
            );
        }
    });
    controller_handles.push(handle);

    // Start project controller (always enabled)
    info!("Starting project controller");
    let settings_clone = settings.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = run_project_controller_loop(settings_clone).await {
            tracing::error!("Project controller error: {}", e);
        }
    });
    controller_handles.push(handle);

    // Start ECR controller if ECR registry is configured (requires aws feature)
    #[cfg(feature = "aws")]
    if let Some(settings::RegistrySettings::Ecr { .. }) = &settings.registry {
        info!("Starting ECR controller");
        let settings_clone = settings.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = run_ecr_controller_loop(settings_clone).await {
                tracing::error!("ECR controller error: {}", e);
            }
        });
        controller_handles.push(handle);
    }

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route("/version", axum::routing::get(version_info))
        .merge(auth::routes::public_routes());

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        .merge(auth::routes::protected_routes())
        .merge(custom_domains::routes())
        .merge(project::routes::routes())
        .merge(team::routes::team_routes())
        .merge(registry::routes::routes())
        .merge(deployment::routes::deployment_routes())
        .merge(workload_identity::routes::routes())
        .merge(env_vars::routes::routes())
        .merge(extensions::routes::routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::auth_middleware,
        ));

    // Nest all API routes under /api/v1
    let api_routes = public_routes.merge(protected_routes);

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .merge(frontend::routes::frontend_routes())
        .with_state(state.clone())
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    info!("HTTP server listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Graceful shutdown support
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("HTTP server shutdown complete");

    // Wait for all controller tasks to complete
    for handle in controller_handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Run the project controller loop (for embedding in server process)
async fn run_project_controller_loop(settings: settings::Settings) -> Result<()> {
    let state =
        ControllerState::new(&settings.database.url, 2, settings.encryption.as_ref()).await?;

    let controller = Arc::new(project::ProjectController::new(Arc::new(state)));
    controller.start();
    info!("Project controller started");

    // Wait for shutdown signal
    shutdown_signal().await;
    info!("Project controller shutdown complete");
    Ok(())
}

/// Run the ECR controller loop (for embedding in server process)
///
/// Manages ECR repository lifecycle:
/// - Creates repositories for new projects
/// - Cleans up repositories when projects are deleted
#[cfg(feature = "aws")]
async fn run_ecr_controller_loop(settings: settings::Settings) -> Result<()> {
    use crate::server::registry::models::EcrConfig;
    use crate::server::settings::RegistrySettings;

    // Extract ECR config from registry settings
    let ecr_config = match &settings.registry {
        Some(RegistrySettings::Ecr {
            region,
            account_id,
            repo_prefix,
            push_role_arn,
            auto_remove,
            access_key_id,
            secret_access_key,
        }) => EcrConfig {
            region: region.clone(),
            account_id: account_id.clone(),
            repo_prefix: repo_prefix.clone(),
            push_role_arn: push_role_arn.clone(),
            auto_remove: *auto_remove,
            access_key_id: access_key_id.clone(),
            secret_access_key: secret_access_key.clone(),
        },
        _ => {
            anyhow::bail!("ECR controller requires ECR registry configuration");
        }
    };

    let state =
        ControllerState::new(&settings.database.url, 2, settings.encryption.as_ref()).await?;
    let manager = Arc::new(ecr::EcrRepoManager::new(ecr_config).await?);

    let controller = Arc::new(ecr::EcrController::new(Arc::new(state), manager));
    controller.start();
    info!("ECR controller started");

    // Wait for shutdown signal
    shutdown_signal().await;
    info!("ECR controller shutdown complete");
    Ok(())
}

/// Run the Kubernetes deployment controller loop (for embedding in server process)
#[cfg(feature = "k8s")]
async fn run_kubernetes_controller_loop(settings: settings::Settings) -> Result<()> {
    // Install default CryptoProvider for rustls (required for kube-rs HTTPS connections)
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // Extract Kubernetes controller settings
    let (
        kubeconfig,
        ingress_class,
        production_ingress_url_template,
        staging_ingress_url_template,
        ingress_port,
        ingress_schema,
        auth_backend_url,
        auth_signin_url,
        _namespace_format,
        namespace_labels,
        namespace_annotations,
        ingress_annotations,
        ingress_tls_secret_name,
        custom_domain_tls_mode,
        node_selector,
    ) = match settings.deployment_controller.clone() {
        Some(settings::DeploymentControllerSettings::Kubernetes {
            kubeconfig,
            ingress_class,
            production_ingress_url_template,
            staging_ingress_url_template,
            ingress_port,
            ingress_schema,
            auth_backend_url,
            auth_signin_url,
            namespace_format,
            namespace_labels,
            namespace_annotations,
            ingress_annotations,
            ingress_tls_secret_name,
            custom_domain_tls_mode,
            node_selector,
        }) => (
            kubeconfig,
            ingress_class,
            production_ingress_url_template,
            staging_ingress_url_template,
            ingress_port,
            ingress_schema,
            auth_backend_url,
            auth_signin_url,
            namespace_format,
            namespace_labels,
            namespace_annotations,
            ingress_annotations,
            ingress_tls_secret_name,
            custom_domain_tls_mode,
            node_selector,
        ),
        None => {
            anyhow::bail!("Deployment controller not configured. Please add deployment_controller configuration with type: kubernetes")
        }
    };

    let app_state = state::AppState::new_for_controller(&settings).await?;
    let controller_state = ControllerState {
        db_pool: app_state.db_pool.clone(),
        encryption_provider: app_state.encryption_provider.clone(),
    };

    // Create kube client
    let kube_config = if kubeconfig.is_some() {
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

    let backend = Arc::new(deployment::controller::KubernetesController::new(
        controller_state.clone(),
        kube_client,
        deployment::controller::KubernetesControllerConfig {
            ingress_class,
            production_ingress_url_template,
            staging_ingress_url_template,
            ingress_port,
            ingress_schema,
            registry_provider,
            auth_backend_url,
            auth_signin_url,
            namespace_labels,
            namespace_annotations,
            ingress_annotations,
            ingress_tls_secret_name,
            custom_domain_tls_mode,
            node_selector,
        },
    )?);

    // Test Kubernetes API connection before proceeding
    backend.test_connection().await?;

    let controller = Arc::new(deployment::controller::DeploymentController::new(
        Arc::new(controller_state),
        backend.clone(),
        Duration::from_secs(settings.controller.reconcile_interval_secs),
        Duration::from_secs(settings.controller.health_check_interval_secs),
        Duration::from_secs(settings.controller.termination_interval_secs),
        Duration::from_secs(settings.controller.cancellation_interval_secs),
        Duration::from_secs(settings.controller.expiration_interval_secs),
    )?);
    controller.start();
    info!("Kubernetes deployment controller started");

    // Start Kubernetes-specific background loops
    Arc::clone(&backend).start(); // Namespace cleanup loop
    info!("Kubernetes namespace cleanup loop started");

    backend.start_secret_refresh_loop(Duration::from_secs(
        settings.controller.secret_refresh_interval_secs,
    ));
    info!("Kubernetes secret refresh loop started");

    // Wait for shutdown signal
    shutdown_signal().await;
    info!("Kubernetes deployment controller shutdown complete");
    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

async fn version_info() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "repository": env!("CARGO_PKG_REPOSITORY"),
    }))
}

/// Wait for a shutdown signal (SIGTERM or SIGINT)
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C), shutting down gracefully");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down gracefully");
        },
    }
}
