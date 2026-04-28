use std::collections::HashSet;
use std::fmt::Debug;

use k8s_openapi::api::apps::v1::Deployment as K8sDeployment;
use k8s_openapi::api::core::v1::{Endpoints, Namespace, Secret, Service, ServiceAccount};
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::Client;
use kube::CustomResource;
use kube::ResourceExt;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{debug, info, warn};

/// CRD spec for RiseProject — intentionally empty.
/// The database remains the source of truth; the CRD is a marker that tells
/// Metacontroller "this project exists, manage its resources."
#[derive(CustomResource, Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "rise.dev",
    version = "v1alpha1",
    kind = "RiseProject",
    plural = "riseprojects",
    shortname = "rp",
    status = "RiseProjectStatus",
    derive = "Default"
)]
pub struct RiseProjectSpec {}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RiseProjectStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync_time: Option<String>,
    /// Set by Metacontroller to track which generation of the spec has been observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
}

/// Annotation key used to trigger an immediate Metacontroller resync.
/// Updating this annotation causes the CRD's `metadata.resourceVersion` to change,
/// which Metacontroller detects and triggers a sync webhook call.
const TRIGGER_ANNOTATION: &str = "rise.dev/trigger";

/// Create or update a `RiseProject` CRD for the given project.
/// Called when a project is created.
pub async fn ensure_rise_project(client: &Client, project_name: &str) -> anyhow::Result<()> {
    let api: Api<RiseProject> = Api::all(client.clone());

    let rise_project = RiseProject::new(project_name, RiseProjectSpec {});

    api.patch(
        project_name,
        &PatchParams::apply("rise-controller").force(),
        &Patch::Apply(&rise_project),
    )
    .await?;

    info!("Ensured RiseProject CRD for project '{}'", project_name);
    Ok(())
}

/// Delete the `RiseProject` CRD for the given project.
/// Metacontroller will call the finalize webhook, which cleans up all children.
pub async fn delete_rise_project(client: &Client, project_name: &str) -> anyhow::Result<()> {
    let api: Api<RiseProject> = Api::all(client.clone());

    match api
        .delete(project_name, &kube::api::DeleteParams::default())
        .await
    {
        Ok(_) => {
            info!("Deleted RiseProject CRD for project '{}'", project_name);
        }
        Err(kube::Error::Api(err)) if err.code == 404 => {
            debug!(
                "RiseProject CRD for project '{}' did not exist (already deleted)",
                project_name
            );
        }
        Err(e) => {
            return Err(e.into());
        }
    }

    Ok(())
}

/// Update the trigger annotation on a `RiseProject` CRD to force an immediate resync.
/// Called when deployment state changes (e.g., image pushed, status updated, stopped).
pub async fn trigger_resync(client: &Client, project_name: &str) -> anyhow::Result<()> {
    let api: Api<RiseProject> = Api::all(client.clone());
    let timestamp = chrono::Utc::now().to_rfc3339();

    let patch = serde_json::json!({
        "metadata": {
            "annotations": {
                TRIGGER_ANNOTATION: timestamp,
            },
        },
    });

    match api
        .patch(project_name, &PatchParams::default(), &Patch::Merge(patch))
        .await
    {
        Ok(_) => {
            info!(
                "Triggered resync for RiseProject '{}' (trigger={})",
                project_name, timestamp
            );
        }
        Err(kube::Error::Api(err)) if err.code == 404 => {
            warn!(
                "Cannot trigger resync: RiseProject '{}' not found (project may have been deleted)",
                project_name
            );
        }
        Err(e) => {
            return Err(e.into());
        }
    }

    Ok(())
}

