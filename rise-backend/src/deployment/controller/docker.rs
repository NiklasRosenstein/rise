use async_trait::async_trait;
use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::models::{HostConfig, PortBinding};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::db::models::{Deployment, Project, DeploymentStatus};
use super::{DeploymentBackend, ReconcileResult, HealthStatus};

/// Docker-specific metadata stored in deployment.controller_metadata
#[derive(Serialize, Deserialize, Default, Clone)]
struct DockerMetadata {
    container_id: Option<String>,
    container_name: Option<String>,
    assigned_port: Option<u16>,
    internal_port: u16,  // Default 8080
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

/// Port allocator that assigns ports in the 8000-9000 range
///
/// Simple incrementing allocator for MVP. Future: check for port availability
struct PortAllocator {
    next_port: Arc<Mutex<u16>>,
    min_port: u16,
    max_port: u16,
}

impl PortAllocator {
    fn new() -> Self {
        Self {
            next_port: Arc::new(Mutex::new(8000)),
            min_port: 8000,
            max_port: 9000,
        }
    }

    async fn allocate(&self) -> anyhow::Result<u16> {
        let mut port = self.next_port.lock().await;
        let allocated = *port;
        *port += 1;

        // Wrap around if we exceed max_port
        if *port > self.max_port {
            *port = self.min_port;
        }

        Ok(allocated)
    }
}

/// Docker controller implementation
pub struct DockerController {
    docker: Docker,
    port_allocator: PortAllocator,
}

impl DockerController {
    /// Create a new Docker controller
    pub fn new() -> anyhow::Result<Self> {
        // Connect to local Docker daemon
        let docker = Docker::connect_with_local_defaults()?;

        Ok(Self {
            docker,
            port_allocator: PortAllocator::new(),
        })
    }

    /// Construct image tag from project name and deployment ID
    ///
    /// Format: registry_url/namespace/project:deployment_id
    /// For local Docker: project:deployment_id
    fn construct_image_tag(&self, project_name: &str, deployment_id: &str) -> String {
        // For MVP, use simple local format
        // Future: Get registry URL from settings
        format!("{}:{}", project_name, deployment_id)
    }

    /// Create a Docker container
    async fn create_container(
        &self,
        image_tag: &str,
        host_port: u16,
        container_name: &str,
    ) -> anyhow::Result<String> {
        debug!("Creating container {} with image {} on port {}", container_name, image_tag, host_port);

        // Container port (default 8080)
        let container_port = 8080;

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
            ..Default::default()
        };

        // Create container
        let options = CreateContainerOptions {
            name: container_name,
            ..Default::default()
        };

        let response = self.docker.create_container(Some(options), config).await?;

