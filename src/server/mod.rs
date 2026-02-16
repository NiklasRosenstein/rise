pub mod auth;
pub mod custom_domains;
pub mod deployment;
#[cfg(feature = "backend")]
pub mod ecr;
pub mod encryption;
pub mod env_vars;
pub mod error;
pub mod extensions;
pub mod frontend;
pub mod middleware;
pub mod oci;
pub mod project;
pub mod registry;
pub mod settings;
pub mod state;
pub mod team;
pub mod workload_identity;

use anyhow::Result;
use axum::{extract::Request, middleware as axum_middleware, response::Response, Router};
use state::{AppState, ControllerState};
use std::sync::Arc;
#[cfg(feature = "backend")]
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{info, Span};

/// Run the HTTP server process with all enabled controllers
pub async fn run_server(settings: settings::Settings) -> Result<()> {
    let state = AppState::new(&settings).await?;

    // Construct ControllerState from AppState components for sharing with controllers
    let controller_state = ControllerState {
        db_pool: state.db_pool.clone(),
        encryption_provider: state.encryption_provider.clone(),
    };

    // Spawn enabled controllers as background tasks
    let mut controller_handles = vec![];

    // Start Kubernetes deployment controller
    info!("Starting Kubernetes deployment controller");

    let settings_clone = settings.clone();
    let controller_state_clone = controller_state.clone();
    let registry_provider = state.registry_provider.clone();
    let handle = tokio::spawn(async move {
        #[cfg(feature = "backend")]
        {
            if let Err(e) = run_kubernetes_controller_loop(
                controller_state_clone,
                registry_provider,
                settings_clone,
            )
            .await
            {
                tracing::error!("Deployment controller error: {:#}", e);
            }
        }
        #[cfg(not(feature = "backend"))]
        {
            tracing::error!(
                "Kubernetes deployment controller is required but the 'backend' feature is not enabled. \
                 Please rebuild with --features backend"
            );
        }
    });
    controller_handles.push(handle);

    // Start project controller (always enabled)
    info!("Starting project controller");
    let settings_clone = settings.clone();
    let controller_state_clone = controller_state.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = run_project_controller_loop(controller_state_clone, settings_clone).await {
            tracing::error!("Project controller error: {:#}", e);
        }
    });
    controller_handles.push(handle);

    // Start ECR controller if ECR registry is configured (requires aws feature)
    #[cfg(feature = "backend")]
    if let Some(settings::RegistrySettings::Ecr { .. }) = &settings.registry {
        info!("Starting ECR controller");
        let settings_clone = settings.clone();
        let controller_state_clone = controller_state.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = run_ecr_controller_loop(controller_state_clone, settings_clone).await {
                tracing::error!("ECR controller error: {:#}", e);
            }
        });
        controller_handles.push(handle);
    }

    // Public routes (no authentication)
    let public_routes = Router::new()
        .route("/health", axum::routing::get(health_check))
        .route("/version", axum::routing::get(version_info))
        .merge(auth::routes::public_routes());

    // Auth-only routes (require authentication but NOT platform access)
    let auth_only_routes = Router::new()
        .merge(auth::routes::auth_only_routes())
        // Apply auth middleware only
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::auth_middleware,
        ));

    // Platform routes (require authentication AND platform access)
    let platform_routes = Router::new()
        .merge(auth::routes::platform_routes())
        .merge(custom_domains::routes())
        .merge(project::routes::routes())
        .merge(team::routes::team_routes())
        .merge(registry::routes::routes())
        .merge(deployment::routes::deployment_routes())
        .merge(workload_identity::routes::routes())
        .merge(env_vars::routes::routes())
        .merge(extensions::routes::routes())
        .merge(encryption::routes::routes())
        // Apply platform access middleware (runs second, after auth)
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::platform_access_middleware,
        ))
        // Apply auth middleware (runs first)
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            auth::middleware::auth_middleware,
        ));

    // Nest all API routes under /api/v1
    let api_routes = public_routes.merge(auth_only_routes).merge(platform_routes);

    let app = Router::new()
        .nest("/api/v1", api_routes)
        // Well-known routes at root level (per OIDC spec)
        .merge(auth::routes::well_known_routes())
        // Root-level auth routes for custom domain support via Ingress routing
        .merge(auth::routes::rise_auth_routes())
        // OAuth/OIDC routes at root level (before frontend fallback)
        .merge(extensions::providers::oauth::routes::oauth_routes())
        .merge(frontend::routes::frontend_routes())
        .with_state(state.clone())
        .layer(
            ServiceBuilder::new()
                // Add request ID middleware first (before TraceLayer so it's available in logs)
                .layer(axum_middleware::from_fn(
                    self::middleware::request_id_middleware,
                ))
                // Enhanced trace layer with custom logging
                .layer(
                    TraceLayer::new_for_http()
                        .on_request(|request: &Request, _span: &Span| {
                            // Extract request ID if available
                            let request_id = request
                                .extensions()
                                .get::<self::middleware::RequestId>()
                                .map(|rid| rid.0);

                            let path = request.uri().path();

                            tracing::info!(
                                method = %request.method(),
                                path = %path,
                                request_id = ?request_id,
                                "request started"
                            );
                        })
                        .on_response(
                            |response: &Response, latency: std::time::Duration, _span: &Span| {
                                let status = response.status();
                                let latency_ms = latency.as_millis();

                                // Extract request ID from response headers
                                let request_id = response
                                    .headers()
                                    .get("x-request-id")
                                    .and_then(|h| h.to_str().ok());

                                // Log with appropriate severity based on status
                                if status.is_server_error() {
                                    tracing::error!(
                                        status = %status,
                                        latency_ms = %latency_ms,
                                        request_id = ?request_id,
                                        "request completed with server error"
                                    );
                                } else if status.is_client_error() {
                                    tracing::warn!(
                                        status = %status,
                                        latency_ms = %latency_ms,
                                        request_id = ?request_id,
                                        "request completed with client error"
                                    );
                                } else {
                                    tracing::info!(
                                        status = %status,
                                        latency_ms = %latency_ms,
                                        request_id = ?request_id,
                                        "request completed successfully"
                                    );
                                }
                            },
                        )
                        .on_failure(
                            |failure: ServerErrorsFailureClass,
                             latency: std::time::Duration,
                             _span: &Span| {
                                let latency_ms = latency.as_millis();
                                tracing::error!(
                                    classification = ?failure,
                                    latency_ms = %latency_ms,
                                    "request failed unexpectedly"
                                );
                            },
                        ),
                ),
        );

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
async fn run_project_controller_loop(
    controller_state: ControllerState,
    _settings: settings::Settings,
) -> Result<()> {
    let controller = Arc::new(project::ProjectController::new(Arc::new(controller_state)));
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
#[cfg(feature = "backend")]
async fn run_ecr_controller_loop(
    controller_state: ControllerState,
    settings: settings::Settings,
) -> Result<()> {
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
    let manager = Arc::new(ecr::EcrRepoManager::new(ecr_config).await?);

    let controller = Arc::new(ecr::EcrController::new(Arc::new(controller_state), manager));
    controller.start();
    info!("ECR controller started");

    // Wait for shutdown signal
    shutdown_signal().await;
    info!("ECR controller shutdown complete");
    Ok(())
}

/// Run the Kubernetes deployment controller loop (for embedding in server process)
#[cfg(feature = "backend")]
async fn run_kubernetes_controller_loop(
    controller_state: ControllerState,
    registry_provider: Arc<dyn crate::server::registry::RegistryProvider>,
    settings: settings::Settings,
) -> Result<()> {
    // Install default CryptoProvider for rustls (required for kube-rs HTTPS connections)
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // Extract Kubernetes controller settings
    let (
        kubeconfig,
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
        custom_domain_ingress_annotations,
        node_selector,
        image_pull_secret_name,
        access_classes,
        host_aliases,
        ingress_controller_namespace,
        ingress_controller_labels,
        network_policy_egress_allow_cidrs,
        pod_security_enabled,
        pod_resources,
        health_probes,
    ) = match settings.deployment_controller.clone() {
        Some(settings::DeploymentControllerSettings::Kubernetes {
            kubeconfig,
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
            custom_domain_ingress_annotations,
            node_selector,
            image_pull_secret_name,
            access_classes,
            host_aliases,
            ingress_controller_namespace,
            ingress_controller_labels,
            network_policy_egress_allow_cidrs,
            pod_security_enabled,
            pod_resources,
            health_probes,
        }) => (
            kubeconfig,
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
            custom_domain_ingress_annotations,
            node_selector,
            image_pull_secret_name,
            access_classes,
            host_aliases,
            ingress_controller_namespace,
            ingress_controller_labels,
            network_policy_egress_allow_cidrs,
            pod_security_enabled,
            pod_resources,
            health_probes,
        ),
        None => {
            anyhow::bail!("Deployment controller not configured. Please add deployment_controller configuration with type: kubernetes")
        }
    };

    // Use components passed from main server (no initialization needed)

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

    // Extract backend_address from auth_backend_url
    let parsed_backend_address = settings::BackendAddress::from_url(&auth_backend_url)?;

    // Filter out null access classes (used to remove inherited entries)
    let access_classes: std::collections::HashMap<_, _> = access_classes
        .into_iter()
        .filter_map(|(k, v)| v.map(|ac| (k, ac)))
        .collect();

    let backend = Arc::new(deployment::controller::KubernetesController::new(
        controller_state.clone(),
        kube_client,
        deployment::controller::KubernetesControllerConfig {
            production_ingress_url_template,
            staging_ingress_url_template,
            ingress_port,
            ingress_schema,
            registry_provider,
            auth_backend_url,
            auth_signin_url,
            backend_address: Some(parsed_backend_address),
            namespace_labels,
            namespace_annotations,
            ingress_annotations,
            ingress_tls_secret_name,
            custom_domain_tls_mode,
            custom_domain_ingress_annotations,
            node_selector,
            image_pull_secret_name,
            access_classes,
            host_aliases,
            ingress_controller_namespace,
            ingress_controller_labels,
            network_policy_egress_allow_cidrs,
            pod_security_enabled,
            pod_resources,
            health_probes,
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
