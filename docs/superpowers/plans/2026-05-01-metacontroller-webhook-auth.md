# Metacontroller Webhook Auth — Pod-IP Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the shared `webhookToken` query-parameter auth on the metacontroller sync/finalize webhooks with Kubernetes pod-IP validation, eliminating the plaintext secret from the `CompositeController` CRD.

**Architecture:** A new `MetacontrollerIpValidator` component holds a TTL-cached set of live metacontroller pod IPs (refreshed via the Kubernetes API). Every incoming webhook request is validated against this set using the TCP source IP extracted via Axum's `ConnectInfo`. In development (Rise running on the host), no validator is constructed and all callers are allowed through; in production the validator is required.

**Tech Stack:** Rust / Axum (`ConnectInfo`), kube-rs (`Api<Pod>`, `ListParams`), Helm, YAML config files.

---

## File Map

| Action | File | Responsibility |
|---|---|---|
| Create | `src/server/deployment/ip_validator.rs` | `MetacontrollerIpValidator` struct + `check_ip` pure fn |
| Modify | `src/server/deployment/mod.rs` | add `pub mod ip_validator` |
| Modify | `src/server/settings.rs` | remove token field; add `metacontroller_pod_namespace` + `metacontroller_pod_label_selector`; update 2 inline test configs |
| Modify | `src/server/state.rs` | swap `metacontroller_webhook_token` for `metacontroller_ip_validator`; new startup guards |
| Modify | `src/server/mod.rs` | listener gate; add `into_make_service_with_connect_info` |
| Modify | `src/server/deployment/webhook.rs` | remove token extraction; add `ConnectInfo` + `validate_source_ip` |
| Modify | `config/development.yaml` | remove `metacontroller_webhook_token` line |
| Modify | `config/production.yaml` | replace `metacontroller_webhook_token` with `metacontroller_pod_namespace` |
| Modify | `helm/rise/templates/metacontroller.yaml` | remove `$tokenParam` block |
| Modify | `helm/rise/values.yaml` | remove `webhookToken`; update comment |
| Modify | `helm/rise/values-dev.yaml` | remove `webhookToken` |
| Modify | `helm/rise/templates/deployment.yaml` | inject `RISE_METACONTROLLER_POD_NAMESPACE` env var |
| Modify | `docs/kubernetes.md` | rewrite webhook auth section |

---

## Task 1: Add `ip_validator.rs` with tests

**Files:**
- Create: `src/server/deployment/ip_validator.rs`

- [ ] **Step 1: Write the tests first**

Create `src/server/deployment/ip_validator.rs` with only the test module and a stub for `check_ip`:

