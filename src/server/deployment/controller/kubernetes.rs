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

use super::{DeploymentBackend, HealthStatus, ReconcileResult};
use crate::db::deployments as db_deployments;
use crate::db::models::{Deployment, DeploymentStatus, Project, ProjectVisibility};
use crate::db::projects as db_projects;
use crate::server::registry::RegistryProvider;
use crate::server::state::ControllerState;

// Kubernetes label and annotation constants
const LABEL_MANAGED_BY: &str = "rise.dev/managed-by";
const LABEL_PROJECT: &str = "rise.dev/project";
const LABEL_DEPLOYMENT_GROUP: &str = "rise.dev/deployment-group";
const LABEL_DEPLOYMENT_ID: &str = "rise.dev/deployment-id";
const ANNOTATION_LAST_REFRESH: &str = "rise.dev/last-refresh";

/// Finalizer name for Kubernetes namespaces
/// Added when a namespace is created for a project, removed when namespace cleanup is complete.
pub const KUBERNETES_NAMESPACE_FINALIZER: &str = "kubernetes.rise.dev/namespace";

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

/// Check if an error indicates the namespace is missing
/// Handles both direct kube::Error and anyhow::Error with kube::Error source
fn is_namespace_not_found_error(
    error: &(impl std::fmt::Display + std::fmt::Debug + ?Sized),
) -> bool {
    let error_string = error.to_string();
    // Check if error message indicates namespace is missing
    // Kubernetes returns messages like: "namespaces \"rise-test\" not found"
    error_string.contains("namespaces") && error_string.contains("not found")
}

/// Parsed ingress URL components
#[derive(Debug, Clone)]
struct IngressUrl {
    /// The hostname (e.g., "rise.dev")
    host: String,
    /// Optional path prefix (e.g., "/myapp")
    /// None for subdomain-based routing
    path_prefix: Option<String>,
}

/// Configuration parameters for the Kubernetes controller
pub struct KubernetesControllerConfig {
    pub ingress_class: String,
    pub production_ingress_url_template: String,
    pub staging_ingress_url_template: Option<String>,
    pub registry_provider: Option<Arc<dyn RegistryProvider>>,
    pub auth_backend_url: String,
    pub auth_signin_url: String,
    pub namespace_annotations: std::collections::HashMap<String, String>,
    pub ingress_annotations: std::collections::HashMap<String, String>,
    pub ingress_tls_secret_name: Option<String>,
    pub node_selector: std::collections::HashMap<String, String>,
}

/// Kubernetes controller implementation
pub struct KubernetesController {
    state: ControllerState,
    kube_client: Client,
    ingress_class: String,
    production_ingress_url_template: String,
    staging_ingress_url_template: Option<String>,
    registry_provider: Option<Arc<dyn RegistryProvider>>,
    auth_backend_url: String,
    auth_signin_url: String,
    namespace_annotations: std::collections::HashMap<String, String>,
    ingress_annotations: std::collections::HashMap<String, String>,
    ingress_tls_secret_name: Option<String>,
    node_selector: std::collections::HashMap<String, String>,
}

impl KubernetesController {
    /// Create a new Kubernetes controller
    pub fn new(
        state: ControllerState,
        kube_client: Client,
        config: KubernetesControllerConfig,
    ) -> Result<Self> {
        Ok(Self {
            state,
            kube_client,
            ingress_class: config.ingress_class,
            production_ingress_url_template: config.production_ingress_url_template,
            staging_ingress_url_template: config.staging_ingress_url_template,
            registry_provider: config.registry_provider,
            auth_backend_url: config.auth_backend_url,
            auth_signin_url: config.auth_signin_url,
            namespace_annotations: config.namespace_annotations,
            ingress_annotations: config.ingress_annotations,
            ingress_tls_secret_name: config.ingress_tls_secret_name,
            node_selector: config.node_selector,
        })
    }

    /// Start namespace cleanup loop
    ///
    /// This loop runs independently and handles namespace deletion when projects are deleted.
    /// It follows the finalizer pattern to coordinate with project deletion.
    pub fn start(self: Arc<Self>) {
        let cleanup_self = Arc::clone(&self);
        tokio::spawn(async move {
            cleanup_self.namespace_cleanup_loop().await;
        });
    }

    /// Start secret refresh loop (Kubernetes-specific)
    pub fn start_secret_refresh_loop(self: Arc<Self>, interval_duration: Duration) {
        tokio::spawn(async move {
            let mut ticker = interval(interval_duration);
            loop {
                ticker.tick().await;
                if let Err(e) = self.refresh_image_pull_secrets().await {
                    error!("Error refreshing image pull secrets: {}", e);
                }
            }
        });
    }

    /// Namespace cleanup loop - deletes namespaces for deleted projects
    ///
    /// Runs every 5 seconds and:
    /// 1. Finds projects in Deleting status with the Kubernetes namespace finalizer
    /// 2. Deletes the Kubernetes namespace
    /// 3. Removes the finalizer so project can be fully deleted
    async fn namespace_cleanup_loop(&self) {
        info!("Kubernetes namespace cleanup loop started");
        let mut ticker = interval(Duration::from_secs(5));

        loop {
            ticker.tick().await;
            if let Err(e) = self.cleanup_namespaces().await {
                error!("Error in Kubernetes namespace cleanup loop: {}", e);
            }
        }
    }

    /// Process namespace cleanup for all deleting projects with Kubernetes finalizer
    async fn cleanup_namespaces(&self) -> Result<()> {
        // Find projects marked for deletion that still have Kubernetes namespace finalizer
        let projects = db_projects::find_deleting_with_finalizer(
            &self.state.db_pool,
            KUBERNETES_NAMESPACE_FINALIZER,
            10,
        )
        .await?;

        for project in projects {
            debug!(
                "Cleaning up Kubernetes namespace for project: {}",
                project.name
            );

            let namespace_name = Self::namespace_name(&project);
            let ns_api: Api<Namespace> = Api::all(self.kube_client.clone());

            // Try to delete the namespace
            match ns_api
                .delete(&namespace_name, &DeleteParams::default())
                .await
            {
                Ok(_) => {
                    info!(
                        "Deleted Kubernetes namespace '{}' for project: {}",
                        namespace_name, project.name
                    );
                }
                Err(kube::Error::Api(err)) if err.code == 404 => {
                    // Namespace doesn't exist, that's fine
                    info!(
                        "Kubernetes namespace '{}' did not exist for project: {} (already deleted)",
                        namespace_name, project.name
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to delete Kubernetes namespace '{}' for project {}: {}",
                        namespace_name, project.name, e
                    );
                    // Continue to next project, will retry on next loop
                    continue;
                }
            }

            // Remove finalizer so project can be deleted
            db_projects::remove_finalizer(
                &self.state.db_pool,
                project.id,
                KUBERNETES_NAMESPACE_FINALIZER,
            )
            .await?;
            info!(
                "Removed Kubernetes namespace finalizer from project: {}, cleanup complete",
                project.name
            );
        }

        Ok(())
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
    fn namespace_name(project: &Project) -> String {
        format!("rise-{}", project.name)
    }

    /// Sanitize a string to be a valid Kubernetes label value
    /// Replaces sequences of invalid characters with '--' to avoid collisions
    /// (e.g., "mr/26" → "mr--26", "mr-26" → "mr-26")
    /// Ensures it matches the regex: (([A-Za-z0-9][-A-Za-z0-9_.]*)?[A-Za-z0-9])?
    fn sanitize_label_value(value: &str) -> String {
        let mut result = String::new();
        let mut last_was_invalid = false;

        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                result.push(ch);
                last_was_invalid = false;
            } else {
                // Replace invalid character(s) with '--' (only once per sequence)
                if !last_was_invalid {
                    result.push_str("--");
                    last_was_invalid = true;
                }
            }
        }

