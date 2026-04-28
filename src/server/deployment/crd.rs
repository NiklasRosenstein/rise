use kube::api::{Api, Patch, PatchParams};
use kube::Client;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
        .patch(
            project_name,
            &PatchParams::default(),
            &Patch::Merge(patch),
        )
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
