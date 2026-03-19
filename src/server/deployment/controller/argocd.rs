use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{AsyncBufReadExt, StreamExt};
use k8s_openapi::api::core::v1::{Namespace, Pod, Secret};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, ListParams, LogParams, Patch, PatchParams};
use kube::{Client, CustomResource, ResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn};

use super::{DeploymentBackend, DeploymentUrls, HealthStatus, ReconcileResult};
use crate::db::custom_domains as db_custom_domains;
use crate::db::deployments as db_deployments;
use crate::db::models::{Deployment, DeploymentStatus, Project};
use crate::db::projects as db_projects;
use crate::server::deployment::models::{normalize_deployment_group, DEFAULT_DEPLOYMENT_GROUP};
use crate::server::registry::{
    models::{RegistryAuthMethod, RegistryCredentials},
    ImageTagType, RegistryProvider,
};
use crate::server::settings::AccessClass;
use crate::server::state::ControllerState;

const LABEL_MANAGED_BY: &str = "rise.dev/managed-by";
const LABEL_PROJECT: &str = "rise.dev/project";
const LABEL_DEPLOYMENT_GROUP: &str = "rise.dev/deployment-group";
const LABEL_DEPLOYMENT_ID: &str = "rise.dev/deployment-id";
pub const LABEL_IGNORE_RECONCILE: &str = "rise.dev/ignore-reconcile";
const ANNOTATION_SPEC_HASH: &str = "rise.dev/spec-hash";
const ANNOTATION_LAST_APPLIED_AT: &str = "rise.dev/last-applied-at";
const ANNOTATION_TARGET_IMAGE: &str = "rise.dev/target-image";
const APPLICATION_RESOURCES_FINALIZER: &str = "resources-finalizer.argocd.argoproj.io";
pub const ARGOCD_PROJECT_FINALIZER: &str = "argocd.rise.dev/resources";
const MAX_STATUS_MESSAGE_CHARS: usize = 240;

#[derive(Clone)]
pub struct ArgoCdHelmChartConfig {
    pub repo_url: String,
    pub chart: String,
    pub target_revision: String,
    pub values: Value,
}

#[derive(Clone)]
pub struct ArgoCdControllerConfig {
    pub argocd_namespace: String,
    pub production_ingress_url_template: String,
    pub staging_ingress_url_template: Option<String>,
    pub ingress_port: Option<u16>,
    pub ingress_schema: String,
    pub appproject_format: String,
    pub application_format: String,
    pub destination_namespace_format: String,
    pub destination_server: String,
    pub namespace_labels: HashMap<String, String>,
    pub namespace_annotations: HashMap<String, String>,
    pub helm_chart: ArgoCdHelmChartConfig,
    pub sync_options: Vec<String>,
    pub access_classes: HashMap<String, AccessClass>,
    pub registry_provider: Arc<dyn RegistryProvider>,
}