/// Backfill missing `RiseProject` CRDs for all active projects in the database.
///
/// This handles two scenarios:
/// 1. **Upgrade**: When migrating to Metacontroller, no RiseProject CRDs exist yet.
/// 2. **Recovery**: If a RiseProject CRD is accidentally deleted, it gets recreated.
///
/// When `adopt_existing` is true, newly-created CRDs will also have their
/// pre-existing child resources (Namespace, Deployment, Service, etc.) patched
/// with Metacontroller's `controller-uid` label so Metacontroller adopts them.
///
/// Runs once at server startup. Failures for individual projects are logged as
/// warnings but do not block startup or affect other projects.
pub async fn backfill_rise_projects(
    client: &Client,
    db_pool: &PgPool,
    adopt_existing: bool,
) -> anyhow::Result<()> {
    // 1. List all RiseProject CRDs currently in the cluster
    let api: Api<RiseProject> = Api::all(client.clone());
    let existing_crds = api.list(&ListParams::default()).await?;
    let existing_names: HashSet<String> =
        existing_crds.items.iter().map(|r| r.name_any()).collect();

    // 2. List all active (non-Deleting, non-Terminated) projects from the database
    let active_projects = crate::db::projects::list_active(db_pool).await?;

    // 3. Find projects missing a CRD and create them
    let mut created = 0u32;
    for project in &active_projects {
        if !existing_names.contains(&project.name) {
            if let Err(e) = ensure_rise_project(client, &project.name).await {
                warn!(
                    project = %project.name,
                    "Failed to backfill RiseProject CRD: {:?}", e
                );
                continue;
            }
            created += 1;

            // Adopt pre-existing child resources for newly-created CRDs
            if adopt_existing {
                if let Err(e) = adopt_children_for_project(client, &project.name).await {
                    warn!(
                        project = %project.name,
                        "Failed to adopt existing resources: {:?}", e
                    );
                }
            }
        }
    }

    if created > 0 {
        info!(
            "Backfilled {} RiseProject CRD(s) ({} active projects, {} already existed)",
            created,
            active_projects.len(),
            existing_names.len()
        );
    } else {
        debug!(
            "No RiseProject CRDs to backfill ({} active projects, {} CRDs present)",
            active_projects.len(),
            existing_names.len()
        );
    }

    Ok(())
}

/// Label key used by Metacontroller (with `generateSelector: true`) to track
/// ownership of child resources.
const CONTROLLER_UID_LABEL: &str = "controller-uid";

/// Patch a single resource's labels with the `controller-uid` label via strategic merge patch.
async fn patch_resource_label<T>(
    api: &Api<T>,
    name: &str,
    uid: &str,
    type_name: &str,
) -> Result<(), kube::Error>
where
    T: kube::Resource<DynamicType = ()> + DeserializeOwned + Serialize + Clone + Debug,
{
    let patch = serde_json::json!({
        "metadata": {
            "labels": {
                CONTROLLER_UID_LABEL: uid,
            },
        },
    });
    api.patch(name, &PatchParams::default(), &Patch::Merge(patch))
        .await?;
    debug!(
        "Patched {} '{}' with controller-uid={}",
        type_name, name, uid
    );
    Ok(())
}

/// List resources matching `lp` and patch each with the `controller-uid` label,
/// skipping any that already have it. Best-effort: errors are logged, not propagated.
async fn patch_all_matching<T>(api: &Api<T>, lp: &ListParams, uid: &str, type_name: &str)
where
    T: kube::Resource<DynamicType = ()> + DeserializeOwned + Serialize + Clone + Debug,
{
    let items = match api.list(lp).await {
        Ok(list) => list.items,
        Err(e) => {
            warn!("Failed to list {} for adoption: {:?}", type_name, e);
            return;
        }
    };

    for item in &items {
        let name = item.name_any();
        // Skip resources already labeled
        if item
            .labels()
            .get(CONTROLLER_UID_LABEL)
            .is_some_and(|v| !v.is_empty())
        {
            debug!(
                "{} '{}' already has controller-uid, skipping",
                type_name, name
            );
            continue;
        }
        match patch_resource_label(api, &name, uid, type_name).await {
            Ok(()) => {}
            Err(kube::Error::Api(err)) if err.code == 404 => {
                debug!("{} '{}' not found (deleted?), skipping", type_name, name);
            }
            Err(e) => {
                warn!(
                    "Failed to patch {} '{}' with controller-uid: {:?}",
                    type_name, name, e
                );
            }
        }
    }
}

