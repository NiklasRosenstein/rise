use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use k8s_openapi::api::apps::v1::{ReplicaSet, ReplicaSetSpec};
use k8s_openapi::api::core::v1::{
    Container, ContainerPort, LocalObjectReference, Namespace, PodSpec, PodTemplateSpec, Secret,
    Service, ServicePort, ServiceSpec,
};
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::api::{Api, DeleteParams, Patch, PatchParams, PostParams};
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

// Kubernetes label and annotation constants
const LABEL_MANAGED_BY: &str = "rise.dev/managed-by";
const LABEL_PROJECT: &str = "rise.dev/project";
const LABEL_DEPLOYMENT_GROUP: &str = "rise.dev/deployment-group";
const LABEL_DEPLOYMENT_ID: &str = "rise.dev/deployment-id";
const ANNOTATION_LAST_REFRESH: &str = "rise.dev/last-refresh";

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
    SwitchingTraffic,
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
        let secret_api: Api<Secret> = Api::namespaced(self.kube_client.clone(), namespace);

        // Check if secret exists and get its last refresh time
        match secret_api.get("rise-registry-creds").await {
            Ok(existing_secret) => {
                // Check annotation for last refresh time
                if let Some(annotations) = &existing_secret.metadata.annotations {
                    if let Some(last_refresh_str) = annotations.get(ANNOTATION_LAST_REFRESH) {
                        // Parse the timestamp
                        if let Ok(last_refresh) =
                            chrono::DateTime::parse_from_rfc3339(last_refresh_str)
                        {
                            let age =
                                Utc::now().signed_duration_since(last_refresh.with_timezone(&Utc));

                            // Refresh if older than 6 hours (50% of 12-hour ECR token lifetime)
                            if age.num_seconds() < 6 * 3600 {
                                debug!(
                                    "Secret in namespace {} is fresh (age: {}s), skipping refresh",
                                    namespace,
                                    age.num_seconds()
                                );
                                return Ok(());
                            }
                        }
                    }
                }

                // If we get here, either annotation is missing or secret is old enough
                debug!(
                    "Secret in namespace {} needs refresh (missing annotation or expired)",
                    namespace
                );
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                // Secret doesn't exist, we'll create it below
                debug!(
                    "Secret in namespace {} does not exist, will create",
                    namespace
                );
            }
            Err(e) => return Err(e.into()),
        }

        // Get fresh credentials and create/update secret
        let (username, password) = provider.get_pull_credentials().await?;

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

        // Add annotation with current timestamp for tracking refresh time
        let mut annotations = BTreeMap::new();
        annotations.insert(ANNOTATION_LAST_REFRESH.to_string(), Utc::now().to_rfc3339());

        Ok(Secret {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                annotations: Some(annotations),
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
        labels.insert(LABEL_MANAGED_BY.to_string(), "rise".to_string());
        labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
        labels
    }

    /// Create deployment-specific labels
    #[allow(dead_code)] // Will be used in reconciliation implementation
    fn deployment_labels(project: &Project, deployment: &Deployment) -> BTreeMap<String, String> {
        let mut labels = Self::common_labels(project);
        labels.insert(
            LABEL_DEPLOYMENT_GROUP.to_string(),
            deployment.deployment_group.clone(),
        );
        labels.insert(
            LABEL_DEPLOYMENT_ID.to_string(),
            deployment.deployment_id.clone(),
        );
        labels
    }

    /// Create Namespace resource
    fn create_namespace(&self, project: &Project) -> Namespace {
        Namespace {
            metadata: ObjectMeta {
                name: Some(Self::namespace_name(project)),
                labels: Some(Self::common_labels(project)),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Create Service resource
    fn create_service(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &KubernetesMetadata,
    ) -> Service {
        Service {
            metadata: ObjectMeta {
                name: Some(format!("{}-svc", project.name)),
                namespace: metadata.namespace.clone(),
                labels: Some(Self::common_labels(project)),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                type_: Some("ClusterIP".to_string()),
                selector: Some(Self::deployment_labels(project, deployment)),
                ports: Some(vec![ServicePort {
                    port: 80,
                    target_port: Some(
                        k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(
                            metadata.http_port as i32,
                        ),
                    ),
                    protocol: Some("TCP".to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Create ReplicaSet resource
    fn create_replicaset(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &KubernetesMetadata,
    ) -> ReplicaSet {
        // Build image reference from deployment.image + digest or registry_url
        let image = if let Some(ref digest) = deployment.image_digest {
            let base_image = deployment.image.as_deref().unwrap_or("unknown");
            format!("{}@{}", base_image, digest)
        } else if let Some(ref registry_url) = self.registry_url {
            // registry_url should include trailing slash if needed (e.g., "host/prefix/")
            format!(
                "{}{}:{}",
                registry_url, project.name, deployment.deployment_id
            )
        } else {
            deployment
                .image
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        };

        ReplicaSet {
            metadata: ObjectMeta {
                name: Some(format!("{}-{}", project.name, deployment.deployment_id)),
                namespace: metadata.namespace.clone(),
                labels: Some(Self::deployment_labels(project, deployment)),
                ..Default::default()
            },
            spec: Some(ReplicaSetSpec {
                replicas: Some(1),
                min_ready_seconds: None,
                selector: LabelSelector {
                    match_labels: Some(Self::deployment_labels(project, deployment)),
                    ..Default::default()
                },
                template: Some(PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some(Self::deployment_labels(project, deployment)),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        image_pull_secrets: Some(vec![LocalObjectReference {
                            name: "rise-registry-creds".to_string(),
                        }]),
                        containers: vec![Container {
                            name: "app".to_string(),
                            image: Some(image),
                            ports: Some(vec![ContainerPort {
                                container_port: metadata.http_port as i32,
                                ..Default::default()
                            }]),
                            image_pull_policy: Some("Always".to_string()),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                }),
            }),
            ..Default::default()
        }
    }

    /// Create or update Ingress resource
    fn create_ingress(&self, project: &Project, metadata: &KubernetesMetadata) -> Ingress {
        let host = format!("{}.{}", project.name, self.domain_suffix);

        Ingress {
            metadata: ObjectMeta {
                name: Some(format!("{}-ingress", project.name)),
                namespace: metadata.namespace.clone(),
                labels: Some(Self::common_labels(project)),
                annotations: Some({
                    let mut annotations = BTreeMap::new();
                    annotations.insert(
                        "kubernetes.io/ingress.class".to_string(),
                        self.ingress_class.clone(),
                    );
                    annotations
                }),
                ..Default::default()
            },
            spec: Some(IngressSpec {
                rules: Some(vec![IngressRule {
                    host: Some(host),
                    http: Some(HTTPIngressRuleValue {
                        paths: vec![HTTPIngressPath {
                            path: Some("/".to_string()),
                            path_type: "Prefix".to_string(),
                            backend: IngressBackend {
                                service: Some(IngressServiceBackend {
                                    name: format!("{}-svc", project.name),
                                    port: Some(ServiceBackendPort {
                                        number: Some(80),
                                        ..Default::default()
                                    }),
                                }),
                                ..Default::default()
                            },
                        }],
                    }),
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

#[async_trait]
impl DeploymentBackend for KubernetesController {
    async fn reconcile(
        &self,
        deployment: &Deployment,
        project: &Project,
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

        // Determine status (preserve Unhealthy during recovery, otherwise Deploying)
        let status = if deployment.status == DeploymentStatus::Unhealthy {
            DeploymentStatus::Unhealthy
        } else {
            DeploymentStatus::Deploying
        };

        // Loop through phases until we hit one that requires waiting
        loop {
            match metadata.reconcile_phase {
                ReconcilePhase::NotStarted => {
                    // Initialize metadata and continue immediately
                    metadata.namespace = Some(Self::namespace_name(project));
                    metadata.service_name = Some(format!("{}-svc", project.name));
                    metadata.ingress_name = Some(format!("{}-ingress", project.name));
                    metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                    // Continue to next phase
                    continue;
                }

                ReconcilePhase::CreatingNamespace => {
                    let namespace = Self::namespace_name(project);
                    let ns_api: Api<Namespace> = Api::all(self.kube_client.clone());

                    // Check if namespace exists
                    match ns_api.get(&namespace).await {
                        Ok(_) => {
                            debug!("Namespace {} already exists", namespace);
                        }
                        Err(kube::Error::Api(ae)) if ae.code == 404 => {
                            // Create namespace
                            let ns = self.create_namespace(project);
                            ns_api.create(&PostParams::default(), &ns).await?;
                            info!("Created namespace {}", namespace);
                        }
                        Err(e) => return Err(e.into()),
                    }

                    metadata.reconcile_phase = ReconcilePhase::CreatingImagePullSecret;
                    // Continue to next phase
                    continue;
                }

                ReconcilePhase::CreatingImagePullSecret => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    if let Some(ref provider) = self.registry_provider {
                        let (username, password) = provider.get_pull_credentials().await?;
                        let secret_api: Api<Secret> =
                            Api::namespaced(self.kube_client.clone(), namespace);

                        let secret = self.create_dockerconfigjson_secret(
                            "rise-registry-creds",
                            provider.registry_host(),
                            &username,
                            &password,
                        )?;

                        // Check if secret exists, create or replace accordingly
                        match secret_api.get("rise-registry-creds").await {
                            Ok(_) => {
                                // Secret exists, replace it
                                secret_api
                                    .replace("rise-registry-creds", &PostParams::default(), &secret)
                                    .await?;
                                info!("Updated image pull secret in namespace {}", namespace);
                            }
                            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                                // Secret doesn't exist, create it
                                secret_api.create(&PostParams::default(), &secret).await?;
                                info!("Created image pull secret in namespace {}", namespace);
                            }
                            Err(e) => return Err(e.into()),
                        }
                    } else {
                        // No registry provider, skip secret creation
                        debug!("No registry provider, skipping secret creation");
                    }

                    metadata.reconcile_phase = ReconcilePhase::CreatingService;
                    // Continue to next phase
                    continue;
                }

                ReconcilePhase::CreatingService => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    let service_name = format!("{}-svc", project.name);
                    let svc_api: Api<Service> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    // Create service with server-side apply (allows future updates)
                    let svc = self.create_service(project, deployment, &metadata);
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&svc);
                    svc_api.patch(&service_name, &patch_params, &patch).await?;
                    info!("Created/updated service {}", service_name);

                    metadata.reconcile_phase = ReconcilePhase::CreatingReplicaSet;
                    // Continue to next phase
                    continue;
                }

                ReconcilePhase::CreatingReplicaSet => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    let rs_name = format!("{}-{}", project.name, deployment.deployment_id);
                    let rs_api: Api<ReplicaSet> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    // Check if ReplicaSet exists
                    match rs_api.get(&rs_name).await {
                        Ok(_) => {
                            debug!("ReplicaSet {} already exists", rs_name);
                        }
                        Err(kube::Error::Api(ae)) if ae.code == 404 => {
                            // Create ReplicaSet
                            let rs = self.create_replicaset(project, deployment, &metadata);
                            rs_api.create(&PostParams::default(), &rs).await?;
                            info!("Created ReplicaSet {}", rs_name);
                        }
                        Err(e) => return Err(e.into()),
                    }

                    metadata.replicaset_name = Some(rs_name);
                    metadata.reconcile_phase = ReconcilePhase::WaitingForReplicaSet;

                    // Return here - need to wait for pods to become ready
                    return Ok(ReconcileResult {
                        status,
                        deployment_url: None,
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                        next_reconcile: ReconcileHint::After(Duration::from_secs(5)),
                    });
                }

                ReconcilePhase::WaitingForReplicaSet => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;
                    let rs_name = metadata
                        .replicaset_name
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No ReplicaSet name in metadata"))?;

                    let rs_api: Api<ReplicaSet> =
                        Api::namespaced(self.kube_client.clone(), namespace);
                    let rs = rs_api.get(rs_name).await?;

                    let spec_replicas = rs.spec.and_then(|s| s.replicas).unwrap_or(1);
                    let ready_replicas = rs.status.and_then(|s| s.ready_replicas).unwrap_or(0);

                    if ready_replicas >= spec_replicas {
                        info!(
                            "ReplicaSet {} is ready ({}/{})",
                            rs_name, ready_replicas, spec_replicas
                        );
                        metadata.reconcile_phase = ReconcilePhase::UpdatingIngress;
                        // Continue to updating ingress
                        continue;
                    } else {
                        debug!(
                            "Waiting for ReplicaSet {} ({}/{})",
                            rs_name, ready_replicas, spec_replicas
                        );

                        // Return here - still waiting for pods
                        return Ok(ReconcileResult {
                            status,
                            deployment_url: None,
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: None,
                            next_reconcile: ReconcileHint::After(Duration::from_secs(5)),
                        });
                    }
                }

                ReconcilePhase::UpdatingIngress => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    // Only create Ingress for default deployment group
                    if deployment.deployment_group
                        == crate::deployment::models::DEFAULT_DEPLOYMENT_GROUP
                    {
                        let ingress_name = format!("{}-ingress", project.name);
                        let ingress_api: Api<Ingress> =
                            Api::namespaced(self.kube_client.clone(), namespace);

                        let ingress = self.create_ingress(project, &metadata);

                        // Use server-side apply with force for idempotent ingress updates
                        let patch_params = PatchParams::apply("rise").force();
                        let patch = Patch::Apply(&ingress);
                        ingress_api
                            .patch(&ingress_name, &patch_params, &patch)
                            .await?;
                        info!("Created/updated Ingress {}", ingress_name);
                    }

                    metadata.reconcile_phase = ReconcilePhase::WaitingForHealth;
                    // Continue to health check
                    continue;
                }

                ReconcilePhase::WaitingForHealth => {
                    // Use health_check to verify deployment is healthy before switching traffic
                    let health = self.health_check(deployment).await?;
                    let deployment_url = format!("https://{}.{}", project.name, self.domain_suffix);

                    if health.healthy {
                        info!(
                            "Deployment {} is healthy, ready for traffic switch",
                            deployment.deployment_id
                        );
                        metadata.reconcile_phase = ReconcilePhase::SwitchingTraffic;

                        // Continue to traffic switching
                        continue;
                    } else {
                        debug!(
                            "Waiting for deployment {} to become healthy",
                            deployment.deployment_id
                        );

                        return Ok(ReconcileResult {
                            status,
                            deployment_url: Some(deployment_url),
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: health.message,
                            next_reconcile: ReconcileHint::After(Duration::from_secs(5)),
                        });
                    }
                }

                ReconcilePhase::SwitchingTraffic => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    // BLUE/GREEN TRAFFIC SWITCH: Update Service selector to point to new deployment
                    let service_name = format!("{}-svc", project.name);
                    let svc_api: Api<Service> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    // Create updated service with selector pointing to this deployment
                    let svc = self.create_service(project, deployment, &metadata);

                    // Use server-side apply with force to update the service selector
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&svc);
                    svc_api.patch(&service_name, &patch_params, &patch).await?;
                    info!(
                        "Switched traffic: Service {} selector now points to deployment {}",
                        service_name, deployment.deployment_id
                    );

                    metadata.reconcile_phase = ReconcilePhase::Completed;
                    let deployment_url = format!("https://{}.{}", project.name, self.domain_suffix);

                    return Ok(ReconcileResult {
                        status: DeploymentStatus::Healthy,
                        deployment_url: Some(deployment_url),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                        next_reconcile: ReconcileHint::Default,
                    });
                }

                ReconcilePhase::Completed => {
                    // No-op, deployment is healthy
                    let deployment_url = format!("https://{}.{}", project.name, self.domain_suffix);

                    return Ok(ReconcileResult {
                        status: DeploymentStatus::Healthy,
                        deployment_url: Some(deployment_url),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                        next_reconcile: ReconcileHint::Default,
                    });
                }
            }
        }
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