pub struct ArgoCdController {
    state: ControllerState,
    kube_client: Client,
    argocd_namespace: String,
    production_ingress_url_template: String,
    staging_ingress_url_template: Option<String>,
    ingress_port: Option<u16>,
    ingress_schema: String,
    appproject_format: String,
    application_format: String,
    destination_namespace_format: String,
    destination_server: String,
    namespace_labels: HashMap<String, String>,
    namespace_annotations: HashMap<String, String>,
    helm_chart: ArgoCdHelmChartConfig,
    sync_options: Vec<String>,
    access_classes: HashMap<String, AccessClass>,
    registry_provider: Arc<dyn RegistryProvider>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct ArgoCdMetadata {
    argocd_namespace: Option<String>,
    appproject_name: Option<String>,
    application_name: Option<String>,
    destination_namespace: Option<String>,
    env_secret_name: Option<String>,
    pull_secret_name: Option<String>,
    target_image: Option<String>,
    applied_spec_hash: Option<String>,
    last_applied_at: Option<DateTime<Utc>>,
    reverted_to_deployment_id: Option<String>,
}

#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "argoproj.io",
    version = "v1alpha1",
    kind = "Application",
    plural = "applications",
    namespaced,
    status = "ApplicationStatus"
)]
#[serde(rename_all = "camelCase")]
struct ApplicationSpec {
    project: String,
    source: ApplicationSource,
    destination: ApplicationDestination,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_policy: Option<ApplicationSyncPolicy>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationSource {
    #[serde(rename = "repoURL")]
    repo_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    chart: Option<String>,
    target_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    helm: Option<ApplicationSourceHelm>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationSourceHelm {
    #[serde(skip_serializing_if = "Option::is_none")]
    release_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationDestination {
    server: String,
    namespace: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationSyncPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    automated: Option<ApplicationSyncPolicyAutomated>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_options: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationSyncPolicyAutomated {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default)]
    prune: bool,
    #[serde(default)]
    self_heal: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<ApplicationHealthStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<ApplicationSyncStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_state: Option<ApplicationOperationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conditions: Option<Vec<ApplicationCondition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<ApplicationSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resources: Option<Vec<ApplicationResourceStatus>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    history: Option<Vec<ApplicationRevisionHistory>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transitions: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reconciled_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationHealthStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_transition_time: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationSyncStatus {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationOperationState {
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationCondition {
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_transition_time: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    external_urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationResourceStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    kind: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<ApplicationHealthStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_pruning: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ApplicationRevisionHistory {
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revisions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deploy_started_at: Option<String>,
}

#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "argoproj.io",
    version = "v1alpha1",
    kind = "AppProject",
    plural = "appprojects",
    namespaced
)]
#[serde(rename_all = "camelCase")]
struct AppProjectSpec {
    description: String,
    source_repos: Vec<String>,
    destinations: Vec<AppProjectDestination>,
    cluster_resource_whitelist: Vec<AppProjectGroupKind>,
    namespace_resource_whitelist: Vec<AppProjectGroupKind>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AppProjectDestination {
    server: String,
    namespace: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AppProjectGroupKind {
    group: String,
    kind: String,
}

struct DesiredApplication {
    object: Application,
    spec_hash: String,
}

struct ApplicationReadiness {
    healthy: bool,
    failed: bool,
    message: Option<String>,
    reconciled_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct DeploymentPodStatus {
    desired_replicas: i32,
    ready_replicas: i32,
    current_replicas: i32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pods: Vec<DeploymentPodInfo>,
    last_checked: DateTime<Utc>,
}

#[derive(Serialize)]
struct DeploymentPodInfo {
    name: String,
    phase: String,
    ready: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    conditions: Vec<DeploymentPodCondition>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    containers: Vec<DeploymentContainerStatus>,
}

#[derive(Serialize)]
struct DeploymentPodCondition {
    #[serde(rename = "type")]
    type_: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
struct DeploymentContainerStatus {
    name: String,
    ready: bool,
    restart_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<DeploymentContainerState>,
}

#[derive(Serialize)]
struct DeploymentContainerState {
    state_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

struct MatchedPodHealth {
    healthy: bool,
    message: Option<String>,
    status: Value,
}

fn truncate_text(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let mut truncated: String = input.chars().take(max_chars).collect();
    truncated.push_str("...");
    truncated
}

fn truncate_optional_text(input: Option<&str>, max_chars: usize) -> Option<String> {
    input.map(|text| truncate_text(text, max_chars))
}

fn application_configured_deployment_id(application: &Application) -> Option<String> {
    application
        .metadata
        .labels
        .as_ref()
        .and_then(|labels| labels.get(LABEL_DEPLOYMENT_ID))
        .cloned()
}

fn reconcile_ignored(metadata: &ObjectMeta) -> bool {
    metadata
        .labels
        .as_ref()
        .and_then(|labels| labels.get(LABEL_IGNORE_RECONCILE))
        .map(|value| matches!(value.as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(false)
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(key, value)| (key, canonicalize_json(value)))
                .collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));

            let mut normalized = Map::new();
            for (key, value) in entries {
                normalized.insert(key, value);
            }
            Value::Object(normalized)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

impl ArgoCdController {
    pub fn new(
        state: ControllerState,
        kube_client: Client,
        config: ArgoCdControllerConfig,
    ) -> Result<Self> {
        Ok(Self {
            state,
            kube_client,
            argocd_namespace: config.argocd_namespace,
            production_ingress_url_template: config.production_ingress_url_template,
            staging_ingress_url_template: config.staging_ingress_url_template,
            ingress_port: config.ingress_port,
            ingress_schema: config.ingress_schema,
            appproject_format: config.appproject_format,
            application_format: config.application_format,
            destination_namespace_format: config.destination_namespace_format,
            destination_server: config.destination_server,
            namespace_labels: config.namespace_labels,
            namespace_annotations: config.namespace_annotations,
            helm_chart: config.helm_chart,
            sync_options: config.sync_options,
            access_classes: config.access_classes,
            registry_provider: config.registry_provider,
        })
    }

    pub async fn test_connection(&self) -> Result<()> {
        let namespaces: Api<Namespace> = Api::all(self.kube_client.clone());
        namespaces
            .get("default")
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("Failed to connect to Kubernetes API: {}", e))
    }

    fn argocd_api_error(
        &self,
        resource_kind: &str,
        operation: &str,
        namespace: Option<&str>,
        error: kube::Error,
    ) -> anyhow::Error {
        let namespace_suffix = namespace
            .map(|value| format!(" in namespace '{}'", value))
            .unwrap_or_default();

        match &error {
            kube::Error::Api(api_error) if api_error.code == 404 => {
                if resource_kind == "Namespace" {
                    return anyhow::anyhow!(
                        "Failed to {} {}{}: namespace '{}' does not exist. \
                         ArgoCD is likely not installed yet or is installed in a different namespace. \
                         Configure deployment_controller.argocd_namespace to the correct namespace.",
                        operation,
                        resource_kind,
                        namespace_suffix,
                        namespace.unwrap_or(&self.argocd_namespace),
                    );
                }

                return anyhow::anyhow!(
                    "Failed to {} ArgoCD {}{}: Kubernetes returned 404 for argoproj.io/v1alpha1. \
                     This usually means ArgoCD is not installed in the cluster or its CRDs are missing. \
                     Expected ArgoCD Application/AppProject CRDs and namespace '{}'. Original error: {}",
                    operation,
                    resource_kind,
                    namespace_suffix,
                    self.argocd_namespace,
                    api_error.message,
                );
            }
            _ => {}
        }

        anyhow::anyhow!(
            "Failed to {} {}{}: {}",
            operation,
            resource_kind,
            namespace_suffix,
            error
        )
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            self.project_cleanup_loop().await;
        });
    }

    async fn project_cleanup_loop(&self) {
        let mut ticker = interval(Duration::from_secs(10));

        loop {
            ticker.tick().await;
            if let Err(err) = self.process_deleting_projects().await {
                warn!("ArgoCD project cleanup loop error: {}", err);
            }
        }
    }

    async fn process_deleting_projects(&self) -> Result<()> {
        let projects = db_projects::find_deleting_with_finalizer(
            &self.state.db_pool,
            ARGOCD_PROJECT_FINALIZER,
            20,
        )
        .await?;

        for project in projects {
            let groups = db_deployments::get_all_deployment_groups(&self.state.db_pool, project.id)
                .await
                .unwrap_or_default();

            let mut all_deleted = true;

            for group in groups {
                let application_name = self.application_name(&project.name, &group);
                if self.delete_application_if_exists(&application_name).await? {
                    all_deleted = false;
                }

                let namespace = self.destination_namespace(&project.name, &group);
                if self.delete_namespace_if_exists(&namespace).await? {
                    all_deleted = false;
                }
            }

            let appproject_name = self.appproject_name(&project.name);
            if self.delete_appproject_if_exists(&appproject_name).await? {
                all_deleted = false;
            }

            if all_deleted {
                db_projects::remove_finalizer(
                    &self.state.db_pool,
                    project.id,
                    ARGOCD_PROJECT_FINALIZER,
                )
                .await?;
                info!(
                    "Removed ArgoCD finalizer from project {}, cleanup complete",
                    project.name
                );
            }
        }

        Ok(())
    }

    async fn delete_application_if_exists(&self, name: &str) -> Result<bool> {
        match self.get_application_opt(name).await? {
            Some(_) => {
                self.delete_application(name).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn delete_appproject_if_exists(&self, name: &str) -> Result<bool> {
        match self.get_appproject_opt(name).await? {
            Some(_) => {
                self.delete_appproject(name).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn delete_namespace_if_exists(&self, name: &str) -> Result<bool> {
        let api = self.namespace_api();
        match api.get_opt(name).await? {
            Some(existing) => {
                if reconcile_ignored(&existing.metadata) {
                    return Ok(false);
                }
                api.delete(name, &DeleteParams::default()).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn escaped_group_name(&self, deployment_group: &str) -> String {
        if deployment_group == DEFAULT_DEPLOYMENT_GROUP {
            "default".to_string()
        } else {
            normalize_deployment_group(deployment_group)
        }
    }

    fn format_name(&self, template: &str, project_name: &str, deployment_group: &str) -> String {
        template
            .replace("{project_name}", project_name)
            .replace("{deployment_group}", &self.escaped_group_name(deployment_group))
    }

    fn appproject_name(&self, project_name: &str) -> String {
        self.format_name(&self.appproject_format, project_name, DEFAULT_DEPLOYMENT_GROUP)
    }

    fn application_name(&self, project_name: &str, deployment_group: &str) -> String {
        self.format_name(&self.application_format, project_name, deployment_group)
    }

    fn destination_namespace(&self, project_name: &str, deployment_group: &str) -> String {
        self.format_name(
            &self.destination_namespace_format,
            project_name,
            deployment_group,
        )
    }

    fn env_secret_name(&self, deployment: &Deployment) -> String {
        format!("rise-env-{}", deployment.deployment_id)
    }

    fn pull_secret_name(&self, deployment: &Deployment) -> String {
        format!("rise-pull-{}", deployment.deployment_id)
    }

    fn namespace_api(&self) -> Api<Namespace> {
        Api::all(self.kube_client.clone())
    }

    fn applications_api(&self) -> Api<Application> {
        Api::namespaced(self.kube_client.clone(), &self.argocd_namespace)
    }

    fn appprojects_api(&self) -> Api<AppProject> {
        Api::namespaced(self.kube_client.clone(), &self.argocd_namespace)
    }

    fn secrets_api(&self, namespace: &str) -> Api<Secret> {
        Api::namespaced(self.kube_client.clone(), namespace)
    }

    fn deployment_label_selector(&self, project_name: &str, deployment: &Deployment) -> String {
        format!(
            "{project_label}={project},{group_label}={group},{deployment_label}={deployment_id}",
            project_label = LABEL_PROJECT,
            project = project_name,
            group_label = LABEL_DEPLOYMENT_GROUP,
            group = self.escaped_group_name(&deployment.deployment_group),
            deployment_label = LABEL_DEPLOYMENT_ID,
            deployment_id = deployment.deployment_id,
        )
    }

    async fn get_secret_opt(&self, namespace: &str, name: &str) -> Result<Option<Secret>> {
        self.secrets_api(namespace)
            .get_opt(name)
            .await
            .map_err(|e| self.argocd_api_error("Secret", "get", Some(namespace), e))
    }

    async fn collect_matched_pod_health(
        &self,
        namespace: &str,
        project_name: &str,
        deployment: &Deployment,
    ) -> Result<MatchedPodHealth> {
        let pod_api: Api<Pod> = Api::namespaced(self.kube_client.clone(), namespace);
        let selector = self.deployment_label_selector(project_name, deployment);
        let pods = pod_api
            .list(&ListParams::default().labels(&selector))
            .await?;

        let current_replicas = pods.items.len() as i32;
        let mut ready_replicas = 0;
        let mut pod_infos = Vec::new();
        let mut errors = Vec::new();

        for pod in pods.items {
            let pod_name = pod.metadata.name.unwrap_or_default();
            let phase = pod
                .status
                .as_ref()
                .and_then(|status| status.phase.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let conditions: Vec<DeploymentPodCondition> = pod
                .status
                .as_ref()
                .and_then(|status| status.conditions.as_ref())
                .map(|conditions| {
                    conditions
                        .iter()
                        .map(|condition| DeploymentPodCondition {
                            type_: condition.type_.clone(),
                            status: condition.status.clone(),
                            reason: condition.reason.clone(),
                            message: truncate_optional_text(
                                condition.message.as_deref(),
                                MAX_STATUS_MESSAGE_CHARS,
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default();

            let pod_ready = conditions
                .iter()
                .any(|condition| condition.type_ == "Ready" && condition.status == "True");

            let containers: Vec<DeploymentContainerStatus> = pod
                .status
                .as_ref()
                .and_then(|status| status.container_statuses.as_ref())
                .map(|statuses| {
                    statuses
                        .iter()
                        .map(|container| {
                            let state = if let Some(waiting) =
                                container.state.as_ref().and_then(|state| state.waiting.as_ref())
                            {
                                Some(DeploymentContainerState {
                                    state_type: "waiting".to_string(),
                                    reason: waiting.reason.clone(),
                                    message: truncate_optional_text(
                                        waiting.message.as_deref(),
                                        MAX_STATUS_MESSAGE_CHARS,
                                    ),
                                    exit_code: None,
                                })
                            } else if container
                                .state
                                .as_ref()
                                .and_then(|state| state.running.as_ref())
                                .is_some()
                            {
                                Some(DeploymentContainerState {
                                    state_type: "running".to_string(),
                                    reason: None,
                                    message: None,
                                    exit_code: None,
                                })
                            } else {
                                container
                                    .state
                                    .as_ref()
                                    .and_then(|state| state.terminated.as_ref())
                                    .map(|terminated| DeploymentContainerState {
                                        state_type: "terminated".to_string(),
                                        reason: terminated.reason.clone(),
                                        message: truncate_optional_text(
                                            terminated.message.as_deref(),
                                            MAX_STATUS_MESSAGE_CHARS,
                                        ),
                                        exit_code: Some(terminated.exit_code),
                                    })
                            };

                            DeploymentContainerStatus {
                                name: container.name.clone(),
                                ready: container.ready,
                                restart_count: container.restart_count,
                                state,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let all_containers_ready = !containers.is_empty() && containers.iter().all(|c| c.ready);
            let ready = pod_ready || all_containers_ready;
            if ready {
                ready_replicas += 1;
            }

            if phase == "Failed" {
                errors.push(format!("pod {pod_name} failed"));
            }

            for container in &containers {
                if let Some(state) = &container.state {
                    if state.state_type == "waiting" {
                        let reason = state.reason.as_deref().unwrap_or("waiting");
                        if matches!(
                            reason,
                            "CrashLoopBackOff"
                                | "ImagePullBackOff"
                                | "ErrImagePull"
                                | "CreateContainerConfigError"
                                | "CreateContainerError"
                                | "RunContainerError"
                        ) {
                            errors.push(format!(
                                "pod {pod_name} container {} waiting: {reason}",
                                container.name
                            ));
                        }
                    }
                    if state.state_type == "terminated" && state.exit_code.unwrap_or(0) != 0 {
                        errors.push(format!(
                            "pod {pod_name} container {} exited with {}",
                            container.name,
                            state.exit_code.unwrap_or_default()
                        ));
                    }
                }
            }

            pod_infos.push(DeploymentPodInfo {
                name: pod_name,
                phase,
                ready,
                conditions,
                containers,
            });
        }

        let status = serde_json::to_value(DeploymentPodStatus {
            desired_replicas: current_replicas.max(1),
            ready_replicas,
            current_replicas,
            pods: pod_infos,
            last_checked: Utc::now(),
        })?;

        let message = if !errors.is_empty() {
            Some(errors.join(", "))
        } else if current_replicas == 0 {
            Some(format!(
                "No pods found for deployment {} in namespace {}",
                deployment.deployment_id, namespace
            ))
        } else if ready_replicas < current_replicas {
            Some(format!("Pods ready: {ready_replicas}/{current_replicas}"))
        } else {
            None
        };

        Ok(MatchedPodHealth {
            healthy: errors.is_empty() && current_replicas > 0 && ready_replicas == current_replicas,
            message,
            status,
        })
    }

    async fn patch_secret(&self, namespace: &str, name: &str, secret: &Secret) -> Result<Secret> {
        if let Some(existing) = self.get_secret_opt(namespace, name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(existing);
            }
        }

        self.secrets_api(namespace)
            .patch(
                name,
                &PatchParams::apply("rise-argocd").force(),
                &Patch::Apply(secret),
            )
            .await
            .map_err(|e| self.argocd_api_error("Secret", "patch", Some(namespace), e))
    }

    async fn get_application_opt(&self, name: &str) -> Result<Option<Application>> {
        self.applications_api()
            .get_opt(name)
            .await
            .map_err(|e| self.argocd_api_error("Application", "get", Some(&self.argocd_namespace), e))
    }

    async fn get_application(&self, name: &str) -> Result<Application> {
        self.applications_api()
            .get(name)
            .await
            .map_err(|e| self.argocd_api_error("Application", "get", Some(&self.argocd_namespace), e))
    }

    async fn patch_application(&self, name: &str, application: &Application) -> Result<Application> {
        if let Some(existing) = self.get_application_opt(name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(existing);
            }
        }

        self.applications_api()
            .patch(
                name,
                &PatchParams::apply("rise-argocd").force(),
                &Patch::Apply(application),
            )
            .await
            .map_err(|e| self.argocd_api_error("Application", "patch", Some(&self.argocd_namespace), e))
    }

    async fn delete_application(&self, name: &str) -> Result<()> {
        if let Some(existing) = self.get_application_opt(name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(());
            }
        }

        self.applications_api()
            .delete(name, &DeleteParams::default())
            .await
            .map(|_| ())
            .map_err(|e| self.argocd_api_error("Application", "delete", Some(&self.argocd_namespace), e))
    }

    async fn get_appproject_opt(&self, name: &str) -> Result<Option<AppProject>> {
        self.appprojects_api()
            .get_opt(name)
            .await
            .map_err(|e| self.argocd_api_error("AppProject", "get", Some(&self.argocd_namespace), e))
    }

    async fn patch_appproject(&self, name: &str, appproject: &AppProject) -> Result<AppProject> {
        if let Some(existing) = self.get_appproject_opt(name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(existing);
            }
        }

        self.appprojects_api()
            .patch(
                name,
                &PatchParams::apply("rise-argocd").force(),
                &Patch::Apply(appproject),
            )
            .await
            .map_err(|e| self.argocd_api_error("AppProject", "patch", Some(&self.argocd_namespace), e))
    }

    async fn delete_appproject(&self, name: &str) -> Result<()> {
        if let Some(existing) = self.get_appproject_opt(name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(());
            }
        }

        self.appprojects_api()
            .delete(name, &DeleteParams::default())
            .await
            .map(|_| ())
            .map_err(|e| self.argocd_api_error("AppProject", "delete", Some(&self.argocd_namespace), e))
    }

    async fn ensure_destination_namespace(
        &self,
        project: &Project,
        deployment_group: &str,
    ) -> Result<String> {
        let namespace_name = self.destination_namespace(&project.name, deployment_group);
        if let Some(existing) = self.namespace_api().get_opt(&namespace_name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(namespace_name);
            }
        }

        let mut labels = BTreeMap::new();
        labels.insert(LABEL_MANAGED_BY.to_string(), "rise-argocd".to_string());
        labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
        labels.insert(
            LABEL_DEPLOYMENT_GROUP.to_string(),
            self.escaped_group_name(deployment_group),
        );
        for (key, value) in &self.namespace_labels {
            labels.insert(key.clone(), value.clone());
        }

        let mut annotations = BTreeMap::new();
        for (key, value) in &self.namespace_annotations {
            annotations.insert(key.clone(), value.clone());
        }

        let namespace = Namespace {
            metadata: ObjectMeta {
                name: Some(namespace_name.clone()),
                labels: Some(labels),
                annotations: Some(annotations),
                ..Default::default()
            },
            ..Default::default()
        };

        self.namespace_api()
            .patch(
                &namespace_name,
                &PatchParams::apply("rise-argocd").force(),
                &Patch::Apply(&namespace),
            )
            .await?;

        Ok(namespace_name)
    }

    async fn ensure_appproject(&self, project: &Project) -> Result<String> {
        let appproject_name = self.appproject_name(&project.name);
        let (source_repo_url, _, _) = self.application_source_reference();
        let appproject = AppProject::new(
            &appproject_name,
            AppProjectSpec {
                description: format!("Rise-managed ArgoCD project for {}", project.name),
                source_repos: vec![source_repo_url],
                destinations: vec![AppProjectDestination {
                    server: self.destination_server.clone(),
                    namespace: format!("rise-{}-*", project.name),
                }],
                cluster_resource_whitelist: vec![AppProjectGroupKind {
                    group: "*".to_string(),
                    kind: "*".to_string(),
                }],
                namespace_resource_whitelist: vec![AppProjectGroupKind {
                    group: "*".to_string(),
                    kind: "*".to_string(),
                }],
            },
        );

        self.patch_appproject(&appproject_name, &appproject).await?;

        if !project
            .finalizers
            .contains(&ARGOCD_PROJECT_FINALIZER.to_string())
        {
            db_projects::add_finalizer(
                &self.state.db_pool,
                project.id,
                ARGOCD_PROJECT_FINALIZER,
            )
            .await?;
        }

        Ok(appproject_name)
    }

    async fn load_env_vars(&self, deployment: &Deployment) -> Result<Vec<(String, String)>> {
        crate::db::env_vars::load_deployment_env_vars_decrypted(
            &self.state.db_pool,
            deployment.id,
            self.state.encryption_provider.as_deref(),
        )
        .await
    }

    fn create_env_secret(
        &self,
        name: &str,
        project: &Project,
        deployment: &Deployment,
        env_vars: &[(String, String)],
    ) -> Secret {
        let mut data = BTreeMap::new();
        for (key, value) in env_vars {
            data.insert(key.clone(), k8s_openapi::ByteString(value.as_bytes().to_vec()));
        }

        let mut labels = BTreeMap::new();
        labels.insert(LABEL_MANAGED_BY.to_string(), "rise-argocd".to_string());
        labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
        labels.insert(
            LABEL_DEPLOYMENT_GROUP.to_string(),
            self.escaped_group_name(&deployment.deployment_group),
        );
        labels.insert(LABEL_DEPLOYMENT_ID.to_string(), deployment.deployment_id.clone());

        Secret {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                labels: Some(labels),
                ..Default::default()
            },
            type_: Some("Opaque".to_string()),
            data: Some(data),
            ..Default::default()
        }
    }

    fn create_dockerconfigjson_secret(
        &self,
        name: &str,
        project: &Project,
        deployment: &Deployment,
        registry_host: &str,
        credentials: &RegistryCredentials,
    ) -> Result<Secret> {
        use base64::Engine;

        let auths_entry = match credentials.auth_method {
            RegistryAuthMethod::LoginCredentials => {
                let auth = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", credentials.username, credentials.password));
                json!({
                    "username": credentials.username,
                    "password": credentials.password,
                    "auth": auth,
                })
            }
            RegistryAuthMethod::RegistryToken => json!({ "registrytoken": credentials.password }),
        };

        let docker_config = json!({ "auths": { registry_host: auths_entry } });
        let docker_config_bytes = docker_config.to_string().into_bytes();

        let mut data = BTreeMap::new();
        data.insert(
            ".dockerconfigjson".to_string(),
            k8s_openapi::ByteString(docker_config_bytes),
        );

        let mut labels = BTreeMap::new();
        labels.insert(LABEL_MANAGED_BY.to_string(), "rise-argocd".to_string());
        labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
        labels.insert(
            LABEL_DEPLOYMENT_GROUP.to_string(),
            self.escaped_group_name(&deployment.deployment_group),
        );
        labels.insert(LABEL_DEPLOYMENT_ID.to_string(), deployment.deployment_id.clone());

        Ok(Secret {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                labels: Some(labels),
                ..Default::default()
            },
            type_: Some("kubernetes.io/dockerconfigjson".to_string()),
            data: Some(data),
            ..Default::default()
        })
    }

    async fn ensure_deployment_secrets(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &mut ArgoCdMetadata,
    ) -> Result<()> {
        let destination_namespace =
            self.ensure_destination_namespace(project, &deployment.deployment_group)
                .await?;
        metadata.destination_namespace = Some(destination_namespace.clone());
        metadata.argocd_namespace = Some(self.argocd_namespace.clone());

        let env_secret_name = self.env_secret_name(deployment);
        let env_vars = self.load_env_vars(deployment).await?;
        let env_secret = self.create_env_secret(&env_secret_name, project, deployment, &env_vars);
        self.patch_secret(&destination_namespace, &env_secret_name, &env_secret)
            .await?;
        metadata.env_secret_name = Some(env_secret_name);

        if self.registry_provider.requires_pull_secret() {
            let pull_secret_name = self.pull_secret_name(deployment);
            let credentials = self
                .registry_provider
                .get_k8s_pull_credentials(&project.name)
                .await?;
            let pull_secret = self.create_dockerconfigjson_secret(
                &pull_secret_name,
                project,
                deployment,
                self.registry_provider.registry_host(),
                &credentials,
            )?;
            self.patch_secret(&destination_namespace, &pull_secret_name, &pull_secret)
                .await?;
            metadata.pull_secret_name = Some(pull_secret_name);
        } else {
            metadata.pull_secret_name = None;
        }

        Ok(())
    }

    async fn get_image_ref(&self, deployment: &Deployment, project: &Project) -> Result<String> {
        if let Some(digest) = &deployment.image_digest {
            return Ok(digest.clone());
        }

        let tag = if let Some(source_deployment_id) = deployment.rolled_back_from_deployment_id {
            match db_deployments::find_by_id(&self.state.db_pool, source_deployment_id).await? {
                Some(source) => source.deployment_id,
                None => deployment.deployment_id.clone(),
            }
        } else {
            deployment.deployment_id.clone()
        };

        Ok(self
            .registry_provider
            .get_image_tag(&project.name, &tag, ImageTagType::Internal))
    }

    fn merge_json(base: &mut Value, overlay: Value) {
        match (base, overlay) {
            (Value::Object(base_map), Value::Object(overlay_map)) => {
                for (key, value) in overlay_map {
                    Self::merge_json(base_map.entry(key).or_insert(Value::Null), value);
                }
            }
            (base_value, overlay_value) => *base_value = overlay_value,
        }
    }

    fn application_source_reference(&self) -> (String, Option<String>, Option<String>) {
        if let Some(repo_url) = self.helm_chart.repo_url.strip_prefix("oci://") {
            let repo_url = format!(
                "oci://{}/{}",
                repo_url.trim_end_matches('/'),
                self.helm_chart.chart.trim_start_matches('/')
            );
            return (repo_url, None, Some(".".to_string()));
        }

        (
            self.helm_chart.repo_url.clone(),
            Some(self.helm_chart.chart.clone()),
            None,
        )
    }

    async fn desired_application_for_deployment(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &mut ArgoCdMetadata,
    ) -> Result<DesiredApplication> {
        let (source_repo_url, source_chart, source_path) = self.application_source_reference();
        let appproject_name = self.ensure_appproject(project).await?;
        self.ensure_deployment_secrets(project, deployment, metadata).await?;

        let application_name = self.application_name(&project.name, &deployment.deployment_group);
        let destination_namespace = metadata
            .destination_namespace
            .clone()
            .unwrap_or_else(|| self.destination_namespace(&project.name, &deployment.deployment_group));
        let target_image = self.get_image_ref(deployment, project).await?;
        let resolved_ingress =
            self.resolved_ingress_url_for_group(project, &deployment.deployment_group);
        let (ingress_host, ingress_path) = Self::parse_ingress_target(&resolved_ingress);
        let custom_domain_hosts = if deployment.deployment_group == DEFAULT_DEPLOYMENT_GROUP {
            db_custom_domains::list_project_custom_domains(&self.state.db_pool, project.id)
                .await?
                .into_iter()
                .map(|domain| domain.domain)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let generated_values = json!({
            "rise": {
                "project": {
                    "id": project.id.to_string(),
                    "name": project.name,
                    "accessClass": project.access_class,
                },
                "deployment": {
                    "id": deployment.deployment_id,
                    "group": deployment.deployment_group,
                    "normalizedGroup": self.escaped_group_name(&deployment.deployment_group),
                    "namespace": destination_namespace,
                    "httpPort": deployment.http_port,
                },
                "ingress": {
                    "host": ingress_host,
                    "path": ingress_path,
                    "customDomainHosts": custom_domain_hosts,
                },
                "image": {
                    "ref": target_image,
                    "pullSecretName": metadata.pull_secret_name,
                },
                "env": {
                    "secretName": metadata.env_secret_name,
                },
                "accessClasses": self.access_classes,
                "argocd": {
                    "applicationName": application_name,
                    "appProjectName": appproject_name,
                }
            }
        });

        let mut merged_values = canonicalize_json(self.helm_chart.values.clone());
        Self::merge_json(&mut merged_values, generated_values);
        merged_values = canonicalize_json(merged_values);
        let values_string = serde_json::to_string_pretty(&merged_values)?;

        let application = Application::new(
            &application_name,
            ApplicationSpec {
                project: appproject_name.clone(),
                source: ApplicationSource {
                    repo_url: source_repo_url,
                    chart: source_chart,
                    target_revision: self.helm_chart.target_revision.clone(),
                    path: source_path,
                    helm: Some(ApplicationSourceHelm {
                        release_name: Some(application_name.clone()),
                        values: Some(values_string),
                    }),
                },
                destination: ApplicationDestination {
                    server: self.destination_server.clone(),
                    namespace: destination_namespace.clone(),
                },
                sync_policy: Some(ApplicationSyncPolicy {
                    automated: Some(ApplicationSyncPolicyAutomated {
                        enabled: Some(true),
                        prune: true,
                        self_heal: true,
                    }),
                    sync_options: Some(self.sync_options.clone()),
                }),
            },
        );

        let spec_hash = {
            let mut hasher = Sha256::new();
            hasher.update(serde_json::to_vec(&application.spec)?);
            format!("{:x}", hasher.finalize())
        };

        let mut labels = BTreeMap::new();
        labels.insert(LABEL_MANAGED_BY.to_string(), "rise-argocd".to_string());
        labels.insert(LABEL_PROJECT.to_string(), project.name.clone());
        labels.insert(
            LABEL_DEPLOYMENT_GROUP.to_string(),
            self.escaped_group_name(&deployment.deployment_group),
        );
        labels.insert(LABEL_DEPLOYMENT_ID.to_string(), deployment.deployment_id.clone());

        let mut annotations = BTreeMap::new();
        annotations.insert(ANNOTATION_SPEC_HASH.to_string(), spec_hash.clone());
        annotations.insert(ANNOTATION_TARGET_IMAGE.to_string(), target_image.clone());

        let mut object = application;
        object.metadata.namespace = Some(self.argocd_namespace.clone());
        object.metadata.labels = Some(labels);
        object.metadata.annotations = Some(annotations);
        object.metadata.finalizers = Some(vec![APPLICATION_RESOURCES_FINALIZER.to_string()]);

        metadata.appproject_name = Some(appproject_name);
        metadata.application_name = Some(application_name);
        metadata.target_image = Some(target_image);

        Ok(DesiredApplication {
            object,
            spec_hash,
        })
    }

    async fn ensure_application(
        &self,
        desired: &DesiredApplication,
        metadata: &mut ArgoCdMetadata,
    ) -> Result<Application> {
        let application_name = desired.object.name_any();
        let existing = self.get_application_opt(&application_name).await?;
        let current_hash = existing.as_ref().and_then(|app| {
            app.metadata
                .annotations
                .as_ref()
                .and_then(|map| map.get(ANNOTATION_SPEC_HASH))
                .cloned()
        });

        if current_hash.as_deref() != Some(desired.spec_hash.as_str()) {
            let mut object = desired.object.clone();
            let now = Utc::now();
            object
                .metadata
                .annotations
                .get_or_insert_with(BTreeMap::new)
                .insert(ANNOTATION_LAST_APPLIED_AT.to_string(), now.to_rfc3339());

            let applied = self.patch_application(&application_name, &object).await?;
            metadata.applied_spec_hash = Some(desired.spec_hash.clone());
            metadata.last_applied_at = Some(now);
            metadata.reverted_to_deployment_id = None;
            Ok(applied)
        } else {
            Ok(existing.unwrap())
        }
    }

    fn evaluate_application_status(&self, application: &Application) -> ApplicationReadiness {
        let mut message_parts = Vec::new();

        let health_status = application
            .status
            .as_ref()
            .and_then(|status| status.health.as_ref());
        let sync_status = application
            .status
            .as_ref()
            .and_then(|status| status.sync.as_ref());
        let operation_state = application
            .status
            .as_ref()
            .and_then(|status| status.operation_state.as_ref());
        let reconciled_at = application
            .status
            .as_ref()
            .and_then(|status| status.reconciled_at.as_deref())
            .and_then(parse_rfc3339);

        if let Some(health) = health_status {
            message_parts.push(format!("health={}", health.status));
            if let Some(msg) = &health.message {
                message_parts.push(truncate_text(msg, MAX_STATUS_MESSAGE_CHARS));
            }
            if let Some(last_transition_time) = &health.last_transition_time {
                message_parts.push(format!("lastTransition={}", last_transition_time));
            }
        }
        if let Some(sync) = sync_status {
            message_parts.push(format!("sync={}", sync.status));
            if let Some(revision) = &sync.revision {
                message_parts.push(format!("revision={}", revision));
            }
        }
        if let Some(operation) = operation_state {
            message_parts.push(format!("operation={}", operation.phase));
            if let Some(msg) = &operation.message {
                message_parts.push(truncate_text(msg, MAX_STATUS_MESSAGE_CHARS));
            }
        }
        if let Some(conditions) = application
            .status
            .as_ref()
            .and_then(|status| status.conditions.as_ref())
        {
            if let Some(condition) = conditions.iter().find(|condition| condition.message.is_some()) {
                message_parts.push(format!(
                    "condition={}{}",
                    condition.type_,
                    condition
                        .message
                        .as_deref()
                        .map(|message| format!(
                            ": {}",
                            truncate_text(message, MAX_STATUS_MESSAGE_CHARS)
                        ))
                        .unwrap_or_default()
                ));
            }
        }

        if let Some(status) = &application.status {
            if let Some(resources) = &status.resources {
                let unhealthy_resources: Vec<String> = resources
                    .iter()
                    .filter_map(|resource| {
                        let sync_status = resource.status.as_deref();
                        let health_status = resource.health.as_ref().map(|health| health.status.as_str());
                        let needs_attention = !matches!(sync_status, None | Some("Synced"))
                            || matches!(health_status, Some("Degraded" | "Missing" | "Suspended"));

                        if needs_attention {
                            Some(format!(
                                "{}/{}{}",
                                resource.kind,
                                resource.name,
                                resource
                                    .status
                                    .as_ref()
                                    .map(|status| format!(" [{}]", status))
                                    .unwrap_or_default()
                            ))
                        } else {
                            None
                        }
                    })
                    .take(3)
                    .collect();

                if !unhealthy_resources.is_empty() {
                    message_parts.push(format!("resources={}", unhealthy_resources.join(", ")));
                }
            }

            if let Some(transitions) = &status.transitions {
                if let Some(last_transition) = transitions.last() {
                    message_parts.push(format!(
                        "transition={}",
                        truncate_text(&last_transition.to_string(), MAX_STATUS_MESSAGE_CHARS)
                    ));
                }
            }
        }

        let health_is_healthy =
            matches!(health_status.map(|status| status.status.as_str()), Some("Healthy"));
        let sync_is_synced =
            matches!(sync_status.map(|status| status.status.as_str()), Some("Synced"));
        let failed = matches!(
            health_status.map(|status| status.status.as_str()),
            Some("Degraded" | "Missing")
        ) || matches!(
            operation_state.map(|status| status.phase.as_str()),
            Some("Error" | "Failed")
        );

        ApplicationReadiness {
            healthy: health_is_healthy && sync_is_synced,
            failed,
            message: if message_parts.is_empty() {
                None
            } else {
                Some(message_parts.join(", "))
            },
            reconciled_at,
        }
    }

    fn controller_metadata_with_status(
        &self,
        metadata: &ArgoCdMetadata,
        pods: Option<Value>,
    ) -> Result<Value> {
        let mut controller_metadata = serde_json::to_value(metadata)?;
        if let Some(obj) = controller_metadata.as_object_mut() {
            if let Some(pods) = pods {
                obj.insert("pod_status".to_string(), pods);
            }
        }
        Ok(controller_metadata)
    }

    async fn group_reconcile_owner(&self, deployment: &Deployment) -> Result<Option<Deployment>> {
        let deployments = db_deployments::find_non_terminal_for_project_and_group(
            &self.state.db_pool,
            deployment.project_id,
            &deployment.deployment_group,
        )
        .await?;

        Ok(deployments.into_iter().next())
    }

    async fn should_defer_to_newer_deployment(&self, deployment: &Deployment) -> Result<bool> {
        Ok(self
            .group_reconcile_owner(deployment)
            .await?
            .map(|owner| owner.id != deployment.id)
            .unwrap_or(false))
    }

    async fn rollback_to_active_deployment(
        &self,
        project: &Project,
        deployment: &Deployment,
        metadata: &mut ArgoCdMetadata,
    ) -> Result<()> {
        let active = db_deployments::find_active_for_project_and_group(
            &self.state.db_pool,
            project.id,
            &deployment.deployment_group,
        )
        .await?;

        match active {
            Some(active) if active.id != deployment.id => {
                let mut rollback_metadata = ArgoCdMetadata::default();
                let desired = self
                    .desired_application_for_deployment(project, &active, &mut rollback_metadata)
                    .await?;
                let _ = self
                    .patch_application(&desired.object.name_any(), &desired.object)
                    .await?;
                metadata.reverted_to_deployment_id = Some(active.deployment_id);
            }
            _ => {
                self.delete_group_application(project, &deployment.deployment_group)
                    .await?;
                metadata.reverted_to_deployment_id = None;
            }
        }

        Ok(())
    }

    async fn delete_group_application(&self, project: &Project, deployment_group: &str) -> Result<()> {
        let application_name = self.application_name(&project.name, deployment_group);
        if self.get_application_opt(&application_name).await?.is_some() {
            self.delete_application(&application_name).await?;
        }

        Ok(())
    }

    async fn delete_secret_if_exists(&self, namespace: &str, name: &str) -> Result<()> {
        if let Some(existing) = self.get_secret_opt(namespace, name).await? {
            if reconcile_ignored(&existing.metadata) {
                return Ok(());
            }
        }

        match self
            .secrets_api(namespace)
            .delete(name, &DeleteParams::default())
            .await
        {
            Ok(_) => Ok(()),
            Err(kube::Error::Api(ae)) if ae.code == 404 => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    async fn cleanup_deployment_secrets(&self, metadata: &ArgoCdMetadata) -> Result<()> {
        if let Some(namespace) = &metadata.destination_namespace {
            if let Some(env_secret) = &metadata.env_secret_name {
                self.delete_secret_if_exists(namespace, env_secret).await?;
            }
            if let Some(pull_secret) = &metadata.pull_secret_name {
                self.delete_secret_if_exists(namespace, pull_secret).await?;
            }
        }

        Ok(())
    }

    async fn reconcile_group_to_active_or_delete(&self, deployment: &Deployment) -> Result<()> {
        if self.should_defer_to_newer_deployment(deployment).await? {
            return Ok(());
        }

        let project = db_projects::find_by_id(&self.state.db_pool, deployment.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

        let active = db_deployments::find_active_for_project_and_group(
            &self.state.db_pool,
            project.id,
            &deployment.deployment_group,
        )
        .await?;

        match active {
            Some(active) if active.id != deployment.id => {
                let mut metadata = ArgoCdMetadata::default();
                let desired = self
                    .desired_application_for_deployment(&project, &active, &mut metadata)
                    .await?;
                let _ = self
                    .patch_application(&desired.object.name_any(), &desired.object)
                    .await?;
            }
            _ => {
                self.delete_group_application(&project, &deployment.deployment_group)
                    .await?;
            }
        }

        Ok(())
    }

    fn resolved_ingress_url_for_group(&self, project: &Project, deployment_group: &str) -> String {
        if deployment_group == DEFAULT_DEPLOYMENT_GROUP {
            self.production_ingress_url_template
                .replace("{project_name}", &project.name)
        } else if let Some(staging_template) = &self.staging_ingress_url_template {
            staging_template
                .replace("{project_name}", &project.name)
                .replace("{deployment_group}", &self.escaped_group_name(deployment_group))
        } else {
            let base = self
                .production_ingress_url_template
                .replace("{project_name}", &project.name);
            if let Some(dot_index) = base.find('.') {
                format!(
                    "{}-{}{}",
                    &base[..dot_index],
                    self.escaped_group_name(deployment_group),
                    &base[dot_index..]
                )
            } else {
                format!("{}-{}", base, self.escaped_group_name(deployment_group))
            }
        }
    }

    fn full_ingress_url_from_host(&self, host_or_path: &str) -> String {
        if let Some(port) = self.ingress_port {
            match host_or_path.find('/') {
                Some(slash) => format!(
                    "{}:{}{}",
                    &host_or_path[..slash],
                    port,
                    &host_or_path[slash..]
                ),
                None => format!("{}:{}", host_or_path, port),
            }
        } else {
            host_or_path.to_string()
        }
    }

    fn parse_ingress_target(url: &str) -> (String, String) {
        match url.find('/') {
            Some(slash) => (url[..slash].to_string(), url[slash..].to_string()),
            None => (url.to_string(), "/".to_string()),
        }
    }

    async fn build_urls(&self, project: &Project, deployment_group: &str) -> Result<DeploymentUrls> {
        let default_url = format!(
            "{}://{}",
            self.ingress_schema,
            self.full_ingress_url_from_host(&self.resolved_ingress_url_for_group(
                project,
                deployment_group,
            ))
        );

        let mut custom_domain_urls = Vec::new();
        let mut primary_url = default_url.clone();

        if deployment_group == DEFAULT_DEPLOYMENT_GROUP {
            let custom_domains =
                db_custom_domains::list_project_custom_domains(&self.state.db_pool, project.id)
                    .await?;
            for domain in &custom_domains {
                let host = if let Some(port) = self.ingress_port {
                    format!("{}:{}", domain.domain, port)
                } else {
                    domain.domain.clone()
                };
                let url = format!("{}://{}", self.ingress_schema, host);
                if domain.is_primary {
                    primary_url = url.clone();
                }
                custom_domain_urls.push(url);
            }
        }

        Ok(DeploymentUrls {
            default_url,
            primary_url,
            custom_domain_urls,
        })
    }
}

#[async_trait]
impl DeploymentBackend for ArgoCdController {
    async fn reconcile(
        &self,
        deployment: &Deployment,
        project: &Project,
    ) -> Result<ReconcileResult> {
        if !matches!(
            deployment.status,
            DeploymentStatus::Pushed
                | DeploymentStatus::Deploying
                | DeploymentStatus::Unhealthy
                | DeploymentStatus::Healthy
        ) {
            return Ok(ReconcileResult {
                status: deployment.status.clone(),
                controller_metadata: deployment.controller_metadata.clone(),
                error_message: None,
            });
        }

        if self.should_defer_to_newer_deployment(deployment).await? {
            return Ok(ReconcileResult {
                status: deployment.status.clone(),
                controller_metadata: deployment.controller_metadata.clone(),
                error_message: None,
            });
        }

        let mut metadata: ArgoCdMetadata =
            serde_json::from_value(deployment.controller_metadata.clone()).unwrap_or_default();
        let desired = self
            .desired_application_for_deployment(project, deployment, &mut metadata)
            .await?;
        let application = self.ensure_application(&desired, &mut metadata).await?;
        let readiness = self.evaluate_application_status(&application);
        let pod_health = if let Some(namespace) = metadata.destination_namespace.as_deref() {
            self.collect_matched_pod_health(namespace, &project.name, deployment)
                .await
                .ok()
        } else {
            None
        };
        let rollout_healthy = readiness.healthy
            && pod_health
                .as_ref()
                .map(|pods| pods.healthy)
                .unwrap_or(false);

        let last_applied_at = metadata.last_applied_at;
        let reconciled_after_apply = match (last_applied_at, readiness.reconciled_at) {
            (Some(applied_at), Some(reconciled_at)) => reconciled_at >= applied_at,
            (None, _) => true,
            _ => false,
        };

        if !reconciled_after_apply {
            return Ok(ReconcileResult {
                status: if deployment.first_healthy_at.is_some() {
                    DeploymentStatus::Unhealthy
                } else {
                    DeploymentStatus::Deploying
                },
                controller_metadata: self.controller_metadata_with_status(
                    &metadata,
                    pod_health.as_ref().map(|pods| pods.status.clone()),
                )?,
                error_message: pod_health
                    .as_ref()
                    .and_then(|pods| pods.message.clone())
                    .or(readiness.message),
            });
        }

        let status = if rollout_healthy {
            DeploymentStatus::Healthy
        } else if readiness.failed {
            self.rollback_to_active_deployment(project, deployment, &mut metadata)
                .await?;
            DeploymentStatus::Failed
        } else if deployment.first_healthy_at.is_some() {
            DeploymentStatus::Unhealthy
        } else {
            DeploymentStatus::Deploying
        };

        Ok(ReconcileResult {
            status,
            controller_metadata: self.controller_metadata_with_status(
                &metadata,
                pod_health.as_ref().map(|pods| pods.status.clone()),
            )?,
            error_message: pod_health
                .as_ref()
                .and_then(|pods| pods.message.clone())
                .or(readiness.message),
        })
    }

    async fn health_check(&self, deployment: &Deployment) -> Result<HealthStatus> {
        let metadata: ArgoCdMetadata = serde_json::from_value(deployment.controller_metadata.clone())
            .map_err(|err| anyhow::anyhow!("Invalid ArgoCD metadata: {}", err))?;
        let application_name = metadata
            .application_name
            .ok_or_else(|| anyhow::anyhow!("No application_name in ArgoCD metadata"))?;
        let application = self.get_application(&application_name).await?;
        let readiness = self.evaluate_application_status(&application);
        let project = db_projects::find_by_id(&self.state.db_pool, deployment.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;
        let configured_deployment_id = application_configured_deployment_id(&application);
        let deployment_mismatch = configured_deployment_id.as_deref() != Some(&deployment.deployment_id);
        let defer_to_newer = if deployment_mismatch {
            self.should_defer_to_newer_deployment(deployment).await?
        } else {
            false
        };

        if defer_to_newer {
            return Ok(HealthStatus {
                healthy: true,
                message: Some(format!(
                    "Application is currently configured for newer deployment {}",
                    configured_deployment_id.as_deref().unwrap_or("unknown")
                )),
                last_check: Utc::now(),
                pod_status: None,
            });
        }

        let destination_namespace = metadata
            .destination_namespace
            .ok_or_else(|| anyhow::anyhow!("No destination namespace in ArgoCD metadata"))?;
        let pod_health = self
            .collect_matched_pod_health(&destination_namespace, &project.name, deployment)
            .await?;

        let message = match (
            pod_health.message.as_deref(),
            readiness.message.as_deref(),
            deployment_mismatch,
        ) {
            (Some(pod_message), Some(app_message), true) => Some(format!(
                "{pod_message}, {app_message}, configuredDeploymentId={}",
                configured_deployment_id.as_deref().unwrap_or("unknown")
            )),
            (Some(pod_message), Some(app_message), false) => {
                Some(format!("{pod_message}, {app_message}"))
            }
            (Some(pod_message), None, true) => Some(format!(
                "{pod_message}, configuredDeploymentId={}",
                configured_deployment_id.as_deref().unwrap_or("unknown")
            )),
            (Some(pod_message), None, false) => Some(pod_message.to_string()),
            (None, Some(app_message), true) => Some(format!(
                "{app_message}, configuredDeploymentId={}",
                configured_deployment_id.as_deref().unwrap_or("unknown")
            )),
            (None, Some(app_message), false) => Some(app_message.to_string()),
            (None, None, true) => Some(format!(
                "Application is currently configured for deployment {}",
                configured_deployment_id.as_deref().unwrap_or("unknown")
            )),
            (None, None, false) => None,
        };

        Ok(HealthStatus {
            healthy: readiness.healthy && pod_health.healthy,
            message,
            last_check: Utc::now(),
            pod_status: Some(pod_health.status),
        })
    }

    async fn cancel(&self, deployment: &Deployment) -> Result<()> {
        let metadata: ArgoCdMetadata =
            serde_json::from_value(deployment.controller_metadata.clone()).unwrap_or_default();
        self.cleanup_deployment_secrets(&metadata).await?;
        self.reconcile_group_to_active_or_delete(deployment).await?;
        Ok(())
    }

    async fn terminate(&self, deployment: &Deployment) -> Result<()> {
        let metadata: ArgoCdMetadata =
            serde_json::from_value(deployment.controller_metadata.clone()).unwrap_or_default();
        self.cleanup_deployment_secrets(&metadata).await?;
        self.reconcile_group_to_active_or_delete(deployment).await?;
        Ok(())
    }

    async fn get_deployment_urls(
        &self,
        deployment: &Deployment,
        project: &Project,
    ) -> Result<DeploymentUrls> {
        self.build_urls(project, &deployment.deployment_group).await
    }

    async fn get_project_urls(
        &self,
        project: &Project,
        deployment_group: &str,
    ) -> Result<DeploymentUrls> {
        self.build_urls(project, deployment_group).await
    }

    async fn stream_logs(
        &self,
        deployment: &Deployment,
        follow: bool,
        tail_lines: Option<i64>,
        timestamps: bool,
        since_seconds: Option<i64>,
    ) -> Result<futures::stream::BoxStream<'static, Result<bytes::Bytes, anyhow::Error>>> {
        let metadata: ArgoCdMetadata =
            serde_json::from_value(deployment.controller_metadata.clone()).unwrap_or_default();
        let namespace = metadata
            .destination_namespace
            .ok_or_else(|| anyhow::anyhow!("No destination namespace in ArgoCD metadata"))?;
        let project = db_projects::find_by_id(&self.state.db_pool, deployment.project_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Project not found"))?;
        let normalized_group = self.escaped_group_name(&deployment.deployment_group);

        let pod_api: Api<Pod> = Api::namespaced(self.kube_client.clone(), &namespace);
        let label_selector = format!(
            "{project_label}={project},{group_label}={group},{deployment_label}={deployment_id}",
            project_label = LABEL_PROJECT,
            project = project.name,
            group_label = LABEL_DEPLOYMENT_GROUP,
            group = normalized_group,
            deployment_label = LABEL_DEPLOYMENT_ID,
            deployment_id = deployment.deployment_id,
        );
        let pods = pod_api.list(&ListParams::default().labels(&label_selector)).await?;

        let pod_names: Vec<String> = pods
            .items
            .into_iter()
            .filter_map(|pod| pod.metadata.name)
            .collect();

        if pod_names.is_empty() {
            return Err(anyhow::anyhow!(
                "No pods found for deployment {} in namespace {}",
                deployment.deployment_id,
                namespace
            ));
        }

        let multiple_pods = pod_names.len() > 1;
        let mut streams = futures::stream::SelectAll::new();

        for pod_name in pod_names {
            let pod_api = pod_api.clone();
            let pod_name_for_stream = pod_name.clone();
            let mut log_params = LogParams {
                follow,
                timestamps,
                ..Default::default()
            };
            log_params.tail_lines = tail_lines;
            log_params.since_seconds = since_seconds;

            let log_stream = pod_api.log_stream(&pod_name, &log_params).await?;
            let stream = async_stream::stream! {
                let mut lines = log_stream.lines();
                loop {
                    match lines.next().await {
                        Some(Ok(line)) => {
                            let rendered = if multiple_pods {
                                format!("[{}] {}\n", pod_name_for_stream, line)
                            } else {
                                format!("{}\n", line)
                            };
                            yield Ok(Bytes::from(rendered));
                        }
                        Some(Err(err)) => {
                            yield Err(anyhow::anyhow!(
                                "Log stream error for pod {}: {}",
                                pod_name_for_stream,
                                err
                            ));
                            break;
                        }
                        None => break,
                    }
                }
            };
            streams.push(stream.boxed());
        }

        Ok(streams.boxed())
    }
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ArgoCdControllerConfig {
        ArgoCdControllerConfig {
            argocd_namespace: "argocd".to_string(),
            production_ingress_url_template: "{project_name}.apps.example.com".to_string(),
            staging_ingress_url_template: None,
            ingress_port: None,
            ingress_schema: "https".to_string(),
            appproject_format: "rise-{project_name}".to_string(),
            application_format: "rise-{project_name}-{deployment_group}".to_string(),
            destination_namespace_format: "rise-{project_name}-{deployment_group}".to_string(),
            destination_server: "https://kubernetes.default.svc".to_string(),
            namespace_labels: HashMap::new(),
            namespace_annotations: HashMap::new(),
            helm_chart: ArgoCdHelmChartConfig {
                repo_url: "https://charts.example.com".to_string(),
                chart: "app".to_string(),
                target_revision: "1.2.3".to_string(),
                values: json!({}),
            },
            sync_options: vec![
                "ServerSideApply=true".to_string(),
                "ApplyOutOfSyncOnly=true".to_string(),
            ],
            access_classes: HashMap::new(),
            registry_provider: Arc::new(crate::server::registry::providers::OciClientAuthProvider::new(
                crate::server::registry::models::OciClientAuthConfig {
                    registry_url: "registry.example.com".to_string(),
                    namespace: String::new(),
                    client_registry_url: None,
                },
            ).expect("provider")),
        }
    }

    fn test_kube_client() -> kube::Client {
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();

        kube::Client::try_from(kube::Config::new(axum::http::Uri::from_static(
            "http://127.0.0.1",
        )))
        .expect("client")
    }

    #[tokio::test]
    async fn naming_templates_use_normalized_group_names() {
        let controller = ArgoCdController::new(
            ControllerState {
                db_pool: sqlx::PgPool::connect_lazy("postgres://unused").expect("pool"),
                encryption_provider: None,
            },
            test_kube_client(),
            sample_config(),
        )
        .expect("controller");

        assert_eq!(controller.appproject_name("hello-world"), "rise-hello-world");
        assert_eq!(
            controller.application_name("hello-world", "mr/42"),
            "rise-hello-world-mr--42"
        );
        assert_eq!(
            controller.destination_namespace("hello-world", "mr/42"),
            "rise-hello-world-mr--42"
        );
    }

    #[tokio::test]
    async fn builds_oci_application_source_reference() {
        let controller = ArgoCdController::new(
            ControllerState {
                db_pool: sqlx::PgPool::connect_lazy("postgres://unused").expect("pool"),
                encryption_provider: None,
            },
            test_kube_client(),
            ArgoCdControllerConfig {
                helm_chart: ArgoCdHelmChartConfig {
                    repo_url: "oci://rise-registry:5000".to_string(),
                    chart: "helm-charts/rise-app".to_string(),
                    target_revision: "0.1.0".to_string(),
                    values: json!({}),
                },
                ..sample_config()
            },
        )
        .expect("controller");

        let (repo_url, chart, path) = controller.application_source_reference();
        assert_eq!(repo_url, "oci://rise-registry:5000/helm-charts/rise-app");
        assert_eq!(chart, None);
        assert_eq!(path, Some(".".to_string()));
    }

    #[test]
    fn canonicalize_json_sorts_object_keys_recursively() {
        let input = json!({
            "z": 1,
            "a": {
                "d": 4,
                "b": 2
            }
        });

        let output = canonicalize_json(input);
        assert_eq!(
            serde_json::to_string(&output).unwrap(),
            r#"{"a":{"b":2,"d":4},"z":1}"#
        );
    }

    #[test]
    fn reconcile_ignored_recognizes_true_label() {
        let metadata = ObjectMeta {
            labels: Some(BTreeMap::from([(
                LABEL_IGNORE_RECONCILE.to_string(),
                "true".to_string(),
            )])),
            ..Default::default()
        };

        assert!(reconcile_ignored(&metadata));
    }

    #[tokio::test]
    async fn evaluate_application_status_marks_degraded_as_failed() {
        let application = Application {
            status: Some(ApplicationStatus {
                health: Some(ApplicationHealthStatus {
                    status: "Degraded".to_string(),
                    message: Some("deployment failed".to_string()),
                    last_transition_time: None,
                }),
                sync: Some(ApplicationSyncStatus {
                    status: "OutOfSync".to_string(),
                    revision: None,
                }),
                operation_state: Some(ApplicationOperationState {
                    phase: "Failed".to_string(),
                    message: Some("sync failed".to_string()),
                    started_at: None,
                    finished_at: None,
                }),
                conditions: Some(vec![ApplicationCondition {
                    type_: "ComparisonError".to_string(),
                    message: Some("failed to generate manifest".to_string()),
                    last_transition_time: Some("2026-03-16T23:54:06Z".to_string()),
                }]),
                summary: None,
                resources: None,
                history: None,
                transitions: None,
                reconciled_at: Some(Utc::now().to_rfc3339()),
            }),
            ..Application::new(
                "rise-hello-world-default",
                ApplicationSpec {
                    project: "rise-hello-world".to_string(),
                    source: ApplicationSource::default(),
                    destination: ApplicationDestination::default(),
                    sync_policy: None,
                },
            )
        };

        let controller = ArgoCdController {
            state: ControllerState {
                db_pool: sqlx::PgPool::connect_lazy("postgres://unused").expect("pool"),
                encryption_provider: None,
            },
            kube_client: test_kube_client(),
            argocd_namespace: "argocd".to_string(),
            production_ingress_url_template: "{project_name}.apps.example.com".to_string(),
            staging_ingress_url_template: None,
            ingress_port: None,
            ingress_schema: "https".to_string(),
            appproject_format: "rise-{project_name}".to_string(),
            application_format: "rise-{project_name}-{deployment_group}".to_string(),
            destination_namespace_format: "rise-{project_name}-{deployment_group}".to_string(),
            destination_server: "https://kubernetes.default.svc".to_string(),
            namespace_labels: HashMap::new(),
            namespace_annotations: HashMap::new(),
            helm_chart: ArgoCdHelmChartConfig {
                repo_url: "https://charts.example.com".to_string(),
                chart: "app".to_string(),
                target_revision: "1.2.3".to_string(),
                values: json!({}),
            },
            sync_options: vec![],
            access_classes: HashMap::new(),
            registry_provider: Arc::new(crate::server::registry::providers::OciClientAuthProvider::new(
                crate::server::registry::models::OciClientAuthConfig {
                    registry_url: "registry.example.com".to_string(),
                    namespace: String::new(),
                    client_registry_url: None,
                },
            ).expect("provider")),
        };

        let readiness = controller.evaluate_application_status(&application);
        assert!(readiness.failed);
        assert!(!readiness.healthy);
    }

}