```rust
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::Api;
use tokio::sync::Mutex;
use tracing::warn;

const DEFAULT_TTL: Duration = Duration::from_secs(15);
pub const DEFAULT_LABEL_SELECTOR: &str = "app.kubernetes.io/name=metacontroller-operator";

pub struct MetacontrollerIpValidator {
    kube_client: kube::Client,
    pub namespace: String,
    pub label_selector: String,
    ttl: Duration,
    cache: Mutex<IpCache>,
}

struct IpCache {
    ips: HashSet<IpAddr>,
    fetched_at: Option<Instant>,
}

impl MetacontrollerIpValidator {
    pub fn new(
        kube_client: kube::Client,
        namespace: String,
        label_selector: Option<String>,
    ) -> Self {
        Self {
            kube_client,
            namespace,
            label_selector: label_selector
                .unwrap_or_else(|| DEFAULT_LABEL_SELECTOR.to_string()),
            ttl: DEFAULT_TTL,
            cache: Mutex::new(IpCache {
                ips: HashSet::new(),
                fetched_at: None,
            }),
        }
    }

    pub async fn validate(&self, addr: SocketAddr) -> Result<(), (StatusCode, &'static str)> {
        let mut cache = self.cache.lock().await;

        let is_fresh = cache
            .fetched_at
            .map(|t| t.elapsed() < self.ttl)
            .unwrap_or(false);

        if !is_fresh {
            match self.fetch_pod_ips().await {
                Ok(ips) => {
                    cache.ips = ips;
                    cache.fetched_at = Some(Instant::now());
                }
                Err(e) => {
                    warn!("Failed to refresh metacontroller pod IPs: {:?}", e);
                    if cache.fetched_at.is_none() {
                        return Err((
                            StatusCode::SERVICE_UNAVAILABLE,
                            "Cannot validate source IP: Kubernetes API unavailable",
                        ));
                    }
                    // Use stale cache
                }
            }
        }

        check_ip(&cache.ips, addr)
    }

    async fn fetch_pod_ips(&self) -> anyhow::Result<HashSet<IpAddr>> {
        let pods: Api<Pod> = Api::namespaced(self.kube_client.clone(), &self.namespace);
        let lp = ListParams::default().labels(&self.label_selector);
        let pod_list = pods.list(&lp).await?;

        Ok(pod_list
            .items
            .iter()
            .filter_map(|pod| {
                pod.status
                    .as_ref()
                    .and_then(|s| s.pod_ip.as_deref())
                    .and_then(|ip| ip.parse().ok())
            })
            .collect())
    }
}

fn check_ip(ips: &HashSet<IpAddr>, addr: SocketAddr) -> Result<(), (StatusCode, &'static str)> {
    if ips.contains(&addr.ip()) {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Webhook source IP not authorized"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn addr(a: u8, b: u8, c: u8, d: u8) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(a, b, c, d), 12345))
    }

    fn ip_set(octets: &[(u8, u8, u8, u8)]) -> HashSet<IpAddr> {
        octets
            .iter()
            .map(|(a, b, c, d)| IpAddr::V4(Ipv4Addr::new(*a, *b, *c, *d)))
            .collect()
    }

    #[test]
    fn test_check_ip_authorized() {
        let ips = ip_set(&[(10, 0, 0, 1)]);
        assert!(check_ip(&ips, addr(10, 0, 0, 1)).is_ok());
    }

    #[test]
    fn test_check_ip_unauthorized() {
        let ips = ip_set(&[(10, 0, 0, 1)]);
        let (status, _) = check_ip(&ips, addr(10, 0, 0, 2)).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_check_ip_empty_set_rejects() {
        let (status, _) = check_ip(&HashSet::new(), addr(10, 0, 0, 1)).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_check_ip_multiple_pods() {
        let ips = ip_set(&[(10, 0, 0, 1), (10, 0, 0, 2), (10, 0, 0, 3)]);
        assert!(check_ip(&ips, addr(10, 0, 0, 2)).is_ok());
        let (status, _) = check_ip(&ips, addr(10, 0, 0, 4)).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}
```

- [ ] **Step 2: Register the module**

In `src/server/deployment/mod.rs`, add after the existing `#[cfg(feature = "backend")] pub mod webhook;` line:

```rust
#[cfg(feature = "backend")]
pub mod ip_validator;
```

- [ ] **Step 3: Run the tests**

```bash
cargo test --all-features -- deployment::ip_validator
```

Expected: all 4 tests pass.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add src/server/deployment/ip_validator.rs src/server/deployment/mod.rs
git commit -m "feat: add MetacontrollerIpValidator with pod-IP cache"
```

---

## Task 2: Update settings — remove token, add namespace fields

**Files:**
- Modify: `src/server/settings.rs`

- [ ] **Step 1: Remove the token field and add the two new fields**

In `src/server/settings.rs`, locate the `metacontroller_webhook_port` field (around line 680) and replace the block that follows it:

```rust
        // REMOVE this entire block:
        /// Shared secret token for authenticating Metacontroller webhook requests.
        /// Required when deployment controller is configured.
        /// Generate with: openssl rand -base64 32
        metacontroller_webhook_token: String,
```

Replace with:

```rust
        /// Kubernetes namespace where the Metacontroller operator pod runs.
        /// Used to validate webhook source IPs against live pod IPs.
        /// Required in production (enforced at startup); omit only in development.
        #[serde(default)]
        metacontroller_pod_namespace: String,

        /// Pod label selector used to identify Metacontroller pods for IP validation.
        /// Defaults to "app.kubernetes.io/name=metacontroller-operator".
        #[serde(default)]
        metacontroller_pod_label_selector: Option<String>,
