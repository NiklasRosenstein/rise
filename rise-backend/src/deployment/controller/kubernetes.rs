use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, PostParams};
use kube::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use super::{DeploymentBackend, HealthStatus, ReconcileHint, ReconcileResult};
use crate::db::deployments as db_deployments;
use crate::db::models::{Deployment, DeploymentStatus, Project};
use crate::registry::RegistryProvider;
use crate::state::ControllerState;

/// Kubernetes-specific metadata stored in deployment.controller_metadata
#[derive(Serialize, Deserialize, Default, Clone)]
struct KubernetesMetadata {
    namespace: Option<String>,
    replicaset_name: Option<String>,
    service_name: Option<String>,
    ingress_name: Option<String>,
    image_tag: Option<String>,
    http_port: u16,
    #[serde(default)]
    reconcile_phase: ReconcilePhase,
    previous_replicaset: Option<String>,
}

/// Reconciliation phases for Kubernetes deployments
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
enum ReconcilePhase {
    #[default]
    NotStarted,
    CreatingNamespace,
    CreatingImagePullSecret,
    CreatingService,
    CreatingReplicaSet,
    WaitingForReplicaSet,
    UpdatingIngress,
    WaitingForHealth,
    Completed,
}

/// Kubernetes controller implementation
pub struct KubernetesController {
    state: ControllerState,
    kube_client: Client,
    #[allow(dead_code)] // Will be used in reconciliation implementation
    ingress_class: String,
    #[allow(dead_code)] // Will be used in reconciliation implementation
    domain_suffix: String,
    registry_provider: Option<Arc<dyn RegistryProvider>>,
    #[allow(dead_code)] // Will be used in reconciliation implementation
    registry_url: Option<String>,
}

impl KubernetesController {
    /// Create a new Kubernetes controller
    pub fn new(
        state: ControllerState,
        kube_client: Client,
        ingress_class: String,
        domain_suffix: String,
        registry_provider: Option<Arc<dyn RegistryProvider>>,
        registry_url: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            state,
            kube_client,
            ingress_class,
            domain_suffix,
            registry_provider,
            registry_url,
        })
    }

    /// Start secret refresh loop (Kubernetes-specific)
    pub fn start_secret_refresh_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(3600)); // Check every hour
            loop {
                ticker.tick().await;
                if let Err(e) = self.refresh_image_pull_secrets().await {
                    error!("Error refreshing image pull secrets: {}", e);
                }
            }
        });
    }

    /// Refresh image pull secrets for all projects with active deployments
    async fn refresh_image_pull_secrets(&self) -> Result<()> {
        // Get registry provider
        let Some(ref provider) = self.registry_provider else {
            // No registry provider configured, skip refresh
            return Ok(());
        };

        // Find all projects that have active Kubernetes deployments
        // We'll refresh the secret for each namespace (one per project)
        let healthy_deployments =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Healthy).await?;
        let unhealthy_deployments =
            db_deployments::find_by_status(&self.state.db_pool, DeploymentStatus::Unhealthy)
                .await?;
        let deployments = [healthy_deployments, unhealthy_deployments].concat();

        // Group deployments by namespace to avoid refreshing the same secret multiple times
        use std::collections::HashSet;
        let mut namespaces_to_refresh = HashSet::new();

        for deployment in deployments {
            let metadata: Option<KubernetesMetadata> =
                serde_json::from_value(deployment.controller_metadata.clone()).ok();

            let Some(metadata) = metadata else {
                continue; // Not a Kubernetes deployment
            };

            let Some(namespace) = metadata.namespace else {
                continue;
            };

            namespaces_to_refresh.insert(namespace);
        }

        // Refresh secret for each namespace
        for namespace in namespaces_to_refresh {
            if let Err(e) = self
                .refresh_namespace_pull_secret(&namespace, provider)
                .await
            {
                warn!(
                    "Failed to refresh pull secret for namespace {}: {}",
                    namespace, e
                );
            } else {
                debug!("Refreshed pull secret for namespace {}", namespace);
            }
        }

        Ok(())
    }

    /// Refresh the image pull secret for a specific namespace
    async fn refresh_namespace_pull_secret(
        &self,
        namespace: &str,
        provider: &Arc<dyn RegistryProvider>,
    ) -> Result<()> {
        let (username, password) = provider.get_pull_credentials().await?;

        let secret_api: Api<Secret> = Api::namespaced(self.kube_client.clone(), namespace);

        let secret = self.create_dockerconfigjson_secret(
            "rise-registry-creds",
            provider.registry_host(),
            &username,
            &password,
        )?;

        secret_api
            .replace("rise-registry-creds", &PostParams::default(), &secret)
            .await?;

        info!(
            "Refreshed image pull secret for namespace {} (project-wide secret)",
            namespace
        );
        Ok(())
    }

    /// Create a dockerconfigjson Secret for image pulling
    fn create_dockerconfigjson_secret(
        &self,
        name: &str,
        registry_host: &str,
        username: &str,
        password: &str,
    ) -> Result<Secret> {
        use base64::Engine;

        // Create docker config JSON
        let auth =
            base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
        let docker_config = serde_json::json!({
            "auths": {
                registry_host: {
                    "username": username,
                    "password": password,
                    "auth": auth,
                }
            }
        });

        let docker_config_bytes = docker_config.to_string().into_bytes();

        let mut data = BTreeMap::new();
        data.insert(
            ".dockerconfigjson".to_string(),
            k8s_openapi::ByteString(docker_config_bytes),
        );

        Ok(Secret {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            type_: Some("kubernetes.io/dockerconfigjson".to_string()),
            data: Some(data),
            ..Default::default()
        })
    }

    /// Get namespace name for a project
    #[allow(dead_code)] // Will be used in reconciliation implementation
    fn namespace_name(project: &Project) -> String {
        format!("rise-{}", project.name)
    }

    /// Create common labels for all resources
    #[allow(dead_code)] // Will be used in reconciliation implementation
    fn common_labels(project: &Project) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert("rise.dev/managed-by".to_string(), "rise".to_string());
        labels.insert("rise.dev/project".to_string(), project.name.clone());
        labels
    }

    /// Create deployment-specific labels
    #[allow(dead_code)] // Will be used in reconciliation implementation
    fn deployment_labels(project: &Project, deployment: &Deployment) -> BTreeMap<String, String> {
        let mut labels = Self::common_labels(project);
        labels.insert(
            "rise.dev/deployment-group".to_string(),
            deployment.deployment_group.clone(),
        );
        labels.insert(
            "rise.dev/deployment-id".to_string(),
            deployment.deployment_id.clone(),
        );
        labels
    }
}