        // Ensure it doesn't start or end with invalid characters
        result.trim_matches('-').to_string()
    }

    /// Get the escaped deployment group name for use in resource names
    fn escaped_group_name(deployment_group: &str) -> String {
        if deployment_group == crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP {
            "default".to_string()
        } else {
            Self::sanitize_label_value(deployment_group)
        }
    }

    /// Get Service name for a deployment group
    fn service_name(_project: &Project, deployment: &Deployment) -> String {
        Self::escaped_group_name(&deployment.deployment_group)
    }

    /// Get Ingress name for a deployment group
    fn ingress_name(_project: &Project, deployment: &Deployment) -> String {
        Self::escaped_group_name(&deployment.deployment_group)
    }

    /// Get hostname (without path) for deployment
    fn hostname(&self, project: &Project, deployment: &Deployment) -> String {
        let url = self.resolved_ingress_url(project, deployment);
        let parsed = Self::parse_ingress_url(&url);
        parsed.host
    }

    /// Parse a fully-resolved URL into (host, path_prefix)
    ///
    /// Examples:
    ///   "myapp.apps.rise.dev" → IngressUrl { host: "myapp.apps.rise.dev", path_prefix: None }
    ///   "rise.dev/myapp" → IngressUrl { host: "rise.dev", path_prefix: Some("/myapp") }
    fn parse_ingress_url(url: &str) -> IngressUrl {
        match url.find('/') {
            Some(slash_pos) => {
                let host = url[..slash_pos].to_string();
                let path = url[slash_pos..].to_string();
                IngressUrl {
                    host,
                    path_prefix: Some(path),
                }
            }
            None => IngressUrl {
                host: url.to_string(),
                path_prefix: None,
            },
        }
    }

    /// Get fully resolved ingress URL with placeholders replaced
    fn resolved_ingress_url(&self, project: &Project, deployment: &Deployment) -> String {
        if deployment.deployment_group
            == crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP
        {
            self.production_ingress_url_template
                .replace("{project_name}", &project.name)
        } else if let Some(ref staging_template) = self.staging_ingress_url_template {
            staging_template
                .replace("{project_name}", &project.name)
                .replace(
                    "{deployment_group}",
                    &Self::escaped_group_name(&deployment.deployment_group),
                )
        } else {
            // Fallback: insert "-{group}" before first dot
            let base_url = self
                .production_ingress_url_template
                .replace("{project_name}", &project.name);
            if let Some(dot_pos) = base_url.find('.') {
                format!(
                    "{}-{}{}",
                    &base_url[..dot_pos],
                    Self::escaped_group_name(&deployment.deployment_group),
                    &base_url[dot_pos..]
                )
            } else {
                format!(
                    "{}-{}",
                    base_url,
                    Self::escaped_group_name(&deployment.deployment_group)
                )
            }
        }
    }

    /// Get parsed URL components for Ingress creation
    fn ingress_url_components(&self, project: &Project, deployment: &Deployment) -> IngressUrl {
        let url = self.resolved_ingress_url(project, deployment);
        Self::parse_ingress_url(&url)
    }

    /// Get full ingress URL for deployment_url field
    fn full_ingress_url(&self, project: &Project, deployment: &Deployment) -> String {
        self.resolved_ingress_url(project, deployment)
    }

    /// Clean up deployment group resources (Service and Ingress) if no other deployments exist in the group
    async fn cleanup_group_resources_if_empty(
        &self,
        deployment: &Deployment,
        metadata: &KubernetesMetadata,
    ) -> Result<()> {
        let namespace = metadata
            .namespace
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

        // Check if there are other active deployments in this group
        use crate::db::deployments as db_deployments;
        let other_in_group = db_deployments::list_for_project_and_group(
            &self.state.db_pool,
            deployment.project_id,
            Some(&deployment.deployment_group),
            Some(100), // Get up to 100 deployments to check
            None,
        )
        .await?;

        // Count deployments that aren't this one and aren't in terminal/cleanup states
        let active_count = other_in_group
            .iter()
            .filter(|d| {
                d.id != deployment.id
                    && !matches!(
                        d.status,
                        DeploymentStatus::Terminating
                            | DeploymentStatus::Cancelled
                            | DeploymentStatus::Stopped
                            | DeploymentStatus::Failed
                            | DeploymentStatus::Superseded
                            | DeploymentStatus::Expired
                    )
            })
            .count();

        if active_count == 0 {
            info!(
                "Last deployment in group '{}', cleaning up Service and Ingress",
                deployment.deployment_group
            );

            // Get project info to construct resource names
            use crate::db::projects as db_projects;
            let project = db_projects::find_by_id(&self.state.db_pool, deployment.project_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

            // Delete Service
            let service_name = Self::service_name(&project, deployment);
            let svc_api: Api<Service> = Api::namespaced(self.kube_client.clone(), namespace);
            if let Err(e) = svc_api
                .delete(&service_name, &DeleteParams::default())
                .await
            {
                if !e.to_string().contains("404") {
                    warn!("Error deleting Service {}: {}", service_name, e);
                }
            } else {
                info!("Deleted Service {} for empty group", service_name);
            }

            // Delete Ingress
            let ingress_name = Self::ingress_name(&project, deployment);
            let ingress_api: Api<Ingress> = Api::namespaced(self.kube_client.clone(), namespace);
            if let Err(e) = ingress_api
                .delete(&ingress_name, &DeleteParams::default())
                .await
            {
                if !e.to_string().contains("404") {
                    warn!("Error deleting Ingress {}: {}", ingress_name, e);
                }
            } else {
                info!("Deleted Ingress {} for empty group", ingress_name);
            }
        } else {
            debug!(
                "Group '{}' still has {} active deployment(s), keeping resources",
                deployment.deployment_group, active_count
            );
        }

        Ok(())
    }

    /// Check pods for irrecoverable errors (e.g., InvalidImageName, ImagePullBackOff)
    /// Returns (is_failed, error_message) tuple
    async fn check_pod_errors(
        &self,
        namespace: &str,
        rs_name: &str,
    ) -> Result<(bool, Option<String>)> {
        use k8s_openapi::api::core::v1::Pod;

        let pod_api: Api<Pod> = Api::namespaced(self.kube_client.clone(), namespace);

        // List pods owned by this ReplicaSet
        let pods = pod_api
            .list(&kube::api::ListParams::default().labels(&format!(
                "rise.dev/deployment-id={}",
                rs_name.rsplit_once('-').map(|(_, id)| id).unwrap_or(rs_name)
            )))
            .await?;

        for pod in pods.items {
            if let Some(status) = pod.status {
                // Check container statuses for irrecoverable errors
                if let Some(container_statuses) = status.container_statuses {
                    for container_status in container_statuses {
                        if let Some(waiting) = container_status
                            .state
                            .as_ref()
                            .and_then(|s| s.waiting.as_ref())
                        {
                            let reason = waiting.reason.as_deref().unwrap_or("");

                            // Check for irrecoverable errors
                            let is_irrecoverable = matches!(
                                reason,
                                "InvalidImageName"
                                    | "ErrImagePull"
                                    | "ImageInspectError"
                                    | "CrashLoopBackOff"
                                    | "CreateContainerConfigError"
                                    | "CreateContainerError"
                                    | "RunContainerError"
                            );

                            if is_irrecoverable {
                                let message = waiting.message.as_deref().unwrap_or(reason);
                                warn!(
                                    "Pod {} has irrecoverable error: {} - {}",
                                    pod.metadata.name.as_deref().unwrap_or("unknown"),
                                    reason,
                                    message
                                );
                                return Ok((true, Some(format!("{}: {}", reason, message))));
                            }
                        }

                        // Check for terminated containers with non-zero exit codes
                        if let Some(terminated) = container_status
                            .state
                            .as_ref()
                            .and_then(|s| s.terminated.as_ref())
                        {
                            if terminated.exit_code != 0 {
                                let reason =
                                    terminated.reason.as_deref().unwrap_or("ContainerFailed");
                                let default_message =
                                    format!("Exit code: {}", terminated.exit_code);
                                let message =
                                    terminated.message.as_deref().unwrap_or(&default_message);

                                // Only fail if container has restarted multiple times
                                if container_status.restart_count >= 3 {
                                    warn!(
                                        "Pod {} container has failed {} times: {} - {}",
                                        pod.metadata.name.as_deref().unwrap_or("unknown"),
                                        container_status.restart_count,
                                        reason,
                                        message
                                    );
                                    return Ok((
                                        true,
                                        Some(format!(
                                            "{}: {} (restarts: {})",
                                            reason, message, container_status.restart_count
                                        )),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok((false, None))
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
            Self::sanitize_label_value(&deployment.deployment_group),
        );
        labels.insert(
            LABEL_DEPLOYMENT_ID.to_string(),
            deployment.deployment_id.clone(),
        );
        labels
    }

    /// Create Namespace resource
    fn create_namespace(&self, project: &Project) -> Namespace {
        // Convert HashMap to BTreeMap for annotations
        let annotations = if !self.namespace_annotations.is_empty() {
            Some(
                self.namespace_annotations
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
        } else {
            None
        };

        Namespace {
            metadata: ObjectMeta {
                name: Some(Self::namespace_name(project)),
                labels: Some(Self::common_labels(project)),
                annotations,
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
                name: Some(Self::service_name(project, deployment)),
                namespace: metadata.namespace.clone(),
                labels: Some(Self::common_labels(project)),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                type_: Some("ClusterIP".to_string()),
                selector: Some(Self::deployment_labels(project, deployment)),
                ports: Some(vec![ServicePort {
                    name: Some("http".to_string()),
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

    /// Load and decrypt environment variables for a deployment
    async fn load_env_vars(
        &self,
        deployment_id: uuid::Uuid,
    ) -> Result<Vec<k8s_openapi::api::core::v1::EnvVar>> {
        use k8s_openapi::api::core::v1::EnvVar;

        // Load and decrypt environment variables using shared helper
        let env_vars = crate::db::env_vars::load_deployment_env_vars_decrypted(
            &self.state.db_pool,
            deployment_id,
            self.state.encryption_provider.as_deref(),
        )
        .await?;

        // Format as Kubernetes EnvVar objects
        Ok(env_vars
            .into_iter()
            .map(|(key, value)| EnvVar {
                name: key,
                value: Some(value),
                ..Default::default()
            })
            .collect())
    }

    /// Create ReplicaSet resource
    fn create_replicaset(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &KubernetesMetadata,
        env_vars: Vec<k8s_openapi::api::core::v1::EnvVar>,
    ) -> ReplicaSet {
        // Build image reference from deployment.image_digest or registry_provider
        let image = if let Some(ref image_digest) = deployment.image_digest {
            // image_digest already contains the full reference with digest
            // (e.g., "docker.io/library/nginx@sha256:...")
            image_digest.clone()
        } else if let Some(ref registry_provider) = self.registry_provider {
            // Use Internal variant to ignore client_registry_url configuration (Kubernetes always uses internal registry)
            registry_provider.get_image_tag(
                &project.name,
                &deployment.deployment_id,
                crate::server::registry::ImageTagType::Internal,
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
                            env: if env_vars.is_empty() {
                                None
                            } else {
                                Some(env_vars)
                            },
                            ..Default::default()
                        }],
                        node_selector: if self.node_selector.is_empty() {
                            None
                        } else {
                            Some(self.node_selector.clone().into_iter().collect())
                        },
                        ..Default::default()
                    }),
                }),
            }),
            ..Default::default()
        }
    }

    /// Create or update Ingress resource
    fn create_ingress(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &KubernetesMetadata,
    ) -> Ingress {
        let url_components = self.ingress_url_components(project, deployment);

        // Start with user-provided annotations from config (convert HashMap to BTreeMap)
        let mut annotations: BTreeMap<String, String> = self
            .ingress_annotations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Add Nginx rewrite annotations for sub-path routing
        if let Some(ref path) = url_components.path_prefix {
            // Strip path prefix: /myapp/foo → /foo
            annotations.insert(
                "nginx.ingress.kubernetes.io/rewrite-target".to_string(),
                "/$2".to_string(),
            );

            // Pass original prefix as header using the built-in annotation
            annotations.insert(
                "nginx.ingress.kubernetes.io/x-forwarded-prefix".to_string(),
                path.trim_end_matches('/').to_string(),
            );
        }

        if matches!(project.visibility, ProjectVisibility::Private) {
            // Add Nginx auth annotations for private projects
            let auth_url = format!(
                "{}/auth/ingress?project={}",
                self.auth_backend_url, project.name
            );
            let signin_url = format!(
                "{}/auth/signin?project={}&redirect=$escaped_request_uri",
                self.auth_signin_url,
                urlencoding::encode(&project.name)
            );

            annotations.insert("nginx.ingress.kubernetes.io/auth-url".to_string(), auth_url);
            annotations.insert(
                "nginx.ingress.kubernetes.io/auth-signin".to_string(),
                signin_url,
            );
            annotations.insert(
                "nginx.ingress.kubernetes.io/auth-response-headers".to_string(),
                "X-Auth-Request-Email,X-Auth-Request-User".to_string(),
            );
        }
        // Public projects have no auth annotations

        // Determine path and path type based on routing mode
        let (ingress_path, path_type) = if let Some(ref path) = url_components.path_prefix {
            // Regex: /myapp(/|$)(.*)
            // Matches: /myapp, /myapp/, /myapp/anything
            // $2 captures everything after /myapp/
            let pattern = format!("{}(/|$)(.*)", path.trim_end_matches('/'));
            (pattern, "ImplementationSpecific")
        } else {
            ("/".to_string(), "Prefix")
        };

        // Build TLS configuration if secret name is provided
        let tls = self.ingress_tls_secret_name.as_ref().map(|secret_name| {
            vec![k8s_openapi::api::networking::v1::IngressTLS {
                hosts: Some(vec![url_components.host.clone()]),
                secret_name: Some(secret_name.clone()),
            }]
        });

        Ingress {
            metadata: ObjectMeta {
                name: Some(Self::ingress_name(project, deployment)),
                namespace: metadata.namespace.clone(),
                labels: Some(Self::common_labels(project)),
                annotations: if !annotations.is_empty() {
                    Some(annotations)
                } else {
                    None
                },
                ..Default::default()
            },
            spec: Some(IngressSpec {
                ingress_class_name: Some(self.ingress_class.clone()),
                tls,
                rules: Some(vec![IngressRule {
                    host: Some(url_components.host.clone()),
                    http: Some(HTTPIngressRuleValue {
                        paths: vec![HTTPIngressPath {
                            path: Some(ingress_path),
                            path_type: path_type.to_string(),
                            backend: IngressBackend {
                                service: Some(IngressServiceBackend {
                                    name: Self::service_name(project, deployment),
                                    port: Some(ServiceBackendPort {
                                        name: Some("http".to_string()),
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
        // Wait for image to be pushed before starting Kubernetes deployment
        // The image must be available in the registry before we can create pods
        if !matches!(
            deployment.status,
            DeploymentStatus::Pushed
                | DeploymentStatus::Deploying
                | DeploymentStatus::Unhealthy
                | DeploymentStatus::Healthy
        ) {
            debug!(
                "Deployment {} not yet pushed (status={:?}), skipping Kubernetes reconciliation",
                deployment.deployment_id, deployment.status
            );
            return Ok(ReconcileResult {
                status: deployment.status.clone(),
                deployment_url: None,
                controller_metadata: deployment.controller_metadata.clone(),
                error_message: None,
            });
        }

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

        // Recovery logic: If deployment is Unhealthy and ReplicaSet is missing, reset to recreate it
        if deployment.status == DeploymentStatus::Unhealthy
            && matches!(
                metadata.reconcile_phase,
                ReconcilePhase::WaitingForReplicaSet
                    | ReconcilePhase::UpdatingIngress
                    | ReconcilePhase::WaitingForHealth
                    | ReconcilePhase::SwitchingTraffic
                    | ReconcilePhase::Completed
            )
        {
            // Check if ReplicaSet still exists
            if let (Some(ref rs_name), Some(ref namespace)) =
                (&metadata.replicaset_name, &metadata.namespace)
            {
                let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), namespace);
                match rs_api.get(rs_name).await {
                    Ok(_) => {
                        // ReplicaSet exists, continue normal reconciliation
                        debug!(
                            "ReplicaSet {} exists, continuing normal reconciliation",
                            rs_name
                        );
                    }
                    Err(kube::Error::Api(ae)) if ae.code == 404 => {
                        // ReplicaSet is missing - reset to recreate it
                        warn!(
                            "Unhealthy deployment {} has missing ReplicaSet {} in phase {:?}, resetting to recreate",
                            deployment.deployment_id, rs_name, metadata.reconcile_phase
                        );

                        // Reset to CreatingReplicaSet to recreate the ReplicaSet
                        metadata.reconcile_phase = ReconcilePhase::CreatingReplicaSet;
                        info!(
                            "Reset reconciliation phase to CreatingReplicaSet for deployment {}",
                            deployment.deployment_id
                        );
                    }
                    Err(e) => {
                        // Other errors, continue normal reconciliation (will likely fail and retry)
                        warn!(
                            "Error checking ReplicaSet {} for unhealthy deployment {}: {}",
                            rs_name, deployment.deployment_id, e
                        );
                    }
                }
            } else {
                // No ReplicaSet name in metadata - this shouldn't happen in these phases
                warn!(
                    "Unhealthy deployment {} in phase {:?} has no ReplicaSet name in metadata",
                    deployment.deployment_id, metadata.reconcile_phase
                );
            }
        }

        // Loop through phases until we hit one that requires waiting
        loop {
            match metadata.reconcile_phase {
                ReconcilePhase::NotStarted => {
                    // Initialize metadata and continue immediately
                    metadata.namespace = Some(Self::namespace_name(project));
                    metadata.service_name = Some(Self::service_name(project, deployment));
                    metadata.ingress_name = Some(Self::ingress_name(project, deployment));
                    metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                    // Continue to next phase
                    continue;
                }

                ReconcilePhase::CreatingNamespace => {
                    let namespace = Self::namespace_name(project);
                    let ns_api: Api<Namespace> = Api::all(self.kube_client.clone());

                    // Check if namespace exists
                    match ns_api.get(&namespace).await {
                        Ok(existing_ns) => {
                            debug!("Namespace {} already exists, checking for drift", namespace);

                            // Check if annotations have drifted
                            let desired_annotations: BTreeMap<String, String> = self
                                .namespace_annotations
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();

                            let current_annotations = existing_ns
                                .metadata
                                .annotations
                                .as_ref()
                                .cloned()
                                .unwrap_or_default();

                            // Check if our managed annotations match
                            let mut needs_update = false;
                            for (key, desired_value) in &desired_annotations {
                                if current_annotations.get(key) != Some(desired_value) {
                                    needs_update = true;
                                    break;
                                }
                            }

                            if needs_update {
                                // Merge current annotations with desired ones
                                let mut updated_annotations = current_annotations;
                                for (key, value) in desired_annotations {
                                    updated_annotations.insert(key, value);
                                }

                                // Create updated namespace with merged annotations
                                let mut updated_ns = existing_ns.clone();
                                updated_ns.metadata.annotations = Some(updated_annotations);

                                // Apply the update
                                let patch_params = PatchParams::apply("rise-controller");
                                let patch = Patch::Apply(&updated_ns);
                                ns_api.patch(&namespace, &patch_params, &patch).await?;
                                info!(
                                    "Updated namespace {} annotations to match configuration",
                                    namespace
                                );
                            }
                        }
                        Err(kube::Error::Api(ae)) if ae.code == 404 => {
                            // Create namespace
                            let ns = self.create_namespace(project);
                            ns_api.create(&PostParams::default(), &ns).await?;
                            info!("Created namespace {}", namespace);
                        }
                        Err(e) => return Err(e.into()),
                    }

                    // Add Kubernetes namespace finalizer if not already present
                    // This ensures namespace cleanup when project is deleted
                    if !project
                        .finalizers
                        .contains(&KUBERNETES_NAMESPACE_FINALIZER.to_string())
                    {
                        db_projects::add_finalizer(
                            &self.state.db_pool,
                            project.id,
                            KUBERNETES_NAMESPACE_FINALIZER,
                        )
                        .await?;
                        debug!(
                            "Added Kubernetes namespace finalizer to project: {}",
                            project.name
                        );
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
                                match secret_api
                                    .replace("rise-registry-creds", &PostParams::default(), &secret)
                                    .await
                                {
                                    Ok(_) => {
                                        info!(
                                            project = project.name,
                                            namespace = namespace,
                                            "ImagePullSecret replaced (any drift corrected)"
                                        );
                                    }
                                    Err(e) if is_namespace_not_found_error(&e) => {
                                        warn!(
                                            "Namespace missing during ImagePullSecret replace, resetting to CreatingNamespace"
                                        );
                                        metadata.reconcile_phase =
                                            ReconcilePhase::CreatingNamespace;
                                        metadata.namespace = None;
                                        continue;
                                    }
                                    Err(e) => return Err(e.into()),
                                }
                            }
                            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                                // Secret doesn't exist, create it
                                match secret_api.create(&PostParams::default(), &secret).await {
                                    Ok(_) => {
                                        info!(
                                            project = project.name,
                                            namespace = namespace,
                                            "ImagePullSecret created"
                                        );
                                    }
                                    Err(e) if is_namespace_not_found_error(&e) => {
                                        warn!(
                                            "Namespace missing during ImagePullSecret create, resetting to CreatingNamespace"
                                        );
                                        metadata.reconcile_phase =
                                            ReconcilePhase::CreatingNamespace;
                                        metadata.namespace = None;
                                        continue;
                                    }
                                    Err(e) => return Err(e.into()),
                                }
                            }
                            Err(e) if is_namespace_not_found_error(&e) => {
                                warn!(
                                    "Namespace missing during ImagePullSecret get, resetting to CreatingNamespace"
                                );
                                metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                                metadata.namespace = None;
                                continue;
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

                    let service_name = Self::service_name(project, deployment);
                    let svc_api: Api<Service> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    // Create service with server-side apply (allows future updates)
                    let svc = self.create_service(project, deployment, &metadata);
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&svc);
                    let result = match svc_api.patch(&service_name, &patch_params, &patch).await {
                        Ok(r) => r,
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!(
                                "Namespace missing during Service creation, resetting to CreatingNamespace"
                            );
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    };

                    info!(
                        project = project.name,
                        deployment_id = %deployment.id,
                        resource_version = ?result.metadata.resource_version,
                        "Service applied (any drift corrected)"
                    );

                    metadata.reconcile_phase = ReconcilePhase::CreatingReplicaSet;
                    // Continue to next phase
                    continue;
                }

                ReconcilePhase::CreatingReplicaSet => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    // Load and decrypt environment variables
                    let env_vars = self.load_env_vars(deployment.id).await?;

                    let rs_name = format!("{}-{}", project.name, deployment.deployment_id);
                    let rs_api: Api<ReplicaSet> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    // Check if ReplicaSet exists
                    match rs_api.get(&rs_name).await {
                        Ok(existing_rs) => {
                            // ReplicaSet exists - check for drift
                            let desired_rs = self.create_replicaset(
                                project,
                                deployment,
                                &metadata,
                                env_vars.clone(),
                            );

                            if self.replicaset_has_drifted(&existing_rs, &desired_rs) {
                                info!(
                                    project = project.name,
                                    deployment_id = %deployment.id,
                                    "ReplicaSet has drifted, recreating"
                                );

                                // Delete and wait for deletion to complete
                                if let Err(e) =
                                    self.delete_and_wait_replicaset(&rs_api, &rs_name).await
                                {
                                    if is_namespace_not_found_error(&e) {
                                        warn!(
                                            "Namespace missing during ReplicaSet deletion, resetting to CreatingNamespace"
                                        );
                                        metadata.reconcile_phase =
                                            ReconcilePhase::CreatingNamespace;
                                        metadata.namespace = None;
                                        continue;
                                    } else {
                                        return Err(e);
                                    }
                                }

                                // Create new ReplicaSet
                                match rs_api.create(&PostParams::default(), &desired_rs).await {
                                    Ok(_) => {
                                        info!(
                                            project = project.name,
                                            deployment_id = %deployment.id,
                                            "ReplicaSet recreated after drift detected"
                                        );
                                    }
                                    Err(e) if is_namespace_not_found_error(&e) => {
                                        warn!(
                                            "Namespace missing during ReplicaSet creation, resetting to CreatingNamespace"
                                        );
                                        metadata.reconcile_phase =
                                            ReconcilePhase::CreatingNamespace;
                                        metadata.namespace = None;
                                        continue;
                                    }
                                    Err(e) => return Err(e.into()),
                                }
                            } else {
                                debug!(
                                    project = project.name,
                                    deployment_id = %deployment.id,
                                    "ReplicaSet exists and matches desired state"
                                );
                            }
                        }
                        Err(kube::Error::Api(ae)) if ae.code == 404 => {
                            // ReplicaSet doesn't exist - create it
                            let rs =
                                self.create_replicaset(project, deployment, &metadata, env_vars);
                            match rs_api.create(&PostParams::default(), &rs).await {
                                Ok(_) => {
                                    info!(
                                        project = project.name,
                                        deployment_id = %deployment.id,
                                        "ReplicaSet created"
                                    );
                                }
                                Err(e) if is_namespace_not_found_error(&e) => {
                                    warn!(
                                        "Namespace missing during ReplicaSet creation, resetting to CreatingNamespace"
                                    );
                                    metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                                    metadata.namespace = None;
                                    continue;
                                }
                                Err(e) => return Err(e.into()),
                            }
                        }
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!(
                                "Namespace missing during ReplicaSet get, resetting to CreatingNamespace"
                            );
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
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

                    // Check for irrecoverable pod errors first
                    let (has_errors, error_msg) = match self
                        .check_pod_errors(namespace, rs_name)
                        .await
                    {
                        Ok((errors, msg)) => (errors, msg),
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!("Namespace missing during pod error check, resetting to CreatingNamespace");
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e),
                    };
                    if has_errors {
                        error!(
                            "Deployment {} has irrecoverable pod errors: {}",
                            deployment.deployment_id,
                            error_msg.as_ref().unwrap_or(&"Unknown error".to_string())
                        );

                        // Mark deployment as Failed
                        return Ok(ReconcileResult {
                            status: DeploymentStatus::Failed,
                            deployment_url: None,
                            controller_metadata: serde_json::to_value(&metadata)?,
                            error_message: error_msg,
                        });
                    }

                    let rs_api: Api<ReplicaSet> =
                        Api::namespaced(self.kube_client.clone(), namespace);
                    let rs = match rs_api.get(rs_name).await {
                        Ok(r) => r,
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!("Namespace missing during ReplicaSet get in WaitingForReplicaSet, resetting to CreatingNamespace");
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    };

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
                        });
                    }
                }

                ReconcilePhase::UpdatingIngress => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    // Create Ingress for all deployment groups (each group gets its own hostname)
                    let ingress_name = Self::ingress_name(project, deployment);
                    let ingress_api: Api<Ingress> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    let ingress = self.create_ingress(project, deployment, &metadata);

                    // Use server-side apply with force for idempotent ingress updates
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&ingress);
                    let result = match ingress_api
                        .patch(&ingress_name, &patch_params, &patch)
                        .await
                    {
                        Ok(r) => r,
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!(
                                "Namespace missing during Ingress creation, resetting to CreatingNamespace"
                            );
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    };

                    info!(
                        project = project.name,
                        deployment_id = %deployment.id,
                        resource_version = ?result.metadata.resource_version,
                        "Ingress applied (any drift corrected)"
                    );

                    metadata.reconcile_phase = ReconcilePhase::WaitingForHealth;
                    // Continue to health check
                    continue;
                }

                ReconcilePhase::WaitingForHealth => {
                    // Use health_check to verify deployment is healthy before switching traffic
                    let health = self.health_check(deployment).await?;
                    let deployment_url =
                        format!("https://{}", self.full_ingress_url(project, deployment));

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
                        });
                    }
                }

                ReconcilePhase::SwitchingTraffic => {
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    // BLUE/GREEN TRAFFIC SWITCH: Update Service selector to point to new deployment
                    let service_name = Self::service_name(project, deployment);
                    let svc_api: Api<Service> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    // Create updated service with selector pointing to this deployment
                    let svc = self.create_service(project, deployment, &metadata);

                    // Use server-side apply with force to update the service selector
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&svc);
                    match svc_api.patch(&service_name, &patch_params, &patch).await {
                        Ok(_) => {
                            info!(
                                "Switched traffic: Service {} selector now points to deployment {}",
                                service_name, deployment.deployment_id
                            );
                        }
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!(
                                "Namespace missing during traffic switch, resetting to CreatingNamespace"
                            );
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }

                    metadata.reconcile_phase = ReconcilePhase::Completed;
                    let deployment_url =
                        format!("https://{}", self.full_ingress_url(project, deployment));

                    return Ok(ReconcileResult {
                        status: DeploymentStatus::Healthy,
                        deployment_url: Some(deployment_url),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
                    });
                }

                ReconcilePhase::Completed => {
                    // Check and correct drift on all resources
                    let namespace = metadata
                        .namespace
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No namespace in metadata"))?;

                    // Load and decrypt environment variables for drift detection
                    let env_vars = self.load_env_vars(deployment.id).await?;

                    // 1. Re-apply Service to correct any drift
                    let service_name = Self::service_name(project, deployment);
                    let svc_api: Api<Service> =
                        Api::namespaced(self.kube_client.clone(), namespace);
                    let svc = self.create_service(project, deployment, &metadata);
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&svc);
                    match svc_api.patch(&service_name, &patch_params, &patch).await {
                        Ok(_) => {
                            debug!(
                                project = project.name,
                                deployment_id = %deployment.id,
                                "Service drift check completed"
                            );
                        }
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!("Namespace missing during Completed phase (Service), resetting to CreatingNamespace");
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }

                    // 2. Re-apply Ingress to correct any drift
                    let ingress_name = Self::ingress_name(project, deployment);
                    let ingress_api: Api<Ingress> =
                        Api::namespaced(self.kube_client.clone(), namespace);
                    let ingress = self.create_ingress(project, deployment, &metadata);
                    let patch_params = PatchParams::apply("rise").force();
                    let patch = Patch::Apply(&ingress);
                    match ingress_api
                        .patch(&ingress_name, &patch_params, &patch)
                        .await
                    {
                        Ok(_) => {
                            debug!(
                                project = project.name,
                                deployment_id = %deployment.id,
                                "Ingress drift check completed"
                            );
                        }
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!("Namespace missing during Completed phase (Ingress), resetting to CreatingNamespace");
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }

                    // 3. Check ReplicaSet for drift
                    let rs_name = metadata
                        .replicaset_name
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No ReplicaSet name in metadata"))?;
                    let rs_api: Api<ReplicaSet> =
                        Api::namespaced(self.kube_client.clone(), namespace);

                    match rs_api.get(rs_name).await {
                        Ok(existing_rs) => {
                            let desired_rs = self.create_replicaset(
                                project,
                                deployment,
                                &metadata,
                                env_vars.clone(),
                            );

                            if self.replicaset_has_drifted(&existing_rs, &desired_rs) {
                                info!(
                                    project = project.name,
                                    deployment_id = %deployment.id,
                                    "ReplicaSet has drifted in Completed phase, recreating"
                                );

                                // Delete and wait for deletion to complete
                                if let Err(e) =
                                    self.delete_and_wait_replicaset(&rs_api, rs_name).await
                                {
                                    if is_namespace_not_found_error(&e) {
                                        warn!("Namespace missing during ReplicaSet deletion in Completed phase, resetting to CreatingNamespace");
                                        metadata.reconcile_phase =
                                            ReconcilePhase::CreatingNamespace;
                                        metadata.namespace = None;
                                        continue;
                                    } else {
                                        return Err(e);
                                    }
                                }

                                // Create new ReplicaSet
                                match rs_api.create(&PostParams::default(), &desired_rs).await {
                                    Ok(_) => {
                                        info!(
                                            project = project.name,
                                            deployment_id = %deployment.id,
                                            "ReplicaSet recreated after drift detected in Completed phase"
                                        );

                                        // Move back to WaitingForReplicaSet to ensure it becomes ready
                                        metadata.reconcile_phase =
                                            ReconcilePhase::WaitingForReplicaSet;
                                        return Ok(ReconcileResult {
                                            status,
                                            deployment_url: None,
                                            controller_metadata: serde_json::to_value(&metadata)?,
                                            error_message: None,
                                        });
                                    }
                                    Err(e) if is_namespace_not_found_error(&e) => {
                                        warn!("Namespace missing during ReplicaSet creation in Completed phase, resetting to CreatingNamespace");
                                        metadata.reconcile_phase =
                                            ReconcilePhase::CreatingNamespace;
                                        metadata.namespace = None;
                                        continue;
                                    }
                                    Err(e) => return Err(e.into()),
                                }

                                info!(
                                    project = project.name,
                                    deployment_id = %deployment.id,
                                    "ReplicaSet recreated after drift detected in Completed phase"
                                );

                                // Move back to WaitingForReplicaSet to ensure it becomes ready
                                metadata.reconcile_phase = ReconcilePhase::WaitingForReplicaSet;
                                let metadata_json = serde_json::to_value(&metadata)?;
                                db_deployments::update_controller_metadata(
                                    &self.state.db_pool,
                                    deployment.id,
                                    &metadata_json,
                                )
                                .await?;

                                return Ok(ReconcileResult {
                                    status: DeploymentStatus::Deploying,
                                    deployment_url: Some(format!(
                                        "https://{}",
                                        self.hostname(project, deployment)
                                    )),
                                    controller_metadata: serde_json::to_value(&metadata)?,
                                    error_message: None,
                                });
                            } else {
                                debug!(
                                    project = project.name,
                                    deployment_id = %deployment.id,
                                    "ReplicaSet drift check completed - no drift detected"
                                );
                            }
                        }
                        Err(kube::Error::Api(ae)) if ae.code == 404 => {
                            // ReplicaSet is missing - recreate it
                            warn!(
                                project = project.name,
                                deployment_id = %deployment.id,
                                "ReplicaSet missing in Completed phase, recreating"
                            );

                            metadata.reconcile_phase = ReconcilePhase::CreatingReplicaSet;
                            let metadata_json = serde_json::to_value(&metadata)?;
                            db_deployments::update_controller_metadata(
                                &self.state.db_pool,
                                deployment.id,
                                &metadata_json,
                            )
                            .await?;

                            return Ok(ReconcileResult {
                                status: DeploymentStatus::Deploying,
                                deployment_url: Some(format!(
                                    "https://{}",
                                    self.hostname(project, deployment)
                                )),
                                controller_metadata: serde_json::to_value(&metadata)?,
                                error_message: None,
                            });
                        }
                        Err(e) if is_namespace_not_found_error(&e) => {
                            warn!(
                                "Namespace missing during ReplicaSet get in Completed phase, resetting to CreatingNamespace"
                            );
                            metadata.reconcile_phase = ReconcilePhase::CreatingNamespace;
                            metadata.namespace = None;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }

                    let deployment_url =
                        format!("https://{}", self.full_ingress_url(project, deployment));

                    return Ok(ReconcileResult {
                        status: DeploymentStatus::Healthy,
                        deployment_url: Some(deployment_url),
                        controller_metadata: serde_json::to_value(&metadata)?,
                        error_message: None,
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

        // 1. Check for pod-level errors FIRST
        // This prevents race conditions where ReplicaSet reports ready_replicas
        // but pods are actually in CrashLoopBackOff or other error states
        let (has_errors, error_msg) = match self.check_pod_errors(&namespace, &rs_name).await {
            Ok((errors, msg)) => (errors, msg),
            Err(e) if is_namespace_not_found_error(&e) => {
                // Namespace missing - return unhealthy status
                warn!("Namespace missing during health check, marking deployment as unhealthy");
                return Ok(HealthStatus {
                    healthy: false,
                    message: Some("Namespace missing - recovery in progress".to_string()),
                    last_check: Utc::now(),
                });
            }
            Err(e) => return Err(e),
        };
        if has_errors {
            return Ok(HealthStatus {
                healthy: false,
                message: error_msg,
                last_check: Utc::now(),
            });
        }

        // 2. Then check ReplicaSet readiness
        let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), &namespace);

        // Get ReplicaSet, handling 404 errors gracefully
        match rs_api.get(&rs_name).await {
            Ok(rs) => {
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
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                // ReplicaSet doesn't exist - mark as unhealthy to trigger recreation
                warn!(
                    "ReplicaSet {} not found in namespace {}, marking deployment as unhealthy",
                    rs_name, namespace
                );
                Ok(HealthStatus {
                    healthy: false,
                    message: Some(format!("ReplicaSet {} not found", rs_name)),
                    last_check: Utc::now(),
                })
            }
            Err(e) if is_namespace_not_found_error(&e) => {
                // Namespace missing - mark as unhealthy
                warn!("Namespace missing during ReplicaSet health check");
                Ok(HealthStatus {
                    healthy: false,
                    message: Some("Namespace missing - recovery in progress".to_string()),
                    last_check: Utc::now(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn cancel(&self, deployment: &Deployment) -> Result<()> {
        info!(
            "Cancelling deployment {} (pre-infrastructure)",
            deployment.deployment_id
        );

        // Parse metadata, but don't fail if it's missing or incomplete (deployment may not have started)
        let metadata: Option<KubernetesMetadata> =
            serde_json::from_value(deployment.controller_metadata.clone()).ok();

        if let Some(metadata) = metadata {
            // Clean up any partially created ReplicaSet
            if let (Some(rs_name), Some(namespace)) = (metadata.replicaset_name, metadata.namespace)
            {
                let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), &namespace);
                if let Err(e) = rs_api.delete(&rs_name, &DeleteParams::default()).await {
                    // Ignore 404 errors (already deleted)
                    if !e.to_string().contains("404") {
                        warn!("Error deleting ReplicaSet during cancellation: {}", e);
                    }
                }
            }
        } else {
            debug!(
                "No metadata found for deployment {}, nothing to clean up",
                deployment.deployment_id
            );
        }

        // Service and namespace are shared, don't delete
        Ok(())
    }

    async fn terminate(&self, deployment: &Deployment) -> Result<()> {
        info!(
            "Terminating deployment {} - deleting ReplicaSet",
            deployment.deployment_id
        );

        // Parse metadata, but don't fail if it's missing or incomplete
        let metadata: Option<KubernetesMetadata> =
            serde_json::from_value(deployment.controller_metadata.clone()).ok();

        if let Some(metadata) = metadata {
            // Delete ONLY the ReplicaSet (cascading deletes pods)
            if let (Some(rs_name), Some(namespace)) =
                (metadata.replicaset_name.clone(), metadata.namespace.clone())
            {
                let rs_api: Api<ReplicaSet> = Api::namespaced(self.kube_client.clone(), &namespace);
                if let Err(e) = rs_api.delete(&rs_name, &DeleteParams::default()).await {
                    // Ignore 404 errors (already deleted)
                    if !e.to_string().contains("404") {
                        warn!("Error deleting ReplicaSet during termination: {}", e);
                    }
                }
            }

            // For non-default deployment groups, check if we should clean up group-specific resources
            if deployment.deployment_group
                != crate::server::deployment::models::DEFAULT_DEPLOYMENT_GROUP
            {
                // Check if there are any other active deployments in this group
                if let Err(e) = self
                    .cleanup_group_resources_if_empty(deployment, &metadata)
                    .await
                {
                    warn!(
                        "Error cleaning up group resources for deployment {}: {}",
                        deployment.deployment_id, e
                    );
                }
            }
        } else {
            debug!(
                "No metadata found for deployment {}, nothing to terminate",
                deployment.deployment_id
            );
        }

        // DO NOT delete Secret or Namespace (shared across all groups in the project)
        Ok(())
    }
}

impl KubernetesController {
    /// Compare actual vs desired ReplicaSet state to detect drift
    fn replicaset_has_drifted(&self, actual: &ReplicaSet, desired: &ReplicaSet) -> bool {
        // Compare critical fields that should never drift

        // 1. Replica count
        let actual_replicas = actual.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
        let desired_replicas = desired.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
        if actual_replicas != desired_replicas {
            warn!(
                "ReplicaSet drift: replicas {} != {}",
                actual_replicas, desired_replicas
            );
            return true;
        }

        // 2. Container image
        let actual_image = actual
            .spec
            .as_ref()
            .and_then(|s| s.template.as_ref())
            .and_then(|t| t.spec.as_ref())
            .and_then(|ps| ps.containers.first())
            .and_then(|c| c.image.as_ref());

        let desired_image = desired
            .spec
            .as_ref()
            .and_then(|s| s.template.as_ref())
            .and_then(|t| t.spec.as_ref())
            .and_then(|ps| ps.containers.first())
            .and_then(|c| c.image.as_ref());

        if actual_image != desired_image {
            warn!(
                "ReplicaSet drift: image {:?} != {:?}",
                actual_image, desired_image
            );
            return true;
        }

        // 3. Labels (deployment-id must match)
        let actual_labels = actual
            .spec
            .as_ref()
            .and_then(|s| s.selector.match_labels.as_ref());
        let desired_labels = desired
            .spec
            .as_ref()
            .and_then(|s| s.selector.match_labels.as_ref());

        if actual_labels != desired_labels {
            warn!("ReplicaSet drift: labels don't match");
            return true;
        }

        false
    }

    /// Delete ReplicaSet and wait for deletion to complete
    async fn delete_and_wait_replicaset(&self, rs_api: &Api<ReplicaSet>, name: &str) -> Result<()> {
        // Delete the ReplicaSet
        let delete_params = DeleteParams::default();
        match rs_api.delete(name, &delete_params).await {
            Ok(_) => {}
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                // Already deleted
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }

        // Wait for deletion to complete (max 30 seconds)
        for i in 0..30 {
            tokio::time::sleep(Duration::from_secs(1)).await;

            match rs_api.get(name).await {
                Err(kube::Error::Api(ae)) if ae.code == 404 => {
                    debug!("ReplicaSet {} deleted successfully", name);
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
                Ok(_) => {
                    debug!(
                        "Waiting for ReplicaSet {} deletion (attempt {})",
                        name,
                        i + 1
                    );
                }
            }
        }

        Err(anyhow::anyhow!(
            "Timeout waiting for ReplicaSet {} deletion",
            name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{Container, PodSpec, PodTemplateSpec};
    use std::collections::BTreeMap;

    fn create_test_replicaset(
        name: &str,
        replicas: i32,
        image: &str,
        labels: BTreeMap<String, String>,
    ) -> ReplicaSet {
        ReplicaSet {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: Some(ReplicaSetSpec {
                replicas: Some(replicas),
                selector: LabelSelector {
                    match_labels: Some(labels.clone()),
                    ..Default::default()
                },
                template: Some(PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some(labels),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        containers: vec![Container {
                            name: "app".to_string(),
                            image: Some(image.to_string()),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_replicaset_has_drifted_no_drift() {
        // Create a mock controller (we only need the method, not actual K8s connection)
        let controller = create_mock_controller();

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "test".to_string());

        let rs1 = create_test_replicaset("test-rs", 1, "nginx:1.0", labels.clone());
        let rs2 = create_test_replicaset("test-rs", 1, "nginx:1.0", labels);

        assert!(!controller.replicaset_has_drifted(&rs1, &rs2));
    }

    #[tokio::test]
    async fn test_replicaset_has_drifted_replica_count() {
        let controller = create_mock_controller();

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "test".to_string());

        let rs1 = create_test_replicaset("test-rs", 1, "nginx:1.0", labels.clone());
        let rs2 = create_test_replicaset("test-rs", 3, "nginx:1.0", labels);

        assert!(controller.replicaset_has_drifted(&rs1, &rs2));
    }

    #[tokio::test]
    async fn test_replicaset_has_drifted_image() {
        let controller = create_mock_controller();

        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "test".to_string());

        let rs1 = create_test_replicaset("test-rs", 1, "nginx:1.0", labels.clone());
        let rs2 = create_test_replicaset("test-rs", 1, "nginx:2.0", labels);

        assert!(controller.replicaset_has_drifted(&rs1, &rs2));
    }

    #[tokio::test]
    async fn test_replicaset_has_drifted_labels() {
        let controller = create_mock_controller();

        let mut labels1 = BTreeMap::new();
        labels1.insert("app".to_string(), "test".to_string());

        let mut labels2 = BTreeMap::new();
        labels2.insert("app".to_string(), "other".to_string());

        let rs1 = create_test_replicaset("test-rs", 1, "nginx:1.0", labels1);
        let rs2 = create_test_replicaset("test-rs", 1, "nginx:1.0", labels2);

        assert!(controller.replicaset_has_drifted(&rs1, &rs2));
    }

    // Helper function to create a mock controller for testing
    fn create_mock_controller() -> KubernetesController {
        use crate::server::state::ControllerState;
        use axum::http::Uri;
        use sqlx::postgres::PgPoolOptions;

        // Install default CryptoProvider for rustls (required for kube-rs HTTPS connections)
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();

        // Create a minimal controller with mock values
        // Note: We won't actually connect to K8s or DB for these unit tests
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/test")
            .expect("Failed to create pool");

        let state = ControllerState {
            db_pool: pool,
            encryption_provider: None,
        };

        // Create a fake kube client (won't be used in these tests)
        let cluster_url = "http://localhost:8080"
            .parse::<Uri>()
            .expect("Failed to parse URI");
        let kube_config = kube::Config::new(cluster_url);
        let kube_client = kube::Client::try_from(kube_config).expect("Failed to create client");

        KubernetesController {
            state,
            kube_client,
            ingress_class: "nginx".to_string(),
            production_ingress_url_template: "{project_name}.test.local".to_string(),
            staging_ingress_url_template: None,
            registry_provider: None,
            auth_backend_url: "http://localhost:3000".to_string(),
            auth_signin_url: "http://localhost:3000".to_string(),
            namespace_annotations: std::collections::HashMap::new(),
            ingress_annotations: std::collections::HashMap::new(),
            ingress_tls_secret_name: None,
            node_selector: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_parse_ingress_url_subdomain() {
        let url = "myapp.apps.rise.dev";
        let parsed = KubernetesController::parse_ingress_url(url);
        assert_eq!(parsed.host, "myapp.apps.rise.dev");
        assert!(parsed.path_prefix.is_none());
    }

    #[test]
    fn test_parse_ingress_url_single_path() {
        let url = "rise.dev/myapp";
        let parsed = KubernetesController::parse_ingress_url(url);
        assert_eq!(parsed.host, "rise.dev");
        assert_eq!(parsed.path_prefix, Some("/myapp".to_string()));
    }

    #[test]
    fn test_parse_ingress_url_multi_level_path() {
        let url = "rise.dev/myapp/staging";
        let parsed = KubernetesController::parse_ingress_url(url);
        assert_eq!(parsed.host, "rise.dev");
        assert_eq!(parsed.path_prefix, Some("/myapp/staging".to_string()));
    }
}
