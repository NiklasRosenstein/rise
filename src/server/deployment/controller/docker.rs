use anyhow::Context;
use async_trait::async_trait;
use bollard::auth::DockerCredentials;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, PortBinding};
use bollard::Docker;
use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

use super::{DeploymentBackend, HealthStatus, ReconcileResult};
use crate::db::models::{Deployment, DeploymentStatus, Project};
use crate::server::registry::OptionalCredentialsProvider;
use crate::server::state::ControllerState;

/// Docker-specific metadata stored in deployment.controller_metadata
#[derive(Serialize, Deserialize, Default, Clone)]
struct DockerMetadata {
    container_id: Option<String>,
    container_name: Option<String>,
    assigned_port: Option<u16>,
    internal_port: u16, // Default 8080
    image_tag: Option<String>,
    #[serde(default)]
    reconcile_phase: ReconcilePhase,
}

/// Reconciliation phases for Docker deployments
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
enum ReconcilePhase {
    #[default]
    NotStarted,
    CreatingContainer,
    StartingContainer,
    WaitingForHealth,
    Completed,
}

/// Port allocator that assigns random ports in the ephemeral port range
///
/// Uses random selection from the high port range (49152-65535) to minimize
/// collision probability. This is stateless and doesn't require coordination
/// between controller instances.
struct PortAllocator;

impl PortAllocator {
    fn new() -> Self {
        Self
    }

    fn allocate(&self) -> u16 {
        // IANA ephemeral port range: 49152-65535 (16,384 ports)
        // This gives us a very low collision probability
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(49152..=65535)
    }
}

/// Docker controller implementation
pub struct DockerController {
    #[allow(dead_code)]
    state: ControllerState,
    docker: Docker,
    port_allocator: PortAllocator,
    credentials_provider: OptionalCredentialsProvider,
    registry_url: Option<String>,
}

impl DockerController {
    /// Create a new Docker controller
    pub fn new(
        state: ControllerState,
        credentials_provider: OptionalCredentialsProvider,
        registry_url: Option<String>,
    ) -> anyhow::Result<Self> {
        // Connect to local Docker daemon
        let docker = Docker::connect_with_local_defaults()?;

        Ok(Self {
            state,
            docker,
            port_allocator: PortAllocator::new(),
            credentials_provider,
            registry_url,
        })
    }

    /// Validate that required metadata exists for a phase
    /// Returns None if valid, Some(ReconcileResult) if needs reset
    fn validate_phase_preconditions(
        &self,
        phase: &ReconcilePhase,
        metadata: &DockerMetadata,
        deployment: &Deployment,
    ) -> Option<ReconcileResult> {
        match phase {
            ReconcilePhase::StartingContainer | ReconcilePhase::WaitingForHealth => {
                if metadata.container_id.is_none() {
                    warn!(
                        "Deployment {} in phase {:?} but no container_id - resetting to CreatingContainer",
                        deployment.deployment_id, phase
                    );

                    // Reset to CreatingContainer (not NotStarted) to preserve port allocation
                    let mut reset_metadata = metadata.clone();
                    reset_metadata.reconcile_phase = ReconcilePhase::CreatingContainer;

                    return Some(ReconcileResult {
                        status: deployment.status.clone(),
                        deployment_url: metadata
                            .assigned_port
                            .map(|p| format!("http://localhost:{}", p)),
                        controller_metadata: serde_json::to_value(&reset_metadata).ok()?,
                        error_message: None,
                        next_reconcile: super::ReconcileHint::Immediate,
                    });
                }
            }
            _ => {}
        }
        None
    }

    /// Get pull credentials for an image if available
    async fn get_pull_credentials(
        &self,
        image_tag: &str,
    ) -> anyhow::Result<Option<DockerCredentials>> {
        // Check if we have a credentials provider
        let Some(ref provider) = self.credentials_provider else {
            debug!("No credentials provider configured, pulling without authentication");
            return Ok(None);
        };

        // Extract registry host from image tag
        // Format: registry.example.com/namespace/image:tag
        let registry_host = if let Some(slash_pos) = image_tag.find('/') {
            &image_tag[..slash_pos]
        } else {
            // No slash means Docker Hub format (e.g., "ubuntu:latest")
            debug!("Image appears to be from Docker Hub, pulling without authentication");
            return Ok(None);
        };

        // Get credentials from provider
        match provider.get_credentials(registry_host).await {
            Ok(Some((username, password))) => {
                info!("Using authenticated pull for registry: {}", registry_host);
                Ok(Some(DockerCredentials {
                    username: Some(username),
                    password: Some(password),
                    ..Default::default()
                }))
            }
            Ok(None) => {
                debug!("No credentials available for registry: {}", registry_host);
                Ok(None)
            }
            Err(e) => {
                warn!("Failed to get credentials for {}: {}", registry_host, e);
                // Fall back to anonymous pull rather than failing
                Ok(None)
            }
        }
    }