#[async_trait]
impl DeploymentBackend for KubernetesController {
    async fn reconcile(
        &self,
        deployment: &Deployment,
        _project: &Project,
    ) -> Result<ReconcileResult> {
        // Parse existing metadata (or create default)
        let mut metadata: KubernetesMetadata =
            serde_json::from_value(deployment.controller_metadata.clone()).unwrap_or_default();

        // Default HTTP port
        if metadata.http_port == 0 {
            metadata.http_port = deployment.http_port as u16;
        }

        debug!(
            "Reconciling deployment {} (status={:?}) in phase {:?}",
            deployment.deployment_id, deployment.status, metadata.reconcile_phase
        );

        // TODO: Implement phase-based reconciliation
        // For now, just return a placeholder

        Ok(ReconcileResult {
            status: DeploymentStatus::Deploying,
            deployment_url: None,
            controller_metadata: serde_json::to_value(&metadata)?,
            error_message: Some("Kubernetes controller not yet fully implemented".to_string()),
            next_reconcile: ReconcileHint::After(Duration::from_secs(60)),
        })
    }

    async fn health_check(&self, deployment: &Deployment) -> Result<HealthStatus> {
        let metadata: KubernetesMetadata =
            serde_json::from_value(deployment.controller_metadata.clone())?;

        let rs_name = metadata
            .replicaset_name
            .ok_or_else(|| anyhow::anyhow!("No ReplicaSet name"))?;
        let namespace = metadata
            .namespace
            .ok_or_else(|| anyhow::anyhow!("No namespace"))?;

        let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), &namespace);
        let rs = rs_api.get(&rs_name).await?;

        // Check ReplicaSet status
        let spec_replicas = rs.spec.and_then(|s| s.replicas).unwrap_or(1);
        let ready_replicas = rs.status.and_then(|s| s.ready_replicas).unwrap_or(0);

        let healthy = ready_replicas >= spec_replicas;

        Ok(HealthStatus {
            healthy,
            message: if !healthy {
                Some(format!(
                    "ReplicaSet ready: {}/{}",
                    ready_replicas, spec_replicas
                ))
            } else {
                None
            },
            last_check: Utc::now(),
        })
    }

    async fn stop(&self, deployment: &Deployment) -> Result<()> {
        // For Kubernetes, we can scale the ReplicaSet to 0
        // But this is optional - we could also just leave it running
        info!(
            "Stop requested for deployment {} (no-op for Kubernetes)",
            deployment.deployment_id
        );
        Ok(())
    }

    async fn cancel(&self, deployment: &Deployment) -> Result<()> {
        let metadata: KubernetesMetadata =
            serde_json::from_value(deployment.controller_metadata.clone())?;

        info!(
            "Cancelling deployment {} (pre-infrastructure)",
            deployment.deployment_id
        );

        // Clean up any partially created ReplicaSet
        if let (Some(rs_name), Some(namespace)) = (metadata.replicaset_name, metadata.namespace) {
            let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), &namespace);
            if let Err(e) = rs_api.delete(&rs_name, &DeleteParams::default()).await {
                // Ignore 404 errors (already deleted)
                if !e.to_string().contains("404") {
                    warn!("Error deleting ReplicaSet during cancellation: {}", e);
                }
            }
        }

        // Service and namespace are shared, don't delete
        Ok(())
    }

    async fn terminate(&self, deployment: &Deployment) -> Result<()> {
        let metadata: KubernetesMetadata =
            serde_json::from_value(deployment.controller_metadata.clone())?;

        info!(
            "Terminating deployment {} - deleting ReplicaSet",
            deployment.deployment_id
        );

        // Delete ONLY the ReplicaSet (cascading deletes pods)
        if let (Some(rs_name), Some(namespace)) = (metadata.replicaset_name, metadata.namespace) {
            let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), &namespace);
            if let Err(e) = rs_api.delete(&rs_name, &DeleteParams::default()).await {
                // Ignore 404 errors (already deleted)
                if !e.to_string().contains("404") {
                    warn!("Error deleting ReplicaSet during termination: {}", e);
                }
            }
        }

        // DO NOT delete Service, Ingress, Secret, or Namespace (shared by other deployments)
        Ok(())
    }
}