/// Adopt pre-existing child resources for a project by patching them with the
/// Metacontroller `controller-uid` label derived from the RiseProject CRD's UID.
///
/// This is best-effort: individual failures are warned but do not block startup.
async fn adopt_children_for_project(client: &Client, project_name: &str) -> anyhow::Result<()> {
    // Read back the CRD to get its UID
    let crd_api: Api<RiseProject> = Api::all(client.clone());
    let crd = crd_api.get(project_name).await?;
    let uid = crd
        .metadata
        .uid
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("RiseProject '{}' has no UID", project_name))?;

    let ns_name = format!("rise-{}", project_name);

    // 1. Patch the namespace (cluster-scoped)
    let ns_api: Api<Namespace> = Api::all(client.clone());
    match patch_resource_label(&ns_api, &ns_name, uid, "Namespace").await {
        Ok(()) => {}
        Err(kube::Error::Api(err)) if err.code == 404 => {
            debug!("Namespace '{}' not found, skipping adoption", ns_name);
        }
        Err(e) => {
            warn!(
                "Failed to patch Namespace '{}' with controller-uid: {:?}",
                ns_name, e
            );
        }
    }

    // 2. Patch namespaced children: list by managed-by label, skip already-labeled.
    //    Support both the current label (app.kubernetes.io/managed-by=rise) and the
    //    legacy label (rise.dev/managed-by=rise) used by older controller versions.
    let label_selectors = [
        ListParams::default().labels("app.kubernetes.io/managed-by=rise"),
        ListParams::default().labels("rise.dev/managed-by=rise"),
    ];

    // Secret (by managed-by label)
    let secret_api: Api<Secret> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&secret_api, lp, uid, "Secret").await;
    }

    // Image pull secret — the old controller created this without any labels,
    // so it won't be found by label selectors. Adopt it by name directly.
    match patch_resource_label(
        &secret_api,
        super::resource_builder::IMAGE_PULL_SECRET_NAME,
        uid,
        "Secret",
    )
    .await
    {
        Ok(()) => {}
        Err(kube::Error::Api(err)) if err.code == 404 => {
            debug!(
                "Image pull secret '{}' not found in '{}', skipping",
                super::resource_builder::IMAGE_PULL_SECRET_NAME,
                ns_name
            );
        }
        Err(e) => {
            warn!(
                "Failed to patch image pull secret '{}' in '{}' with controller-uid: {:?}",
                super::resource_builder::IMAGE_PULL_SECRET_NAME,
                ns_name,
                e
            );
        }
    }

    // ServiceAccount
    let sa_api: Api<ServiceAccount> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&sa_api, lp, uid, "ServiceAccount").await;
    }

    // Deployment
    let deploy_api: Api<K8sDeployment> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&deploy_api, lp, uid, "Deployment").await;
    }

    // Service
    let svc_api: Api<Service> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&svc_api, lp, uid, "Service").await;
    }

    // Endpoints
    let ep_api: Api<Endpoints> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&ep_api, lp, uid, "Endpoints").await;
    }

    // Ingress
    let ing_api: Api<Ingress> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&ing_api, lp, uid, "Ingress").await;
    }

    // NetworkPolicy
    let np_api: Api<NetworkPolicy> = Api::namespaced(client.clone(), &ns_name);
    for lp in &label_selectors {
        patch_all_matching(&np_api, lp, uid, "NetworkPolicy").await;
    }

    info!(
        "Adopted existing resources for project '{}' (uid={})",
        project_name, uid
    );
    Ok(())
}