```

- [ ] **Step 2: Update inline test config #1**

In `src/server/settings.rs`, find the first inline test config (~line 1175):
```
  metacontroller_webhook_token: "test-token"
```
Replace with:
```
  metacontroller_pod_namespace: "metacontroller"
```

- [ ] **Step 3: Update inline test config #2**

Find the second inline test config (~line 1260):
```
  metacontroller_webhook_token: "test-token"
```
Replace with:
```
  metacontroller_pod_namespace: "metacontroller"
```

- [ ] **Step 4: Verify it compiles (settings only)**

```bash
cargo check --all-features 2>&1 | grep "settings"
```

Expected: errors only in state.rs and webhook.rs (which reference the removed field), not in settings.rs itself.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add src/server/settings.rs
git commit -m "feat: replace metacontroller_webhook_token with metacontroller_pod_namespace in settings"
```

---

## Task 3: Update `AppState` — swap token for validator

**Files:**
- Modify: `src/server/state.rs`

- [ ] **Step 1: Update the `AppState` struct**

In `src/server/state.rs`, find (around line 77):
```rust
    /// Shared secret token for authenticating Metacontroller webhook requests
    #[cfg(feature = "backend")]
    pub metacontroller_webhook_token: Option<String>,
```
Replace with:
```rust
    /// Source-IP validator for Metacontroller webhook requests
    #[cfg(feature = "backend")]
    pub metacontroller_ip_validator: Option<std::sync::Arc<crate::server::deployment::ip_validator::MetacontrollerIpValidator>>,
```

- [ ] **Step 2: Update the state initialization — settings extraction**

In `src/server/state.rs`, find the `if let Some(DeploymentControllerSettings::Kubernetes { ... }) = &settings.deployment_controller` block (around line 459). In the destructure list, replace:

```rust
                metacontroller_webhook_token,
```
with:
```rust
                metacontroller_pod_namespace,
                metacontroller_pod_label_selector,
```

- [ ] **Step 3: Replace the startup guard and validator construction**

Find and replace the token validation block (around lines 546–570). Remove:

```rust
                // Reject empty/blank tokens and the well-known development token in non-development environments
                if metacontroller_webhook_token.trim().is_empty() {
                    anyhow::bail!(
                        "Refusing to start: metacontroller_webhook_token is empty. \
                         Generate a secure token with: openssl rand -base64 32"
                    );
                }
                const DEV_WEBHOOK_TOKEN: &str = "dev-webhook-token-not-for-production";
                let run_mode =
                    std::env::var("RISE_CONFIG_RUN_MODE").unwrap_or_else(|_| "development".into());
                if metacontroller_webhook_token == DEV_WEBHOOK_TOKEN && run_mode != "development" {
                    anyhow::bail!(
                        "Refusing to start: metacontroller_webhook_token is set to the \
                         well-known development token. Generate a secure token with: \
                         openssl rand -base64 32"
                    );
                }

                tracing::info!("Initialized ResourceBuilder for Metacontroller webhook");
                (
                    Some(Arc::new(rb)),
                    Some(kube_client),
                    Some(metacontroller_webhook_token.clone()),
                    Some(*metacontroller_webhook_port),
                )
```

Replace with:

```rust
                let run_mode =
                    std::env::var("RISE_CONFIG_RUN_MODE").unwrap_or_else(|_| "development".into());
                let ip_validator = if metacontroller_pod_namespace.trim().is_empty() {
                    if run_mode != "development" {
                        anyhow::bail!(
                            "Refusing to start: kubernetes.metacontroller_pod_namespace is not set. \
                             In production, RISE_METACONTROLLER_POD_NAMESPACE must be set (injected \
                             automatically by the Helm chart)."
                        );
                    }
                    tracing::warn!(
                        "Metacontroller webhook running without source IP validation — \
                         acceptable for development only"
                    );
                    None
                } else {
                    Some(Arc::new(
                        crate::server::deployment::ip_validator::MetacontrollerIpValidator::new(
                            kube_client.clone(),
                            metacontroller_pod_namespace.clone(),
                            metacontroller_pod_label_selector.clone(),
                        ),
                    ))
                };

                tracing::info!("Initialized ResourceBuilder for Metacontroller webhook");
                (
                    Some(Arc::new(rb)),
                    Some(kube_client),
                    ip_validator,
                    Some(*metacontroller_webhook_port),
                )
```