    /// Load and decrypt environment variables for a deployment
    async fn load_env_vars(&self, deployment_id: uuid::Uuid) -> anyhow::Result<Vec<String>> {
        // Load and decrypt environment variables using shared helper
        let env_vars = crate::db::env_vars::load_deployment_env_vars_decrypted(
            &self.state.db_pool,
            deployment_id,
            self.state.encryption_provider.as_deref(),
        )
        .await?;

        // Format as KEY=VALUE for Docker
        Ok(env_vars
            .into_iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect())
    }

    /// Create a Docker container
    async fn create_container(
        &self,
        image_tag: &str,
        host_port: u16,
        container_port: u16,
        container_name: &str,
        env_vars: Vec<String>,
    ) -> anyhow::Result<String> {
        debug!(
            "Creating container {} with image {} on port {}",
            container_name, image_tag, host_port
        );

        // Pull the image first (required for digest references and ensures latest version)
        info!("Pulling image: {}", image_tag);

        // Get authentication credentials if available
        let credentials = self
            .get_pull_credentials(image_tag)
            .await
            .context("Failed to prepare pull credentials")?;

        if credentials.is_some() {
            debug!("Using authenticated pull for image: {}", image_tag);
        } else {
            debug!("Using anonymous pull for image: {}", image_tag);
        }

        let options = CreateImageOptions {
            from_image: image_tag,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, credentials);
        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        debug!("Pull status: {}", status);
                    }
                    if let Some(error) = info.error {
                        return Err(anyhow::anyhow!("Failed to pull image: {}", error));
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to pull image '{}': {}",
                        image_tag,
                        e
                    ));
                }
            }
        }
        info!("Successfully pulled image: {}", image_tag);

        // Port bindings
        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            format!("{}/tcp", container_port),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(host_port.to_string()),
            }]),
        );

        // Host config
        let host_config = HostConfig {
            port_bindings: Some(port_bindings),
            ..Default::default()
        };

        // Exposed ports
        let mut exposed_ports = HashMap::new();
        exposed_ports.insert(format!("{}/tcp", container_port), HashMap::new());

        // Container config
        let config = Config {
            image: Some(image_tag.to_string()),
            exposed_ports: Some(exposed_ports),
            host_config: Some(host_config),
            env: if env_vars.is_empty() {
                None
            } else {
                Some(env_vars)
            },
            ..Default::default()
        };

        // Create container
        let options = CreateContainerOptions {
            name: container_name,
            ..Default::default()
        };

        let response = self.docker.create_container(Some(options), config).await?;

        info!(
            "Created container {} with ID {}",
            container_name, response.id
        );

        Ok(response.id)
    }
}

