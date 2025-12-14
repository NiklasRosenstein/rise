pub mod auth;
pub mod deployment;
pub mod domain;
#[cfg(feature = "aws")]
pub mod ecr;
pub mod encryption;
pub mod env_vars;
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
#[cfg(any(feature = "docker", feature = "k8s", feature = "aws"))]
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Run the HTTP server process with all enabled controllers
pub async fn run_server(settings: settings::Settings) -> Result<()> {
    let state = AppState::new_for_server(&settings).await?;

    // Spawn enabled controllers as background tasks
    let mut controller_handles = vec![];

    // Start deployment controller (always enabled)
    let is_kubernetes = settings.kubernetes.is_some();
    info!(
        "Starting deployment controller (backend: {})",
        if is_kubernetes {
            "kubernetes"
        } else {
            "docker"
        }
    );

    let settings_clone = settings.clone();
    let handle = tokio::spawn(async move {
        async fn run_controller(_settings: settings::Settings, is_k8s: bool) -> Result<()> {
            #[cfg(any(feature = "docker", feature = "k8s", feature = "aws"))]
            let settings = _settings;
            if is_k8s {
                #[cfg(feature = "k8s")]
                {
                    run_kubernetes_controller_loop(settings).await
                }
                #[cfg(not(feature = "k8s"))]
                {
                    anyhow::bail!(
                        "Kubernetes deployment backend is configured but the 'k8s' feature is not enabled. \
                         Please rebuild with --features k8s or use a pre-built binary with Kubernetes support."
                    )
                }
            } else {
                #[cfg(feature = "docker")]
                {
                    run_deployment_controller_loop(settings).await
                }
                #[cfg(not(feature = "docker"))]
                {
                    anyhow::bail!(
                        "Docker deployment backend is required but the 'docker' feature is not enabled. \
                         Please rebuild with --features docker or use a pre-built binary with Docker support."
                    )
                }
            }
        }

        if let Err(e) = run_controller(settings_clone, is_kubernetes).await {
            tracing::error!("Deployment controller error: {}", e);
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

    // Start domain verification loop (always enabled)
    info!("Starting domain verification loop");
    let settings_clone = settings.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = run_domain_verification_loop(settings_clone).await {
            tracing::error!("Domain verification loop error: {}", e);
        }
    });
    controller_handles.push(handle);

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
        .merge(env_vars::routes::routes())
        .merge(domain::routes::domain_routes())
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

/// Run the deployment controller loop (for embedding in server process)
#[cfg(feature = "docker")]
async fn run_deployment_controller_loop(settings: settings::Settings) -> Result<()> {
    let app_state = state::AppState::new_for_controller(&settings).await?;

    // Create minimal controller state for the base controller
    let controller_state = ControllerState {
        db_pool: app_state.db_pool.clone(),
        encryption_provider: app_state.encryption_provider.clone(),
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
        Duration::from_secs(settings.controller.reconcile_interval_secs),
        Duration::from_secs(settings.controller.health_check_interval_secs),
        Duration::from_secs(settings.controller.termination_interval_secs),
        Duration::from_secs(settings.controller.cancellation_interval_secs),
        Duration::from_secs(settings.controller.expiration_interval_secs),
    )?);
    controller.start();
    info!("Deployment controller started");

    // Wait for shutdown signal
    shutdown_signal().await;
    info!("Deployment controller shutdown complete");
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

/// Run the domain verification loop (for embedding in server process)
///
/// Automatically verifies pending custom domains:
/// - Checks DNS configuration every 5 minutes
/// - Updates verification status when CNAME is properly configured
/// - No manual intervention required by users
async fn run_domain_verification_loop(settings: settings::Settings) -> Result<()> {
    let state = ControllerState::new(&settings.database.url, 2, settings.encryption.as_ref()).await?;
    let verification_loop = Arc::new(domain::verification_loop::DomainVerificationLoop::new(Arc::new(state)));
    verification_loop.start();
    info!("Domain verification loop started");

    // Wait for shutdown signal
    shutdown_signal().await;
    info!("Domain verification loop shutdown complete");
    Ok(())
}

/// Run the Kubernetes deployment controller loop (for embedding in server process)
#[cfg(feature = "k8s")]
async fn run_kubernetes_controller_loop(settings: settings::Settings) -> Result<()> {
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
        encryption_provider: app_state.encryption_provider.clone(),
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
        deployment::controller::KubernetesControllerConfig {
            ingress_class: k8s_settings.ingress_class,
            production_ingress_url_template: k8s_settings.production_ingress_url_template,
            staging_ingress_url_template: k8s_settings.staging_ingress_url_template,
            registry_provider,
            registry_url,
            auth_backend_url: k8s_settings.auth_backend_url,
            auth_signin_url: k8s_settings.auth_signin_url,
            namespace_annotations: k8s_settings.namespace_annotations,
            ingress_annotations: k8s_settings.ingress_annotations,
            ingress_tls_secret_name: k8s_settings.ingress_tls_secret_name,
            node_selector: k8s_settings.node_selector,
        },
    )?);

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