- [ ] **Step 4: Update the return type signature and the final AppState construction**

The tuple return type was `(resource_builder, kube_client, token, port)`. Update the variable names — find where the tuple is destructured after the `if let` block:

```rust
        #[cfg(feature = "backend")]
        let (resource_builder, webhook_kube_client, webhook_token, webhook_port) = {
```
Rename `webhook_token` to `ip_validator`:
```rust
        #[cfg(feature = "backend")]
        let (resource_builder, webhook_kube_client, ip_validator, webhook_port) = {
```

Then in the final `AppState { ... }` construction at the bottom (around line 849):
```rust
            // Replace:
            metacontroller_webhook_token: webhook_token,
            // With:
            metacontroller_ip_validator: ip_validator,
```

- [ ] **Step 5: Check compilation**

```bash
cargo check --all-features 2>&1 | grep -v "^warning"
```

Expected: only errors in `mod.rs` and `webhook.rs` (which still reference the old field/token).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add src/server/state.rs
git commit -m "feat: swap metacontroller webhook token for IP validator in AppState"
```

---

## Task 4: Update the webhook server setup in `mod.rs`

**Files:**
- Modify: `src/server/mod.rs`

- [ ] **Step 1: Update the webhook listener gate and serve call**

In `src/server/mod.rs`, find (around line 244):

```rust
    #[cfg(feature = "backend")]
    let webhook_handle = if let (Some(port), Some(_token)) = (
        state.metacontroller_webhook_port,
        state.metacontroller_webhook_token.as_ref(),
    ) {
```

Replace with:

```rust
    #[cfg(feature = "backend")]
    let webhook_handle = if let Some(port) = state.metacontroller_webhook_port {
```

- [ ] **Step 2: Add `into_make_service_with_connect_info`**

In the same block, find:

```rust
        Some(tokio::spawn(async move {
            axum::serve(webhook_listener, webhook_app)
                .with_graceful_shutdown(shutdown_signal())
                .await
        }))
```

Replace with:

```rust
        Some(tokio::spawn(async move {
            axum::serve(
                webhook_listener,
                webhook_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_signal())
            .await
        }))
```

- [ ] **Step 3: Check compilation**

```bash
cargo check --all-features 2>&1 | grep -v "^warning"
```

Expected: only errors in `webhook.rs` (handler signatures not yet updated).

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add src/server/mod.rs
git commit -m "feat: switch webhook listener to ConnectInfo, drop token gate"
```

---

## Task 5: Update webhook handlers

**Files:**
- Modify: `src/server/deployment/webhook.rs`

- [ ] **Step 1: Remove token types and functions**

In `src/server/deployment/webhook.rs`, remove these entirely:

1. The `use subtle::ConstantTimeEq;` import
2. The `WebhookQuery` struct and its doc comment
3. The `check_webhook_token` function and its doc comment
4. The `validate_webhook_token` function and its doc comment

- [ ] **Step 2: Add the `validate_source_ip` function**

Add after the imports section:

```rust
async fn validate_source_ip(
    state: &AppState,
    addr: std::net::SocketAddr,
) -> Result<(), (StatusCode, &'static str)> {
    match &state.metacontroller_ip_validator {
        Some(validator) => validator.validate(addr).await,
        None => Ok(()),
    }
}
```

- [ ] **Step 3: Update `handle_sync` signature and auth call**

Find `handle_sync`. Change its signature from:

```rust
pub async fn handle_sync(
    State(state): State<AppState>,
    Query(query): Query<WebhookQuery>,
    Json(request): Json<SyncRequest>,
) -> Response {
    if let Err((status, msg)) = validate_webhook_token(&state, &query.token) {
```

To:

```rust
pub async fn handle_sync(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(request): Json<SyncRequest>,
) -> Response {
    if let Err((status, msg)) = validate_source_ip(&state, addr).await {
```

- [ ] **Step 4: Update `handle_finalize` signature and auth call**

Find `handle_finalize`. Apply the same change:

```rust
pub async fn handle_finalize(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(request): Json<FinalizeRequest>,
) -> Response {
    if let Err((status, msg)) = validate_source_ip(&state, addr).await {
```

- [ ] **Step 5: Update the test module**

Remove these test functions (they test the now-deleted token functions):
- `test_token_validation_no_expected_token`
- `test_token_validation_missing_provided_token`
- `test_token_validation_invalid_token`
- `test_token_validation_valid_token`
- `test_token_validation_empty_strings`

Leave the serialization tests (`test_finalize_response_*`, `test_sync_response_*`) and the `should_have_infrastructure` tests untouched.

- [ ] **Step 6: Build and test**

```bash
cargo build --all-features 2>&1 | grep -v "^warning"
cargo test --all-features
```

Expected: clean build, all tests pass.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add src/server/deployment/webhook.rs
git commit -m "feat: replace token auth with source-IP validation in webhook handlers"
```

---

## Task 6: Update config files

**Files:**
- Modify: `config/development.yaml`
- Modify: `config/production.yaml`

- [ ] **Step 1: Update `development.yaml`**

In `config/development.yaml`, find and remove:
```yaml
  metacontroller_webhook_token: "dev-webhook-token-not-for-production"
```

- [ ] **Step 2: Update `production.yaml`**

In `config/production.yaml`, find:
```yaml
  metacontroller_webhook_token: "${RISE_METACONTROLLER_WEBHOOK_TOKEN}"
```
Replace with:
```yaml
  metacontroller_pod_namespace: "${RISE_METACONTROLLER_POD_NAMESPACE}"
```

- [ ] **Step 3: Regenerate the backend settings JSON schema**

The `docs/schemas/backend-settings.schema.json` is auto-generated. Run:

```bash
mise run config:schema:generate
```

Expected: `docs/schemas/backend-settings.schema.json` is updated — `metacontroller_webhook_token` removed, `metacontroller_pod_namespace` and `metacontroller_pod_label_selector` added.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add config/development.yaml config/production.yaml docs/schemas/backend-settings.schema.json
git commit -m "feat: update config files to use metacontroller_pod_namespace"
```

---

## Task 7: Update Helm chart

**Files:**
- Modify: `helm/rise/templates/metacontroller.yaml`
- Modify: `helm/rise/values.yaml`
- Modify: `helm/rise/values-dev.yaml`
- Modify: `helm/rise/templates/deployment.yaml`

- [ ] **Step 1: Remove `$tokenParam` from `metacontroller.yaml`**

In `helm/rise/templates/metacontroller.yaml`, remove these lines entirely:

```
{{- $tokenParam := "" }}
{{- if .Values.metacontroller.webhookToken }}
{{- $tokenParam = printf "?token=%s" (.Values.metacontroller.webhookToken | urlquery) }}
{{- end }}
```

Then update the two hook URLs (they currently end with `{{ $tokenParam }}`):

```yaml
      webhook:
        url: {{ $baseUrl }}/api/v1/metacontroller/sync
```
```yaml
      webhook:
        url: {{ $baseUrl }}/api/v1/metacontroller/finalize
```

- [ ] **Step 2: Remove `webhookToken` from `values.yaml`**

In `helm/rise/values.yaml`, find and remove the `webhookToken` field and its comment block:

```yaml
  # Shared secret for webhook authentication (required for production).
  # Generate with: openssl rand -base64 32
  # Must match deployment_controller.metacontroller_webhook_token in backend config.
  # webhookToken: ""
```

While you're in this section, add a comment explaining IP validation is automatic:

```yaml
  # Webhook source-IP validation uses the Kubernetes API to verify incoming
  # requests come from a metacontroller pod. The namespace is injected
  # automatically via RISE_METACONTROLLER_POD_NAMESPACE when metacontroller.enabled: true.
```

- [ ] **Step 3: Remove `webhookToken` from `values-dev.yaml`**

In `helm/rise/values-dev.yaml`, find and remove:
```yaml
  webhookToken: "dev-webhook-token-not-for-production"
```

- [ ] **Step 4: Inject `RISE_METACONTROLLER_POD_NAMESPACE` in `deployment.yaml`**

In `helm/rise/templates/deployment.yaml`, find the `env:` block inside the server container. The block currently has `RISE_CONFIG_DIR`, `RISE_CONFIG_RUN_MODE`, and optionally `DATABASE_URL`. Add after the `RISE_CONFIG_RUN_MODE` entry:

```yaml
        {{- if .Values.metacontroller.enabled }}
        - name: RISE_METACONTROLLER_POD_NAMESPACE
          value: {{ .Values.metacontroller.install | ternary .Release.Namespace (.Values.metacontroller.namespace | default "metacontroller") | quote }}
        {{- end }}
```

- [ ] **Step 5: Lint the chart**

```bash
helm lint helm/rise
```

Expected: `1 chart(s) linted, 0 chart(s) failed`

- [ ] **Step 6: Commit**

```bash
git add helm/rise/templates/metacontroller.yaml helm/rise/values.yaml helm/rise/values-dev.yaml helm/rise/templates/deployment.yaml
git commit -m "feat: remove webhookToken from Helm chart; inject RISE_METACONTROLLER_POD_NAMESPACE"
```

---

## Task 8: Update docs

**Files:**
- Modify: `docs/kubernetes.md`

- [ ] **Step 1: Rewrite the webhook auth section**

In `docs/kubernetes.md`, find the section starting at ~line 961 ("The Metacontroller sync/finalize webhooks..."). Replace the entire section up to and including the "Bring-your-own Metacontroller" block with:

```markdown
The Metacontroller sync/finalize webhooks are served on a **separate internal port** (default: 3001). Access is secured by two independent layers:

1. **NetworkPolicy** — restricts ingress on the webhook port to pods labelled `app.kubernetes.io/name=metacontroller-operator` in the Metacontroller namespace.
2. **Source-IP validation** — on every request the Rise backend verifies the TCP source IP belongs to a live Metacontroller pod via the Kubernetes API (cached with a 15 s TTL).

No shared secret is required. The Helm chart injects `RISE_METACONTROLLER_POD_NAMESPACE` automatically, pointing Rise at the correct namespace for pod-IP lookup.

### Configuration

No additional backend config is required when deploying via the Helm chart. The namespace is derived from:
- `metacontroller.install: true` (default) → `Release.Namespace`
- `metacontroller.install: false` → `metacontroller.namespace` (default: `"metacontroller"`)

When running Rise outside the Helm chart, set the env var directly:

```bash
export RISE_METACONTROLLER_POD_NAMESPACE=metacontroller
```

Or in `config/local.yaml`:

```yaml
deployment_controller:
  type: kubernetes
  metacontroller_pod_namespace: "metacontroller"
```

### Network isolation

The Helm chart creates:
- A dedicated `ClusterIP` Service (`*-webhook`) on the webhook port
- A `NetworkPolicy` restricting ingress on the webhook port to pods in the Metacontroller namespace

### Bring-your-own Metacontroller

When you manage the Metacontroller operator yourself (`metacontroller.install: false`), set `metacontroller.namespace` to match where your operator runs:

```yaml
# Helm values
metacontroller:
  enabled: true
  install: false
  namespace: "my-metacontroller"   # Namespace where your operator runs
```
```

- [ ] **Step 2: Commit**

```bash
git add docs/kubernetes.md
git commit -m "docs: update metacontroller webhook auth section for pod-IP validation"
```

---

## Task 9: Final checks

- [ ] **Step 1: Run full lint**

```bash
cargo fmt --all
cargo clippy --all-features --all-targets -- -D warnings
```

Expected: no warnings or errors.

- [ ] **Step 2: Run all tests**

```bash
cargo test --all-features
```

Expected: all tests pass.

- [ ] **Step 3: Helm lint**

```bash
helm lint helm/rise
```

Expected: `1 chart(s) linted, 0 chart(s) failed`

- [ ] **Step 4: Verify schema is up to date**

```bash
mise run config:schema:check
```

Expected: no diff (schema was regenerated in Task 6).

- [ ] **Step 5: Final commit (formatting fixups if any)**

```bash
cargo fmt --all
git add -p   # stage any remaining fmt changes
git commit -m "chore: cargo fmt after metacontroller webhook auth refactor" --allow-empty
```