#[async_trait]
impl DeploymentBackend for DockerController {
    /// Reconcile a deployment - idempotent, handles interruptions
    ///
    /// The reconciliation progresses through phases:
    /// 1. NotStarted → CreatingContainer (transition to Deploying)
    /// 2. CreatingContainer → StartingContainer (create Docker container)
    /// 3. StartingContainer → WaitingForHealth (start container)
    /// 4. WaitingForHealth → Completed (wait for health check)
    async fn reconcile(
        &self,
        deployment: &Deployment,
        project: &Project,
    ) -> anyhow::Result<ReconcileResult> {
        // Parse existing metadata (or create default)
        let mut metadata: DockerMetadata =
            serde_json::from_value(deployment.controller_metadata.clone()).unwrap_or_default();

        // Default internal port
        if metadata.internal_port == 0 {
            metadata.internal_port = 8080;
        }

        debug!(
            "Reconciling deployment {} (status={:?}) in phase {:?}",
            deployment.deployment_id, deployment.status, metadata.reconcile_phase
        );

        // Validate phase preconditions before proceeding
        if let Some(result) =
            self.validate_phase_preconditions(&metadata.reconcile_phase, &metadata, deployment)
        {
            info!(
                "Phase validation failed, resetting deployment {}",
                deployment.deployment_id
            );
            return Ok(result);
        }

        // Handle Unhealthy deployments - attempt recovery
        if deployment.status == DeploymentStatus::Unhealthy {
            info!(
                "Attempting to recover unhealthy deployment {}",
                deployment.deployment_id
            );

            if let Some(ref container_id) = metadata.container_id {
                // Check if container still exists
                match self.docker.inspect_container(container_id, None).await {
                    Ok(inspect) => {
                        // Container exists - check if it's running
                        let state = inspect
                            .state
                            .ok_or_else(|| anyhow::anyhow!("No state in inspect"))?;
                        let is_running = state.running.unwrap_or(false);

                        if is_running {
                            // Container is running - keep as Unhealthy until health check confirms
                            info!("Container {} is running, keeping deployment Unhealthy until health check confirms", container_id);
                            return Ok(ReconcileResult {
                                status: DeploymentStatus::Unhealthy,
                                deployment_url: metadata
                                    .assigned_port
                                    .map(|p| format!("http://localhost:{}", p)),
                                controller_metadata: serde_json::to_value(&metadata)?,
                                error_message: None,
                                next_reconcile: super::ReconcileHint::Default,
                            });
                        } else {
                            // Container exists but stopped - try to restart it
                            info!("Attempting to restart stopped container {}", container_id);
                            match self
                                .docker
                                .start_container(
                                    container_id,
                                    None::<StartContainerOptions<String>>,
                                )
                                .await
                            {
                                Ok(_) => {
                                    info!("Successfully restarted container {}, waiting for health check", container_id);
                                    // Keep in Unhealthy state until health check confirms it's healthy
                                    return Ok(ReconcileResult {
                                        status: DeploymentStatus::Unhealthy,
                                        deployment_url: metadata
                                            .assigned_port
                                            .map(|p| format!("http://localhost:{}", p)),
                                        controller_metadata: serde_json::to_value(&metadata)?,
                                        error_message: None, // Don't set error - we're attempting recovery
                                        next_reconcile: super::ReconcileHint::Default,
                                    });
                                }
                                Err(e) => {
                                    warn!("Failed to restart container {}: {}", container_id, e);
                                    // Keep in Unhealthy state, will retry on next reconciliation
                                    return Ok(ReconcileResult {
                                        status: DeploymentStatus::Unhealthy,
                                        deployment_url: metadata
                                            .assigned_port
                                            .map(|p| format!("http://localhost:{}", p)),
                                        controller_metadata: serde_json::to_value(&metadata)?,
                                        error_message: None,
                                        next_reconcile: super::ReconcileHint::Default,
                                    });
                                }
                            }
                        }
                    }
                    Err(e)
                        if e.to_string().contains("404")
                            || e.to_string().contains("No such container") =>
                    {
                        // Container was removed - attempt to recreate it for recovery
                        warn!(
                            "Container {} no longer exists, attempting to recreate for recovery",
                            container_id
                        );

                        // Reset reconcile phase to recreate container
                        // Use CreatingContainer (not NotStarted) to preserve port allocation
                        metadata.reconcile_phase = ReconcilePhase::CreatingContainer;
                        metadata.container_id = None;

                        // Fall through to normal reconciliation logic to recreate container
                        info!(
                            "Reset reconciliation phase to CreatingContainer for deployment {}",
                            deployment.deployment_id
                        );
                    }
                    Err(e) => {
                        warn!("Error inspecting container {}: {}", container_id, e);
                        // Keep in Unhealthy state, will retry on next reconciliation
                        return Ok(ReconcileResult {
                            status: DeploymentStatus::Unhealthy,
                            deployment_url: metadata
                                .assigned_port
                                .map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        });
                    }
                }
            } else {
                // No container ID in metadata - reset and recreate for recovery
                warn!(
                    "Unhealthy deployment {} has no container ID in phase {:?}, attempting to recreate",
                    deployment.deployment_id, metadata.reconcile_phase
                );

                // Reset reconcile phase to recreate container
                // Use CreatingContainer (not NotStarted) to preserve port allocation
                metadata.reconcile_phase = ReconcilePhase::CreatingContainer;

                // Fall through to normal reconciliation logic to recreate container
                info!(
                    "Reset reconciliation phase to CreatingContainer for deployment {}",
                    deployment.deployment_id
                );
            }
        }

        match metadata.reconcile_phase {
            ReconcilePhase::NotStarted => {
                // Determine status based on current deployment state
                // If already Unhealthy, keep it Unhealthy (recovery attempt)
                // Otherwise transition to Deploying (initial deployment)
                let status = if deployment.status == DeploymentStatus::Unhealthy {
                    info!(
                        "Starting recovery attempt for unhealthy deployment {}",
                        deployment.deployment_id
                    );
                    DeploymentStatus::Unhealthy
                } else {
                    info!(
                        "Starting reconciliation for deployment {}",
                        deployment.deployment_id
                    );
                    DeploymentStatus::Deploying
                };

                metadata.reconcile_phase = ReconcilePhase::CreatingContainer;
                Ok(ReconcileResult {
                    status,
                    deployment_url: None,
                    controller_metadata: serde_json::to_value(&metadata)?,
                    error_message: None,
                    next_reconcile: super::ReconcileHint::Default,
                })
            }

            ReconcilePhase::CreatingContainer => {
                // Allocate port (idempotent - uses existing if already allocated)
                let port = if let Some(p) = metadata.assigned_port {
                    p
                } else {
                    self.port_allocator.allocate()
                };
                metadata.assigned_port = Some(port);

                // Determine image to use
                let image_tag = if let Some(ref digest) = deployment.image_digest {
                    // Pre-built images use the pinned digest
                    digest.clone()
                } else if let Some(ref registry_url) = self.registry_url {
                    // Build-from-source: construct from registry config
                    format!(
                        "{}/{}:{}",
                        registry_url.trim_end_matches('/'),
                        project.name,
                        deployment.deployment_id
                    )
                } else {
                    // Fallback if no registry configured (shouldn't happen in practice)
                    format!("{}:{}", project.name, deployment.deployment_id)
                };
                debug!("Using image tag: {}", image_tag);
                metadata.image_tag = Some(image_tag.clone());

                // Create container (check if already exists first)
                let container_name = format!("rise-{}-{}", project.name, deployment.deployment_id);
                metadata.container_name = Some(container_name.clone());

                let container_port = deployment.http_port as u16;

                // Load and decrypt environment variables
                let env_vars = self.load_env_vars(deployment.id).await.map_err(|e| {
                    error!(
                        "Failed to load environment variables for deployment {}: {}",
                        deployment.deployment_id, e
                    );
                    e
                })?;

                match self
                    .create_container(&image_tag, port, container_port, &container_name, env_vars)
                    .await
                {
                    Ok(container_id) => {
                        metadata.container_id = Some(container_id);
                        metadata.reconcile_phase = ReconcilePhase::StartingContainer;

                        // Preserve Unhealthy status during recovery, otherwise use Deploying
                        let status = if deployment.status == DeploymentStatus::Unhealthy {
                            DeploymentStatus::Unhealthy
                        } else {
                            DeploymentStatus::Deploying
                        };

                        Ok(ReconcileResult {
                            status,
                            deployment_url: Some(format!("http://localhost:{}", port)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e)
                        if e.to_string().contains("already exists")
                            || e.to_string().contains("Conflict") =>
                    {
                        // Container exists - need to get its ID
                        info!(
                            "Container {} already exists, retrieving container ID",
                            container_name
                        );

                        // Use Docker API to find the container by name
                        match self.docker.inspect_container(&container_name, None).await {
                            Ok(inspect) => {
                                let container_id = inspect
                                    .id
                                    .ok_or_else(|| anyhow::anyhow!("No ID in container inspect"))?;
                                info!(
                                    "Found existing container {} with ID {}",
                                    container_name, container_id
                                );

                                metadata.container_id = Some(container_id);
                                metadata.reconcile_phase = ReconcilePhase::StartingContainer;

                                // Preserve Unhealthy status during recovery, otherwise use Deploying
                                let status = if deployment.status == DeploymentStatus::Unhealthy {
                                    DeploymentStatus::Unhealthy
                                } else {
                                    DeploymentStatus::Deploying
                                };

                                Ok(ReconcileResult {
                                    status,
                                    deployment_url: Some(format!("http://localhost:{}", port)),
                                    controller_metadata: serde_json::to_value(&metadata)?,
                                    error_message: None,
                                    next_reconcile: super::ReconcileHint::Immediate,
                                })
                            }
                            Err(inspect_err) => {
                                // Container name conflict but can't inspect - likely in intermediate state
                                warn!(
                                    "Container {} exists but cannot inspect: {}",
                                    container_name, inspect_err
                                );

                                // Keep in current status and retry
                                let status = if deployment.status == DeploymentStatus::Unhealthy {
                                    DeploymentStatus::Unhealthy
                                } else {
                                    DeploymentStatus::Deploying
                                };

                                Ok(ReconcileResult {
                                    status,
                                    deployment_url: Some(format!("http://localhost:{}", port)),
                                    controller_metadata: serde_json::to_value(&metadata)?,
                                    error_message: Some(format!(
                                        "Container exists but cannot inspect: {}",
                                        inspect_err
                                    )),
                                    next_reconcile: super::ReconcileHint::Default,
                                })
                            }
                        }
                    }
                    Err(e) => {
                        // During recovery (Unhealthy), keep as Unhealthy and retry
                        // During initial deployment, mark as Failed (no recovery needed)
                        let status = if deployment.status == DeploymentStatus::Unhealthy {
                            DeploymentStatus::Unhealthy
                        } else {
                            DeploymentStatus::Failed
                        };

                        Ok(ReconcileResult {
                            status,
                            deployment_url: None,
                            controller_metadata: serde_json::to_value(&metadata)?,
                            next_reconcile: super::ReconcileHint::Default,
                            error_message: Some(format!("Failed to create container: {}", e)),
                        })
                    }
                }
            }

            ReconcilePhase::StartingContainer => {
                // Get container ID with better error handling
                let container_id = match metadata.container_id.as_ref() {
                    Some(id) => id,
                    None => {
                        // This should have been caught by phase validation, but handle defensively
                        error!(
                            "Deployment {} in StartingContainer phase without container_id - this indicates a bug",
                            deployment.deployment_id
                        );

                        // Reset to CreatingContainer to attempt recovery
                        metadata.reconcile_phase = ReconcilePhase::CreatingContainer;

                        return Ok(ReconcileResult {
                            status: if deployment.status == DeploymentStatus::Unhealthy {
                                DeploymentStatus::Unhealthy
                            } else {
                                DeploymentStatus::Deploying
                            },
                            deployment_url: metadata
                                .assigned_port
                                .map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: Some(
                                "Internal error: missing container ID, retrying".to_string(),
                            ),
                            next_reconcile: super::ReconcileHint::Immediate,
                        });
                    }
                };

                // Start container (idempotent - Docker handles if already running)
                match self
                    .docker
                    .start_container(container_id, None::<StartContainerOptions<String>>)
                    .await
                {
                    Ok(_) => {
                        info!("Started container {}", container_id);
                        metadata.reconcile_phase = ReconcilePhase::WaitingForHealth;

                        // Preserve Unhealthy status during recovery, otherwise use Deploying
                        let status = if deployment.status == DeploymentStatus::Unhealthy {
                            DeploymentStatus::Unhealthy
                        } else {
                            DeploymentStatus::Deploying
                        };

                        Ok(ReconcileResult {
                            status,
                            deployment_url: metadata
                                .assigned_port
                                .map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e)
                        if e.to_string().contains("already started")
                            || e.to_string().contains("is not stopped") =>
                    {
                        // Container already running, move to next phase
                        info!("Container {} already running", container_id);
                        metadata.reconcile_phase = ReconcilePhase::WaitingForHealth;

                        // Preserve Unhealthy status during recovery, otherwise use Deploying
                        let status = if deployment.status == DeploymentStatus::Unhealthy {
                            DeploymentStatus::Unhealthy
                        } else {
                            DeploymentStatus::Deploying
                        };

                        Ok(ReconcileResult {
                            status,
                            deployment_url: metadata
                                .assigned_port
                                .map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e) => {
                        // During recovery (Unhealthy), keep as Unhealthy and retry
                        // During initial deployment, mark as Failed (no recovery needed)
                        let status = if deployment.status == DeploymentStatus::Unhealthy {
                            DeploymentStatus::Unhealthy
                        } else {
                            DeploymentStatus::Failed
                        };

                        Ok(ReconcileResult {
                            status,
                            deployment_url: None,
                            controller_metadata: serde_json::to_value(&metadata)?,
                            next_reconcile: super::ReconcileHint::Default,
                            error_message: Some(format!("Failed to start container: {}", e)),
                        })
                    }
                }
            }

            ReconcilePhase::WaitingForHealth => {
                // Check if container is healthy
                let health = self.health_check(deployment).await?;

                if health.healthy {
                    info!("Deployment {} is healthy", deployment.deployment_id);
                    metadata.reconcile_phase = ReconcilePhase::Completed;
                    Ok(ReconcileResult {
                        status: DeploymentStatus::Healthy,
                        deployment_url: metadata
                            .assigned_port
                            .map(|p| format!("http://localhost:{}", p)),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                        next_reconcile: super::ReconcileHint::Default,
                    })
                } else {
                    // Still waiting for health
                    // Preserve Unhealthy status during recovery, otherwise use Deploying
                    let status = if deployment.status == DeploymentStatus::Unhealthy {
                        debug!(
                            "Deployment {} still unhealthy, waiting for health",
                            deployment.deployment_id
                        );
                        DeploymentStatus::Unhealthy
                    } else {
                        debug!(
                            "Deployment {} still waiting for health",
                            deployment.deployment_id
                        );
                        DeploymentStatus::Deploying
                    };

                    Ok(ReconcileResult {
                        status,
                        deployment_url: metadata
                            .assigned_port
                            .map(|p| format!("http://localhost:{}", p)),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                        next_reconcile: super::ReconcileHint::Default,
                    })
                }
            }

            ReconcilePhase::Completed => {
                // Already healthy, no-op (keep returning Healthy)
                Ok(ReconcileResult {
                    status: DeploymentStatus::Healthy,
                    deployment_url: metadata
                        .assigned_port
                        .map(|p| format!("http://localhost:{}", p)),
                    controller_metadata: serde_json::to_value(&metadata)?,
                    error_message: None,
                    next_reconcile: super::ReconcileHint::Default,
                })
            }
        }
    }

    /// Check if container is healthy and running
    async fn health_check(&self, deployment: &Deployment) -> anyhow::Result<HealthStatus> {
        let metadata: DockerMetadata =
            serde_json::from_value(deployment.controller_metadata.clone())?;

        let container_id = metadata
            .container_id
            .ok_or_else(|| anyhow::anyhow!("No container ID in metadata"))?;

        // Inspect container
        match self.docker.inspect_container(&container_id, None).await {
            Ok(inspect) => {
                let state = inspect
                    .state
                    .ok_or_else(|| anyhow::anyhow!("No state in inspect"))?;
                let healthy = state.running.unwrap_or(false) && !state.restarting.unwrap_or(false);

                Ok(HealthStatus {
                    healthy,
                    message: if !healthy {
                        Some(format!(
                            "Container state: running={}, status={:?}",
                            state.running.unwrap_or(false),
                            state.status
                        ))
                    } else {
                        None
                    },
                    last_check: Utc::now(),
                })
            }
            Err(e)
                if e.to_string().contains("404") || e.to_string().contains("No such container") =>
            {
                // Container doesn't exist - this is expected during termination or if removed externally
                // Return unhealthy status instead of error
                Ok(HealthStatus {
                    healthy: false,
                    message: Some("Container not found".to_string()),
                    last_check: Utc::now(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Stop a running deployment
    async fn stop(&self, deployment: &Deployment) -> anyhow::Result<()> {
        let metadata: DockerMetadata =
            serde_json::from_value(deployment.controller_metadata.clone())?;
        if let Some(container_id) = metadata.container_id {
            info!("Stopping container {}", container_id);
            self.docker.stop_container(&container_id, None).await?;
        }
        Ok(())
    }

    /// Cancel a deployment (pre-infrastructure)
    async fn cancel(&self, deployment: &Deployment) -> anyhow::Result<()> {
        info!(
            "Cancelling deployment {} (no infrastructure to clean up)",
            deployment.deployment_id
        );
        // For Docker backend, pre-infrastructure cancellation just means no cleanup needed
        // Build artifacts are managed by CLI, not the controller
        Ok(())
    }

    /// Terminate a running deployment (post-infrastructure)
    async fn terminate(&self, deployment: &Deployment) -> anyhow::Result<()> {
        let metadata: DockerMetadata =
            serde_json::from_value(deployment.controller_metadata.clone())?;
        if let Some(container_id) = metadata.container_id {
            info!(
                "Terminating deployment {} - stopping and removing container {}",
                deployment.deployment_id, container_id
            );

            // Stop container
            if let Err(e) = self.docker.stop_container(&container_id, None).await {
                warn!("Error stopping container {}: {}", container_id, e);
            }

            // Remove container
            if let Err(e) = self.docker.remove_container(&container_id, None).await {
                warn!("Error removing container {}: {}", container_id, e);
            }
        }
        Ok(())
    }
}