        info!("Created container {} with ID {}", container_name, response.id);

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
    async fn reconcile(&self, deployment: &Deployment, project: &Project) -> anyhow::Result<ReconcileResult> {
        // Parse existing metadata (or create default)
        let mut metadata: DockerMetadata = serde_json::from_value(deployment.controller_metadata.clone())
            .unwrap_or_default();

        // Default internal port
        if metadata.internal_port == 0 {
            metadata.internal_port = 8080;
        }

        debug!("Reconciling deployment {} (status={:?}) in phase {:?}",
            deployment.deployment_id, deployment.status, metadata.reconcile_phase);

        // Handle Unhealthy deployments - attempt recovery
        if deployment.status == DeploymentStatus::Unhealthy {
            info!("Attempting to recover unhealthy deployment {}", deployment.deployment_id);

            if let Some(ref container_id) = metadata.container_id {
                // Check if container still exists
                match self.docker.inspect_container(container_id, None).await {
                    Ok(inspect) => {
                        // Container exists - check if it's running
                        let state = inspect.state.ok_or_else(|| anyhow::anyhow!("No state in inspect"))?;
                        let is_running = state.running.unwrap_or(false);

                        if is_running {
                            // Container is running again - it recovered!
                            info!("Deployment {} has recovered (container is running)", deployment.deployment_id);
                            return Ok(ReconcileResult {
                                status: DeploymentStatus::Healthy,
                                deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                                controller_metadata: serde_json::to_value(&metadata)?,
                                error_message: None,
                                next_reconcile: super::ReconcileHint::Default,
                            });
                        } else {
                            // Container exists but stopped - try to restart it
                            info!("Attempting to restart stopped container {}", container_id);
                            match self.docker.start_container(container_id, None::<StartContainerOptions<String>>).await {
                                Ok(_) => {
                                    info!("Successfully restarted container {}, waiting for health check", container_id);
                                    // Keep in Unhealthy state until health check confirms it's healthy
                                    return Ok(ReconcileResult {
                                        status: DeploymentStatus::Unhealthy,
                                        deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                                        controller_metadata: serde_json::to_value(&metadata)?,
                                        error_message: None,  // Don't set error - we're attempting recovery
                                        next_reconcile: super::ReconcileHint::Default,
                                    });
                                }
                                Err(e) => {
                                    warn!("Failed to restart container {}: {}", container_id, e);
                                    // Keep in Unhealthy state, let timeout handle it
                                    return Ok(ReconcileResult {
                                        status: DeploymentStatus::Unhealthy,
                                        deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                                        controller_metadata: serde_json::to_value(&metadata)?,
                                        error_message: None,  // Don't set error - timeout will mark as Failed
                                        next_reconcile: super::ReconcileHint::Default,
                                    });
                                }
                            }
                        }
                    }
                    Err(e) if e.to_string().contains("404") || e.to_string().contains("No such container") => {
                        // Container was removed - can't recover
                        warn!("Container {} no longer exists, marking deployment as failed", container_id);
                        return Ok(ReconcileResult {
                            status: DeploymentStatus::Failed,
                            deployment_url: None,
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: Some("Container was removed".to_string()),
                            next_reconcile: super::ReconcileHint::Default,
                        });
                    }
                    Err(e) => {
                        warn!("Error inspecting container {}: {}", container_id, e);
                        // Keep in Unhealthy state, timeout will handle if persistent
                        return Ok(ReconcileResult {
                            status: DeploymentStatus::Unhealthy,
                            deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,  // Don't set error - timeout will mark as Failed
                            next_reconcile: super::ReconcileHint::Default,
                        });
                    }
                }
            } else {
                // No container ID in metadata - can't recover
                return Ok(ReconcileResult {
                    status: DeploymentStatus::Failed,
                    deployment_url: None,
                    controller_metadata: serde_json::to_value(&metadata)?,
                    error_message: Some("No container ID in metadata".to_string()),
                    next_reconcile: super::ReconcileHint::Default,
                });
            }
        }

        match metadata.reconcile_phase {
            ReconcilePhase::NotStarted => {
                // Transition to Deploying status
                info!("Starting reconciliation for deployment {}", deployment.deployment_id);
                metadata.reconcile_phase = ReconcilePhase::CreatingContainer;
                Ok(ReconcileResult {
                    status: DeploymentStatus::Deploying,
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
                    self.port_allocator.allocate().await?
                };
                metadata.assigned_port = Some(port);

                // Determine image to use
                let image_tag = if let Some(ref digest) = deployment.image_digest {
                    // Pre-built image - use the pinned digest
                    debug!("Using pre-built image digest: {}", digest);
                    digest.clone()
                } else {
                    // Built from source - construct image tag
                    let tag = self.construct_image_tag(&project.name, &deployment.deployment_id);
                    debug!("Using constructed image tag: {}", tag);
                    tag
                };
                metadata.image_tag = Some(image_tag.clone());

                // Create container (check if already exists first)
                let container_name = format!("rise-{}-{}", project.name, deployment.deployment_id);
                metadata.container_name = Some(container_name.clone());

                match self.create_container(&image_tag, port, &container_name).await {
                    Ok(container_id) => {
                        metadata.container_id = Some(container_id);
                        metadata.reconcile_phase = ReconcilePhase::StartingContainer;
                        Ok(ReconcileResult {
                            status: DeploymentStatus::Deploying,
                            deployment_url: Some(format!("http://localhost:{}", port)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e) if e.to_string().contains("already exists") || e.to_string().contains("Conflict") => {
                        // Container exists, move to next phase
                        info!("Container {} already exists, moving to next phase", container_name);
                        metadata.reconcile_phase = ReconcilePhase::StartingContainer;
                        Ok(ReconcileResult {
                            status: DeploymentStatus::Deploying,
                            deployment_url: Some(format!("http://localhost:{}", port)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e) => {
                        Ok(ReconcileResult {
                            status: DeploymentStatus::Failed,
                            deployment_url: None,
                            controller_metadata: serde_json::to_value(&metadata)?,
                            next_reconcile: super::ReconcileHint::Default,
                            error_message: Some(format!("Failed to create container: {}", e)),
                        })
                    }
                }
            }

            ReconcilePhase::StartingContainer => {
                let container_id = metadata.container_id.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No container ID in metadata"))?;

                // Start container (idempotent - Docker handles if already running)
                match self.docker.start_container(container_id, None::<StartContainerOptions<String>>).await {
                    Ok(_) => {
                        info!("Started container {}", container_id);
                        metadata.reconcile_phase = ReconcilePhase::WaitingForHealth;
                        Ok(ReconcileResult {
                            status: DeploymentStatus::Deploying,
                            deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e) if e.to_string().contains("already started") || e.to_string().contains("is not stopped") => {
                        // Container already running, move to next phase
                        info!("Container {} already running", container_id);
                        metadata.reconcile_phase = ReconcilePhase::WaitingForHealth;
                        Ok(ReconcileResult {
                            status: DeploymentStatus::Deploying,
                            deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: super::ReconcileHint::Default,
                        })
                    }
                    Err(e) => {
                        Ok(ReconcileResult {
                            status: DeploymentStatus::Failed,
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
                        deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                        next_reconcile: super::ReconcileHint::Default,
                    })
                } else {
                    // Still waiting for health, keep in Deploying state
                    debug!("Deployment {} still waiting for health", deployment.deployment_id);
                    Ok(ReconcileResult {
                        status: DeploymentStatus::Deploying,
                        deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
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
                    deployment_url: metadata.assigned_port.map(|p| format!("http://localhost:{}", p)),
                    controller_metadata: serde_json::to_value(&metadata)?,
                    error_message: None,
                    next_reconcile: super::ReconcileHint::Default,
                })
            }
        }
    }

    /// Check if container is healthy and running
    async fn health_check(&self, deployment: &Deployment) -> anyhow::Result<HealthStatus> {
        let metadata: DockerMetadata = serde_json::from_value(deployment.controller_metadata.clone())?;

        let container_id = metadata.container_id
            .ok_or_else(|| anyhow::anyhow!("No container ID in metadata"))?;

        // Inspect container
        let inspect = self.docker.inspect_container(&container_id, None).await?;

        let state = inspect.state.ok_or_else(|| anyhow::anyhow!("No state in inspect"))?;

        let healthy = state.running.unwrap_or(false) && !state.restarting.unwrap_or(false);

        Ok(HealthStatus {
            healthy,
            message: if !healthy {
                Some(format!("Container state: running={}, status={:?}",
                    state.running.unwrap_or(false),
                    state.status))
            } else {
                None
            },
            last_check: Utc::now(),
        })
    }

    /// Stop a running deployment
    async fn stop(&self, deployment: &Deployment) -> anyhow::Result<()> {
        let metadata: DockerMetadata = serde_json::from_value(deployment.controller_metadata.clone())?;
        if let Some(container_id) = metadata.container_id {
            info!("Stopping container {}", container_id);
            self.docker.stop_container(&container_id, None).await?;
        }
        Ok(())
    }

    /// Cancel a deployment (pre-infrastructure)
    async fn cancel(&self, deployment: &Deployment) -> anyhow::Result<()> {
        info!("Cancelling deployment {} (no infrastructure to clean up)", deployment.deployment_id);
        // For Docker backend, pre-infrastructure cancellation just means no cleanup needed
        // Build artifacts are managed by CLI, not the controller
        Ok(())
    }

    /// Terminate a running deployment (post-infrastructure)
    async fn terminate(&self, deployment: &Deployment) -> anyhow::Result<()> {
        let metadata: DockerMetadata = serde_json::from_value(deployment.controller_metadata.clone())?;
        if let Some(container_id) = metadata.container_id {
            info!("Terminating deployment {} - stopping and removing container {}", deployment.deployment_id, container_id);

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
