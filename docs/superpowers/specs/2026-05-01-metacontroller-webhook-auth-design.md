# Metacontroller Webhook Authentication — Design Spec

**Date:** 2026-05-01  
**Status:** Approved

## Problem

The current approach embeds a shared secret (`webhookToken`) as a query parameter directly in the `CompositeController` resource URL:

```yaml
hooks:
  sync:
    webhook:
      url: http://rise-webhook.svc:3001/api/v1/metacontroller/sync?token=<secret>
```

This has two problems:

1. The secret lives in a CRD field, not a Kubernetes `Secret`. Anyone with `get compositecontroller` permission sees it in plaintext, and it appears in audit logs.
2. It must be supplied as a Helm value and cannot be sourced from a Kubernetes `Secret` (e.g. via ESO or VSO).

Additionally, the channel is unencrypted, which allows passive sniffing of the token on the cluster network.

## Goals

- Remove the shared secret from the `CompositeController` resource entirely
- Auth must work with stock metacontroller (no binary patches)
- Defense-in-depth: at least two independent auth layers
- TLS encryption deferred — addressed in a follow-up once the auth approach is stable

## Non-Goals

- Service mesh / Istio mTLS
- cert-manager integration (deferred follow-up)
- Patching the metacontroller binary or deployment

## Chosen Approach: Kubernetes Pod-IP Validation

Replace the token with source-IP validation backed by the Kubernetes API.

**Two independent auth layers:**

1. **NetworkPolicy** (already exists): restricts port 3001 to pods labelled `app.kubernetes.io/name=metacontroller-operator`
2. **Pod-IP validation in Rise**: on every webhook request, Rise checks that the TCP source IP belongs to a live metacontroller pod (via a TTL-cached `pods.list` call)

An attacker must bypass *both* layers for a forged request to succeed. In development, where Rise runs on the host outside the cluster, IP validation is skipped (configured via the absence of `metacontroller_pod_namespace`).

## Component Design

### `MetacontrollerIpValidator`

New module: `src/server/deployment/ip_validator.rs`

```rust
pub struct MetacontrollerIpValidator {
    kube_client: kube::Client,
    namespace: String,        // where metacontroller pods run
    label_selector: String,   // default: "app.kubernetes.io/name=metacontroller-operator"
    ttl: Duration,            // default: 15s
    cache: tokio::sync::Mutex<IpCache>,
}

struct IpCache {
    ips: HashSet<IpAddr>,
    fetched_at: Option<Instant>,
}
```

**`validate(addr: SocketAddr) -> Result<(), (StatusCode, &'static str)>`**

1. If cache is fresh (within TTL) → check `addr.ip()` against `ips`; return `Ok` or `403`
2. If stale or empty → call `refresh()`
3. If refresh succeeds → check against new set
4. If refresh fails:
   - Stale cache available → use it, log `warn!`
   - No cache at all → return `503` (fail closed)

**`refresh()`** — lists pods in `namespace` with `label_selector` via kube-rs and replaces `ips` with current `status.podIP` values.

### Settings changes (`src/server/settings.rs`)

Inside `DeploymentControllerSettings::Kubernetes`:

| Change | Detail |
|---|---|
| Remove | `metacontroller_webhook_token: String` |
| Add | `metacontroller_pod_namespace: String` (`#[serde(default)]`; empty string = validation disabled) |
| Add | `metacontroller_pod_label_selector: Option<String>` (default: `"app.kubernetes.io/name=metacontroller-operator"`) |

### `AppState` changes (`src/server/state.rs`)

| Change | Detail |
|---|---|
| Remove | `metacontroller_webhook_token: Option<String>` |
| Add | `metacontroller_ip_validator: Option<Arc<MetacontrollerIpValidator>>` |

**Startup guards** (replacing the old empty/dev-token checks):

- Webhook port set + `metacontroller_pod_namespace` absent + run mode ≠ `development` → `bail!`
- Webhook port set + `metacontroller_pod_namespace` absent + run mode = `development` → `warn!`, continue (validator is `None`)
- `metacontroller_pod_namespace` present → construct validator, enforce on every request

### Webhook server (`src/server/mod.rs`)

The listener currently requires both `metacontroller_webhook_port` and `metacontroller_webhook_token`. After this change:

- Gate changes to: `if let Some(port) = state.metacontroller_webhook_port`
- `axum::serve` gains `.into_make_service_with_connect_info::<SocketAddr>()` so handlers can read the real TCP source IP

### Webhook handlers (`src/server/deployment/webhook.rs`)

- Remove `WebhookQuery`, `check_webhook_token`, `validate_webhook_token`, and their unit tests
- Add `ConnectInfo(addr): ConnectInfo<SocketAddr>` to `handle_sync` and `handle_finalize`; remove `Query(query)`
- Add:

```rust
async fn validate_source_ip(
    state: &AppState,
    addr: SocketAddr,
) -> Result<(), (StatusCode, &'static str)> {
    match &state.metacontroller_ip_validator {
        Some(validator) => validator.validate(addr).await,
        None => Ok(()), // dev mode — no validation
    }
}
```

## Helm Chart Changes

### `metacontroller.yaml`

Remove the `$tokenParam` block entirely. Webhook URLs become:

```
{{ $baseUrl }}/api/v1/metacontroller/sync
{{ $baseUrl }}/api/v1/metacontroller/finalize
```

### `values.yaml`

- Remove `webhookToken` field and its comment
- Add comment to `metacontroller` block noting that `metacontroller_pod_namespace` is auto-injected into the Rise config when `metacontroller.enabled: true`

### `values-dev.yaml`

- Remove `webhookToken`
- No `metacontroller_pod_namespace` (its absence disables IP validation in dev)

### `deployment.yaml`

When `metacontroller.enabled`, inject `RISE_METACONTROLLER_POD_NAMESPACE` as an environment variable into the Rise server container. `config/production.yaml` reads it via `${RISE_METACONTROLLER_POD_NAMESPACE}`:

```yaml
{{- if .Values.metacontroller.enabled }}
- name: RISE_METACONTROLLER_POD_NAMESPACE
  value: {{ .Values.metacontroller.install | ternary .Release.Namespace (.Values.metacontroller.namespace | default "metacontroller") | quote }}
{{- end }}
```

### `clusterrole.yaml`

No change — `list/watch pods` is already granted cluster-wide.

## Migration Notes

Operators upgrading from the previous version must:

1. Remove `webhookToken` from their Helm values
2. Remove `metacontroller_webhook_token` from any manually managed Rise backend config
3. Ensure the Rise ClusterRole is up to date (no new permissions needed; already has pod list)

The `CompositeController` resource will be updated in-place by Helm on the next `helm upgrade`.

## Security Properties

| Threat | Mitigation |
|---|---|
| External caller with no cluster access | NetworkPolicy (port 3001 not reachable) |
| Compromised pod in cluster (wrong labels) | NetworkPolicy label selector |
| Compromised pod with correct labels | Pod-IP validation (IP must match live metacontroller pod) |
| Both NetworkPolicy bypass + correct source IP | Attacker has deep cluster compromise; scope is then broader than this endpoint |
| Traffic sniffing | Deferred — TLS follow-up |
| Token exfiltration from CRD | Eliminated — no token in CRD |
