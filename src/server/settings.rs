use config::{Config, ConfigError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct Settings {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub database: DatabaseSettings,
    #[serde(default)]
    pub controller: ControllerSettings,
    #[serde(default)]
    pub registry: Option<RegistrySettings>,
    #[serde(default)]
    pub deployment_controller: Option<DeploymentControllerSettings>,
    #[serde(default)]
    pub encryption: Option<EncryptionSettings>,
    #[serde(default)]
    pub extensions: Option<ExtensionsSettings>,
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub public_url: String,
    /// Development-only frontend proxy target (for Vite), e.g. "http://localhost:5173"
    /// When set, non-API frontend routes are proxied to this URL instead of serving embedded assets.
    #[serde(default)]
    pub frontend_dev_proxy_url: Option<String>,

    /// Cookie domain for session cookies (e.g., ".rise.dev" for all subdomains, "" for current host only)
    #[serde(default)]
    pub cookie_domain: String,

    /// Whether to set Secure flag on cookies (true for HTTPS, false for HTTP development)
    #[serde(default = "default_cookie_secure")]
    pub cookie_secure: bool,

    /// JWT signing secret for ingress authentication (base64-encoded, minimum 32 bytes)
    /// Generate with: openssl rand -base64 32
    /// Required for ingress authentication
    pub jwt_signing_secret: String,

    /// Optional RS256 private key in PEM format for JWT signing
    /// If not provided, a new key pair will be generated on startup (tokens will be invalidated on restart)
    /// To persist keys across restarts, generate with: openssl genrsa -out rs256.key 2048
    #[serde(default)]
    pub rs256_private_key_pem: Option<String>,

    /// Optional RS256 public key in PEM format for JWT verification
    /// If not provided, will be derived from rs256_private_key_pem or generated
    /// Generate from private key with: openssl rsa -in rs256.key -pubout -out rs256.pub
    #[serde(default)]
    pub rs256_public_key_pem: Option<String>,

    /// JWT claims to include from IdP token when issuing Rise JWTs
    /// Default: ["sub", "email", "name"]
    #[serde(default = "default_jwt_claims")]
    pub jwt_claims: Vec<String>,

    /// JWT token expiry duration in seconds
    /// Default: 86400 (24 hours)
    #[serde(default = "default_jwt_expiry_seconds")]
    pub jwt_expiry_seconds: u64,

    /// Directory containing static assets (Tera templates, SVGs, Vite build output).
    /// Defaults to the RISE_STATIC_DIR environment variable.
    #[serde(default = "default_static_dir")]
    pub static_dir: Option<String>,

    /// Directory to serve documentation files from (e.g., "/var/rise/docs" or "docs")
    /// Defaults to the RISE_DOCS_DIR environment variable.
    #[serde(default = "default_docs_dir")]
    pub docs_dir: Option<String>,

    /// SSRF validation configuration.
    #[serde(default)]
    pub ssrf: super::ssrf::SsrfConfig,
}

fn default_cookie_secure() -> bool {
    true
}

fn default_static_dir() -> Option<String> {
    std::env::var("RISE_STATIC_DIR").ok()
}

fn default_docs_dir() -> Option<String> {
    std::env::var("RISE_DOCS_DIR").ok()
}

fn default_jwt_claims() -> Vec<String> {
    vec!["sub".to_string(), "email".to_string(), "name".to_string()]
}

fn default_jwt_expiry_seconds() -> u64 {
    86400 // 24 hours
}

fn default_reconcile_interval() -> u64 {
    5
}

fn default_health_check_interval() -> u64 {
    5
}

fn default_termination_interval() -> u64 {
    5
}

fn default_cancellation_interval() -> u64 {
    5
}

fn default_expiration_interval() -> u64 {
    60
}

fn default_secret_refresh_interval() -> u64 {
    3600
}

fn default_idp_group_sync_enabled() -> bool {
    true
}

fn default_active_sync_interval_secs() -> u64 {
    300 // 5 minutes
}

/// Supported active sync sources for pulling users and groups
#[derive(Debug, Deserialize, Clone, JsonSchema, PartialEq)]
pub enum ActiveSyncSource {
    /// Microsoft Entra ID (Azure AD) - uses Microsoft Graph API to pull
    /// users and groups assigned to the configured app registration.
    Entra,
}

fn default_allow_team_creation() -> bool {
    true // Backward compatible - existing behavior
}

fn default_allow_list_all_teams() -> bool {
    false // Backward compatible - non-admins only see their own teams by default
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct ControllerSettings {
    /// Interval in seconds for checking deployments to reconcile (default: 5)
    #[serde(default = "default_reconcile_interval")]
    pub reconcile_interval_secs: u64,

    /// Interval in seconds for health checks on active deployments (default: 5)
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u64,

    /// Interval in seconds for processing terminating deployments (default: 5)
    #[serde(default = "default_termination_interval")]
    pub termination_interval_secs: u64,

    /// Interval in seconds for processing cancelling deployments (default: 5)
    #[serde(default = "default_cancellation_interval")]
    pub cancellation_interval_secs: u64,

    /// Interval in seconds for checking expired deployments (default: 60)
    #[serde(default = "default_expiration_interval")]
    pub expiration_interval_secs: u64,

    /// Interval in seconds for refreshing Kubernetes image pull secrets (default: 3600)
    #[serde(default = "default_secret_refresh_interval")]
    #[allow(dead_code)]
    pub secret_refresh_interval_secs: u64,
}

impl Default for ControllerSettings {
    fn default() -> Self {
        Self {
            reconcile_interval_secs: default_reconcile_interval(),
            health_check_interval_secs: default_health_check_interval(),
            termination_interval_secs: default_termination_interval(),
            cancellation_interval_secs: default_cancellation_interval(),
            expiration_interval_secs: default_expiration_interval(),
            secret_refresh_interval_secs: default_secret_refresh_interval(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct AuthSettings {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// List of admin user emails (have full permissions)
    #[serde(default)]
    pub admin_users: Vec<String>,
    /// Platform access control configuration
    #[serde(default)]
    pub platform_access: PlatformAccessConfig,
    /// Allow regular users to create teams (default: true).
    /// When false, only admin users can create teams.
    #[serde(default = "default_allow_team_creation")]
    pub allow_team_creation: bool,
    /// Allow all users to list all teams (default: false).
    /// When false, non-admin users only see teams they are members of.
    #[serde(default = "default_allow_list_all_teams")]
    pub allow_list_all_teams: bool,
    /// Optional custom authorize endpoint URL
    /// If not set, will be discovered from issuer's .well-known/openid-configuration
    /// or default to {issuer}/authorize
    #[serde(default)]
    pub authorize_url: Option<String>,
    /// Optional custom token endpoint URL
    /// If not set, will be discovered from issuer's .well-known/openid-configuration
    /// or default to {issuer}/token
    #[serde(default)]
    pub token_url: Option<String>,
    /// Enable IdP group synchronization (default: true)
    /// When enabled, user team memberships are automatically synced from IdP groups claim on login
    #[serde(default = "default_idp_group_sync_enabled")]
    pub idp_group_sync_enabled: bool,
    /// Optional active sync source for pulling users and groups from an external IdP.
    /// When configured, Rise will periodically query the IdP for users and groups
    /// assigned to the app and sync them as Rise teams.
    #[serde(default)]
    pub active_sync_source: Option<ActiveSyncSource>,
    /// Interval in seconds for active sync polling (default: 300 = 5 minutes)
    #[serde(default = "default_active_sync_interval_secs")]
    pub active_sync_interval_secs: u64,
}

#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct DatabaseSettings {
    #[serde(default)]
    pub url: String,
}

fn default_repo_prefix() -> String {
    "rise/".to_string()
}

fn default_ingress_schema() -> String {
    "https".to_string()
}

fn default_namespace_format() -> String {
    "rise-{project_name}".to_string()
}

fn default_node_selector() -> std::collections::HashMap<String, String> {
    let mut selector = std::collections::HashMap::new();
    selector.insert("kubernetes.io/arch".to_string(), "amd64".to_string());
    selector
}

/// Backend address for routing /.rise/* traffic to the Rise backend
#[derive(Debug, Clone)]
pub struct BackendAddress {
    pub host: String,
    pub port: u16,
}

impl BackendAddress {
    /// Parse backend address from a URL by extracting host and port
    /// Example: "http://172.17.0.1:3000" -> BackendAddress { host: "172.17.0.1", port: 3000 }
    pub fn from_url(url: &str) -> Result<Self, anyhow::Error> {
        let parsed = url::Url::parse(url)
            .map_err(|e| anyhow::anyhow!("Invalid URL for backend address: {}", e))?;

        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("URL missing host"))?
            .to_string();

        let port = parsed
            .port()
            .or_else(|| {
                // Default ports based on scheme
                match parsed.scheme() {
                    "http" => Some(80),
                    "https" => Some(443),
                    _ => None,
                }
            })
            .ok_or_else(|| anyhow::anyhow!("URL missing port and no default for scheme"))?;

        Ok(Self { host, port })
    }

    /// Check if the host is an IP address (vs a DNS name)
    pub fn is_ip_address(&self) -> bool {
        self.host.parse::<std::net::IpAddr>().is_ok()
    }
}

/// TLS mode for custom domains
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CustomDomainTlsMode {
    /// All hosts (primary + custom domains) share the same TLS secret
    Shared,
    /// Each custom domain gets its own tls-{domain} secret (cert-manager integration)
    PerDomain,
}

fn default_custom_domain_tls_mode() -> CustomDomainTlsMode {
    CustomDomainTlsMode::PerDomain
}

/// Access requirement level for project ingress
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum AccessRequirement {
    /// No authentication required - fully public access
    None,
    /// Must be authenticated, but no project membership required
    Authenticated,
    /// Must be authenticated AND have project membership (owner or team member)
    Member,
}

/// Access class configuration for ingress authentication
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AccessClass {
    /// Display name for UI (e.g., "Public")
    pub display_name: String,

    /// Description for UI
    pub description: String,

    /// Ingress class to use
    pub ingress_class: String,

    /// Access requirement level
    pub access_requirement: AccessRequirement,

    /// Optional custom nginx annotations
    #[serde(default)]
    pub custom_annotations: std::collections::HashMap<String, String>,
}

/// Resource limits for pods
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PodResourceLimits {
    /// CPU request (e.g., "100m", "500m", "1")
    #[serde(default = "default_cpu_request")]
    pub cpu_request: String,

    /// Memory request (e.g., "128Mi", "256Mi", "1Gi")
    #[serde(default = "default_memory_request")]
    pub memory_request: String,

    /// CPU limit (e.g., "1", "2", "4")
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit: String,

    /// Memory limit (e.g., "512Mi", "1Gi", "2Gi")
    #[serde(default = "default_memory_limit")]
    pub memory_limit: String,
}

impl Default for PodResourceLimits {
    fn default() -> Self {
        Self {
            cpu_request: default_cpu_request(),
            memory_request: default_memory_request(),
            cpu_limit: default_cpu_limit(),
            memory_limit: default_memory_limit(),
        }
    }
}

/// Health probe configuration
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HealthProbeConfig {
    /// Enable liveness probes (default: true)
    #[serde(default = "default_true")]
    pub liveness_enabled: bool,

    /// Enable readiness probes (default: true)
    #[serde(default = "default_true")]
    pub readiness_enabled: bool,

    /// Path for HTTP probes (default: "/")
    #[serde(default = "default_probe_path")]
    pub path: String,

    /// Initial delay in seconds (default: 10)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_seconds: i32,

    /// Period in seconds (default: 10)
    #[serde(default = "default_period_seconds")]
    pub period_seconds: i32,

    /// Timeout in seconds (default: 5)
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: i32,

    /// Failure threshold (default: 3)
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: i32,
}

/// NetworkPolicy configuration for deployed apps
///
/// Uses Kubernetes NetworkPolicy types directly. Egress semantics:
/// - null: policyTypes is ["Ingress"] only, Kubernetes does not restrict egress
/// - Empty list: policyTypes includes "Egress" with no rules = deny all egress
/// - Non-empty list: explicit egress rules enforced
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct NetworkPolicyConfig {
    /// Ingress rules
    pub ingress: Vec<k8s_openapi::api::networking::v1::NetworkPolicyIngressRule>,
    /// Egress rules (null = unrestricted egress)
    pub egress: Option<Vec<k8s_openapi::api::networking::v1::NetworkPolicyEgressRule>>,
}

// Default functions for pod security settings
fn default_pod_security_enabled() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_cpu_request() -> String {
    "500m".to_string()
}

fn default_memory_request() -> String {
    "256Mi".to_string()
}

fn default_cpu_limit() -> String {
    "2".to_string()
}

fn default_memory_limit() -> String {
    "2Gi".to_string()
}

fn default_probe_path() -> String {
    "/".to_string()
}

fn default_initial_delay() -> i32 {
    10
}

fn default_period_seconds() -> i32 {
    10
}

fn default_timeout_seconds() -> i32 {
    5
}

fn default_failure_threshold() -> i32 {
    3
}

/// Deployment controller configuration
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum DeploymentControllerSettings {
    /// Kubernetes deployment controller
    #[cfg(feature = "backend")]
    Kubernetes {
        /// Optional kubeconfig path (defaults to in-cluster or ~/.kube/config)
        #[serde(default)]
        kubeconfig: Option<String>,

        /// Ingress URL template for production (default) deployment group
        /// Supports both subdomain and sub-path routing:
        ///   Subdomain: "{project_name}.apps.rise.dev"
        ///   Sub-path: "rise.dev/{project_name}"
        /// Must contain {project_name} placeholder
        production_ingress_url_template: String,

        /// Ingress URL template for staging (non-default) deployment groups
        /// Supports both subdomain and sub-path routing:
        ///   Subdomain: "{project_name}-{deployment_group}.preview.rise.dev"
        ///   Sub-path: "rise.dev/{project_name}/{deployment_group}"
        /// Must contain both {project_name} and {deployment_group} placeholders
        /// If not set, falls back to inserting "-{deployment_group}" before first dot
        #[serde(default)]
        staging_ingress_url_template: Option<String>,

        /// Optional port number to append to all generated ingress URLs
        /// Used for development environments with port-forwarding (e.g., kubectl port-forward)
        /// Example: 8080 → "https://myapp.apps.rise.local:8080"
        /// If not set, URLs use standard ports (80 for HTTP, 443 for HTTPS)
        #[serde(default)]
        ingress_port: Option<u16>,

        /// URL scheme for generated ingress URLs
        /// Used to specify whether URLs should use "http" or "https"
        /// Example: "http" → "http://myapp.apps.rise.local"
        /// Defaults to "https"
        #[serde(default = "default_ingress_schema")]
        ingress_schema: String,

        /// Backend URL for Nginx auth subrequests (internal cluster URL)
        /// Example: "http://rise-backend.default.svc.cluster.local:3000"
        /// This is the URL Nginx will use internally within the cluster to validate authentication.
        /// For Minikube development, use the Docker bridge IP to reach host (e.g., "http://host.minikube.internal:3000").
        auth_backend_url: String,

        /// Public backend URL for browser redirects during authentication
        /// Example: "https://rise.dev"
        /// This must be the public URL where the backend is accessible via Ingress.
        /// The domain should share a parent with app domains for cookie sharing (see struct docs).
        auth_signin_url: String,

        /// Namespace format template for deployed applications
        /// Template variables: {project_name}
        /// Example: "rise-{project_name}" → namespace "rise-myapp" for project "myapp"
        /// Defaults to "rise-{project_name}"
        #[serde(default = "default_namespace_format")]
        namespace_format: String,

        /// Labels to apply to all managed namespaces
        /// Example: {"environment": "production", "team": "platform"}
        #[serde(default)]
        namespace_labels: std::collections::HashMap<String, String>,

        /// Annotations to apply to all managed namespaces
        /// Example: {"company.com/team": "platform", "cost-center": "engineering"}
        #[serde(default)]
        namespace_annotations: std::collections::HashMap<String, String>,

        /// Ingress annotations to apply to all deployed application ingresses
        /// These apply to both primary ingresses and custom domain ingresses
        /// For annotations specific to custom domains only, use custom_domain_ingress_annotations
        /// Example: {"nginx.ingress.kubernetes.io/proxy-body-size": "10m"}
        #[serde(default)]
        ingress_annotations: std::collections::HashMap<String, String>,

        /// TLS secret name for primary ingress certificates
        /// If set, enables TLS on primary ingresses with this secret
        /// For custom domain TLS, see custom_domain_tls_mode and custom_domain_ingress_annotations
        /// Example: "rise-apps-tls" (secret must exist in each namespace)
        #[serde(default)]
        ingress_tls_secret_name: Option<String>,

        /// TLS mode for custom domains
        /// - "shared": All custom domains share ingress_tls_secret_name (requires it to be set)
        /// - "per-domain": Each custom domain gets its own tls-{domain} secret
        ///   (works with cert-manager when custom_domain_ingress_annotations are configured)
        ///
        /// Defaults to "per-domain"
        #[serde(default = "default_custom_domain_tls_mode")]
        custom_domain_tls_mode: CustomDomainTlsMode,

        /// Annotations to apply ONLY to custom domain ingresses (not primary ingresses)
        /// Use this for cert-manager integration or other custom domain-specific configuration
        /// Example: {"cert-manager.io/cluster-issuer": "letsencrypt-prod"}
        #[serde(default)]
        custom_domain_ingress_annotations: std::collections::HashMap<String, String>,

        /// Node selector for pod placement (controls which nodes pods can run on)
        /// Default: {"kubernetes.io/arch": "amd64"}
        /// Example: {"kubernetes.io/arch": "amd64", "node-type": "compute"}
        #[serde(default = "default_node_selector")]
        node_selector: std::collections::HashMap<String, String>,

        /// Optional name of an existing imagePullSecret to use for deployments
        ///
        /// If not specified:
        ///   - With a registry provider (e.g., ECR): The controller creates and manages the secret
        ///   - Without a registry provider: No image pull secret is used
        ///
        /// If specified:
        ///   - The named secret must exist in each project namespace
        ///   - The controller will NOT create or manage the secret
        ///   - Useful for static registries where credentials are managed externally
        ///
        /// Example: "my-registry-secret"
        #[serde(default)]
        image_pull_secret_name: Option<String>,

        /// Access classes defining ingress authentication levels
        /// Key: access class identifier (e.g., "public", "private")
        /// Value: access class configuration (display info, ingress settings)
        /// Use `null` in YAML to remove an inherited access class from parent configs
        access_classes: std::collections::HashMap<String, Option<AccessClass>>,

        /// Host aliases to inject into pod specs (hostname -> IP address)
        /// Maps hostnames to IP addresses, injected as Kubernetes hostAliases.
        /// Useful for local development where pods need to resolve custom hostnames.
        /// Example: {"rise.local": "192.168.49.1"}
        #[serde(default)]
        host_aliases: std::collections::HashMap<String, String>,

        /// Extra projected service account tokens to mount into every deployed app pod.
        /// Key becomes the in-pod filename under /var/run/secrets/rise/tokens/, value is the audience.
        /// Example: {"vault": "https://vault.example.com"}
        #[serde(default)]
        extra_service_token_audiences: std::collections::HashMap<String, String>,

        /// NetworkPolicy configuration for deployed apps
        network_policy: NetworkPolicyConfig,

        /// Pod security settings (enabled by default)
        /// Set to false to disable security context enforcement
        #[serde(default = "default_pod_security_enabled")]
        pod_security_enabled: bool,

        /// Resource limits for deployed containers
        /// If not set, uses defaults: 500m CPU request, 256Mi memory request, 2 CPU limit, 2Gi memory limit
        #[serde(default)]
        pod_resources: Option<PodResourceLimits>,

        /// Health probe configuration
        /// If not set, uses defaults (HTTP probes on app port at "/" path)
        #[serde(default)]
        health_probes: Option<HealthProbeConfig>,
    },
}

/// Registry provider configuration
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RegistrySettings {
    Ecr {
        #[allow(dead_code)]
        region: String,
        account_id: String,
        /// Literal prefix for ECR repository names (e.g., "rise/" → repos named "rise/{project}")
        #[serde(default = "default_repo_prefix")]
        #[allow(dead_code)]
        repo_prefix: String,
        /// IAM role ARN for push operations (assumed to generate scoped credentials)
        #[allow(dead_code)]
        push_role_arn: String,
        /// Whether to automatically delete ECR repos when projects are deleted
        #[serde(default)]
        #[allow(dead_code)]
        auto_remove: bool,
        #[serde(default)]
        #[allow(dead_code)]
        access_key_id: Option<String>,
        #[serde(default)]
        #[allow(dead_code)]
        secret_access_key: Option<String>,
    },
    #[serde(rename = "oci-client-auth")]
    OciClientAuth {
        registry_url: String,
        #[serde(default)]
        namespace: String,
        /// Optional client-facing registry URL for CLI push operations
        /// If not specified, defaults to registry_url
        #[serde(default)]
        client_registry_url: Option<String>,
    },
    /// GitLab container registry — mints scoped JWTs per deployment
    #[serde(rename = "gitlab")]
    GitLab {
        /// GitLab instance URL (e.g., "https://gitlab.com")
        gitlab_url: String,
        /// Registry URL (e.g., "registry.gitlab.com")
        registry_url: String,
        /// Full image path prefix within the registry
        /// (e.g., "my-org/my-project" or "my-org/my-project/rise-apps")
        namespace: String,
        /// GitLab username for JWT auth endpoint
        username: String,
        /// Personal Access Token or Deploy Token
        token: String,
        /// When true, the Kubernetes controller creates and manages image pull secrets
        /// in each project namespace. Set to false if the cluster has its own pull mechanism.
        #[serde(default)]
        mint_pull_secrets: bool,
        /// Optional client-facing registry URL override
        #[serde(default)]
        client_registry_url: Option<String>,
    },
}

/// Encryption provider configuration
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EncryptionSettings {
    /// Local AES-256-GCM encryption using a symmetric key
    #[serde(rename = "aes-gcm-256")]
    Local {
        /// Base64-encoded 32-byte encryption key
        /// Generate with: openssl rand -base64 32
        key: String,
    },
    /// AWS KMS encryption
    #[serde(rename = "aws-kms")]
    AwsKms {
        #[allow(dead_code)]
        region: String,
        /// KMS key ID or ARN
        key_id: String,
        /// Optional static credentials (development only)
        #[serde(default)]
        #[allow(dead_code)]
        access_key_id: Option<String>,
        /// Optional static credentials (development only)
        #[serde(default)]
        #[allow(dead_code)]
        secret_access_key: Option<String>,
    },
}

impl Settings {
    pub fn json_schema_value() -> serde_json::Value {
        let schema = schemars::schema_for!(Settings);
        schema.to_value()
    }

    fn substitute_env_vars_in_string_with(
        s: &str,
        env_lookup: &impl Fn(&str) -> Option<String>,
    ) -> String {
        let re = regex::Regex::new(r"\$\{([^}:]+)(?::-([^}]*))?\}").unwrap();

        re.replace_all(s, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default_value = caps.get(2).map(|m| m.as_str());

            env_lookup(var_name).unwrap_or_else(|| default_value.unwrap_or("").to_string())
        })
        .to_string()
    }

    fn config_value_to_json_with(
        value: &config::Value,
        env_lookup: &impl Fn(&str) -> Option<String>,
    ) -> serde_json::Value {
        use config::ValueKind;

        match &value.kind {
            ValueKind::Nil => serde_json::Value::Null,
            ValueKind::Boolean(b) => serde_json::Value::Bool(*b),
            ValueKind::I64(i) => serde_json::Value::Number((*i).into()),
            ValueKind::I128(i) => serde_json::Value::Number((*i as i64).into()),
            ValueKind::U64(u) => serde_json::Value::Number((*u).into()),
            ValueKind::U128(u) => serde_json::Value::Number((*u as u64).into()),
            ValueKind::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            ValueKind::String(s) => {
                // Perform environment variable substitution
                serde_json::Value::String(Self::substitute_env_vars_in_string_with(s, env_lookup))
            }
            ValueKind::Table(table) => {
                let mut map = serde_json::Map::new();
                for (k, v) in table.iter() {
                    map.insert(k.clone(), Self::config_value_to_json_with(v, env_lookup));
                }
                serde_json::Value::Object(map)
            }
            ValueKind::Array(arr) => {
                let vec: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|value| Self::config_value_to_json_with(value, env_lookup))
                    .collect();
                serde_json::Value::Array(vec)
            }
        }
    }

    /// Try to add a config file with multiple extension attempts (.toml, .yaml, .yml)
    /// Returns Ok(true) if a file was loaded, Ok(false) if no file found (when not required)
    fn try_add_config_file(
        builder: &mut config::ConfigBuilder<config::builder::DefaultState>,
        config_dir: &str,
        name: &str,
        required: bool,
    ) -> Result<bool, ConfigError> {
        // Try extensions in order of preference
        let extensions = ["toml", "yaml", "yml"];

        for ext in extensions {
            let path = format!("{}/{}.{}", config_dir, name, ext);
            if std::path::Path::new(&path).exists() {
                tracing::info!("Loading config file: {}", path);
                *builder = builder
                    .clone()
                    .add_source(config::File::with_name(&format!("{}/{}", config_dir, name)));
                return Ok(true);
            }
        }

        if required {
            Err(ConfigError::Message(format!(
                "Required config file not found: {}/{}.{{toml,yaml,yml}}",
                config_dir, name
            )))
        } else {
            tracing::debug!(
                "Optional config file not found: {}/{}.{{toml,yaml,yml}}",
                config_dir,
                name
            );
            Ok(false)
        }
    }

    fn validate_extra_service_token_audiences(
        audiences: &std::collections::HashMap<String, String>,
    ) -> Result<(), ConfigError> {
        let token_name_re = regex::Regex::new(r"^[A-Za-z0-9._-]+$").map_err(|e| {
            ConfigError::Message(format!("Failed to compile token name regex: {}", e))
        })?;

        for (name, audience) in audiences {
            if name.is_empty() {
                return Err(ConfigError::Message(
                    "extra_service_token_audiences contains an empty token name".to_string(),
                ));
            }

            if name == "." || name == ".." || !token_name_re.is_match(name) {
                return Err(ConfigError::Message(format!(
                    "extra_service_token_audiences token name '{}' is invalid; use only letters, numbers, '.', '_' or '-'",
                    name
                )));
            }

            if audience.trim().is_empty() {
                return Err(ConfigError::Message(format!(
                    "extra_service_token_audiences token '{}' has an empty audience",
                    name
                )));
            }
        }

        Ok(())
    }

    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("RISE_CONFIG_RUN_MODE").unwrap_or_else(|_| "development".into());
        let config_dir = env::var("RISE_CONFIG_DIR").unwrap_or_else(|_| "config".into());

        Self::new_with_env(&config_dir, &run_mode, &|name| env::var(name).ok())
    }

    fn new_with_env(
        config_dir: &str,
        run_mode: &str,
        env_lookup: &impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let mut builder = Config::builder();

        // Load config files in order, trying both .toml and .yaml/.yml extensions.
        // TOML takes precedence if both exist.

        // 1. Load environment-specific config (required)
        Self::try_add_config_file(&mut builder, config_dir, run_mode, true)?;

        // 2. Load local config (optional, not checked into git)
        Self::try_add_config_file(&mut builder, config_dir, "local", false)?;

        // Build config and substitute environment variables
        let config = builder.build()?;

        // Get the root value and convert to JSON with env var substitution
        let root_value = config
            .cache
            .into_table()
            .map_err(|e| ConfigError::Message(format!("Failed to get config table: {}", e)))?;

        // Convert config values to serde_json::Value (with env var substitution in strings)
        let mut json_map = serde_json::Map::new();
        for (k, v) in root_value.iter() {
            json_map.insert(k.clone(), Self::config_value_to_json_with(v, env_lookup));
        }
        let json_value = serde_json::Value::Object(json_map);

        // Deserialize from JSON value and collect unused fields
        let mut unused_fields = Vec::new();
        let mut settings: Settings = serde_ignored::deserialize(json_value, |path| {
            unused_fields.push(path.to_string());
        })
        .map_err(|e| ConfigError::Message(format!("Failed to deserialize settings: {}", e)))?;

        // Warn about unused fields
        for field in &unused_fields {
            tracing::warn!("Unknown configuration field in backend config: {}", field);
        }

        // Special handling for DATABASE_URL environment variable (common convention)
        // This takes precedence over both TOML config and RISE_DATABASE__URL
        if let Some(database_url) = env_lookup("DATABASE_URL") {
            if !database_url.is_empty() {
                settings.database.url = database_url;
            }
        }

        // Validate that database URL is set
        if settings.database.url.is_empty() {
            return Err(ConfigError::Message(
                "Database URL not configured. Set DATABASE_URL environment variable or [database] url in config".to_string()
            ));
        }

        // Validate that JWT signing secret is set and valid
        if settings.server.jwt_signing_secret.is_empty() {
            return Err(ConfigError::Message(
                "JWT signing secret not configured. Set RISE_SERVER__JWT_SIGNING_SECRET environment variable or [server] jwt_signing_secret in config. Generate with: openssl rand -base64 32".to_string()
            ));
        }

        // Validate deployment controller settings if configured
        if let Some(DeploymentControllerSettings::Kubernetes {
            ref namespace_format,
            ref production_ingress_url_template,
            ref staging_ingress_url_template,
            ref access_classes,
            ref extra_service_token_audiences,
            ..
        }) = settings.deployment_controller
        {
            Self::validate_format_string(namespace_format, "namespace_format", "{project_name}")?;
            Self::validate_format_string(
                production_ingress_url_template,
                "production_ingress_url_template",
                "{project_name}",
            )?;

            if let Some(ref staging_template) = staging_ingress_url_template {
                Self::validate_format_string(
                    staging_template,
                    "staging_ingress_url_template",
                    "{project_name}",
                )?;
                Self::validate_format_string(
                    staging_template,
                    "staging_ingress_url_template",
                    "{deployment_group}",
                )?;
            }

            Self::validate_extra_service_token_audiences(extra_service_token_audiences)?;

            // Filter out null access classes (used to remove inherited entries)
            // and validate the remaining ones
            let active_classes: Vec<_> = access_classes
                .iter()
                .filter_map(|(id, class)| class.as_ref().map(|c| (id, c)))
                .collect();

            if active_classes.is_empty() {
                return Err(ConfigError::Message(
                    "Kubernetes deployment_controller requires at least one access class to be configured. \
                     Add access_classes to your configuration file.".to_string()
                ));
            }

            for (id, class) in active_classes {
                if class.display_name.is_empty() {
                    return Err(ConfigError::Message(format!(
                        "Access class '{}' has empty display_name",
                        id
                    )));
                }
                if class.description.is_empty() {
                    return Err(ConfigError::Message(format!(
                        "Access class '{}' has empty description",
                        id
                    )));
                }
                if class.ingress_class.is_empty() {
                    return Err(ConfigError::Message(format!(
                        "Access class '{}' has empty ingress_class",
                        id
                    )));
                }
            }
        }

        Ok(settings)
    }

    /// Validate that a format string contains the required placeholder
    fn validate_format_string(
        format_str: &str,
        field_name: &str,
        required_placeholder: &str,
    ) -> Result<(), ConfigError> {
        if !format_str.contains(required_placeholder) {
            return Err(ConfigError::Message(format!(
                "Kubernetes configuration error: '{}' must contain '{}' placeholder. Got: '{}'",
                field_name, required_placeholder, format_str
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_env_vars_in_string_basic() {
        let result = Settings::substitute_env_vars_in_string_with("${TEST_VAR}", &|name| {
            (name == "TEST_VAR").then(|| "test_value".to_string())
        });
        assert_eq!(result, "test_value");
    }

    #[test]
    fn test_substitute_env_vars_in_string_with_default() {
        let result =
            Settings::substitute_env_vars_in_string_with("${MISSING_VAR:-default_value}", &|_| {
                None
            });
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_substitute_env_vars_in_string_override_default() {
        let result = Settings::substitute_env_vars_in_string_with(
            "${OVERRIDE_VAR:-default_value}",
            &|name| (name == "OVERRIDE_VAR").then(|| "actual_value".to_string()),
        );
        assert_eq!(result, "actual_value");
    }

    #[test]
    fn test_substitute_env_vars_in_string_multiple() {
        let result =
            Settings::substitute_env_vars_in_string_with(
                "${VAR1} and ${VAR2}",
                &|name| match name {
                    "VAR1" => Some("value1".to_string()),
                    "VAR2" => Some("value2".to_string()),
                    _ => None,
                },
            );
        assert_eq!(result, "value1 and value2");
    }

    #[test]
    fn test_substitute_env_vars_in_string_no_substitution() {
        let result = Settings::substitute_env_vars_in_string_with("plain_value", &|_| None);
        assert_eq!(result, "plain_value");
    }

    #[test]
    fn test_unused_fields_warning() {
        // This test verifies that unused fields are detected during deserialization
        // We can't easily test the warning output itself, but we can verify the config
        // still loads successfully even with unknown fields

        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory with a development.yaml config
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("development.yaml");

        fs::write(
            &config_path,
            r#"
server:
  host: "0.0.0.0"
  port: 3000
  public_url: "http://localhost:3000"
  jwt_signing_secret: "test-secret-key-for-testing-123456"
  unknown_field: "should trigger warning"

database:
  url: "postgres://test@localhost/test"

auth:
  issuer: "http://localhost:5556"
  client_id: "test"
  client_secret: "test"

deployment_controller:
  type: "kubernetes"
  ingress_class: "nginx"
  production_ingress_url_template: "{project_name}.test.local"
  namespace_format: "rise-{project_name}"
  auth_backend_url: "http://localhost:3000"
  auth_signin_url: "http://localhost:3000"
  network_policy:
    ingress:
      - from:
          - podSelector:
              matchLabels: {}
    egress: null
  access_classes:
    public:
      display_name: "Public"
      description: "Test public access"
      ingress_class: "nginx"
      access_requirement: None

unknown_top_level: "also unknown"
"#,
        )
        .unwrap();

        // This should load successfully despite unknown fields
        // (The warnings would appear in logs)
        let result =
            Settings::new_with_env(temp_dir.path().to_str().unwrap(), "development", &|_| None);

        // Config should load successfully (warnings are logged, not errors)
        assert!(
            result.is_ok(),
            "Config should load despite unknown fields: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_validate_extra_service_token_audiences_accepts_empty_map() {
        let audiences = std::collections::HashMap::new();
        assert!(Settings::validate_extra_service_token_audiences(&audiences).is_ok());
    }

    #[test]
    fn test_validate_extra_service_token_audiences_rejects_invalid_name() {
        let mut audiences = std::collections::HashMap::new();
        audiences.insert(
            "vault/token".to_string(),
            "https://vault.example.com".to_string(),
        );

        let error = Settings::validate_extra_service_token_audiences(&audiences)
            .expect_err("invalid token name should fail validation");

        assert!(error
            .to_string()
            .contains("extra_service_token_audiences token name 'vault/token' is invalid"));
    }

    #[test]
    fn test_settings_load_with_extra_service_token_audiences() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("development.yaml");

        fs::write(
            &config_path,
            r#"
server:
  host: "0.0.0.0"
  port: 3000
  public_url: "http://localhost:3000"
  jwt_signing_secret: "test-secret-key-for-testing-123456"

database:
  url: "postgres://test@localhost/test"

auth:
  issuer: "http://localhost:5556"
  client_id: "test"
  client_secret: "test"

deployment_controller:
  type: "kubernetes"
  production_ingress_url_template: "{project_name}.test.local"
  namespace_format: "rise-{project_name}"
  auth_backend_url: "http://localhost:3000"
  auth_signin_url: "http://localhost:3000"
  extra_service_token_audiences:
    vault: "https://vault.example.com"
  network_policy:
    ingress:
      - from:
          - podSelector:
              matchLabels: {}
    egress: null
  access_classes:
    public:
      display_name: "Public"
      description: "Test public access"
      ingress_class: "nginx"
      access_requirement: None
"#,
        )
        .unwrap();

        let result =
            Settings::new_with_env(temp_dir.path().to_str().unwrap(), "development", &|_| None);

        let settings = result.expect("config with extra service token audiences should load");
        let Some(DeploymentControllerSettings::Kubernetes {
            extra_service_token_audiences,
            ..
        }) = settings.deployment_controller
        else {
            panic!("expected kubernetes deployment_controller");
        };

        assert_eq!(
            extra_service_token_audiences.get("vault"),
            Some(&"https://vault.example.com".to_string())
        );
    }

    #[test]
    fn test_run_mode_config_is_required_even_if_local_exists() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let local_path = temp_dir.path().join("local.yaml");

        fs::write(
            &local_path,
            r#"
server:
  host: "0.0.0.0"
  port: 3000
  public_url: "http://localhost:3000"
  jwt_signing_secret: "test-secret-key-for-testing-123456"

database:
  url: "postgres://test@localhost/test"

auth:
  issuer: "http://localhost:5556"
  client_id: "test"
  client_secret: "test"
"#,
        )
        .unwrap();

        let result =
            Settings::new_with_env(temp_dir.path().to_str().unwrap(), "production", &|_| None);

        assert!(
            result.is_err(),
            "Config should fail without required run_mode file"
        );
    }
}

/// Extensions configuration
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ExtensionsSettings {
    #[serde(default)]
    pub providers: Vec<ExtensionProviderConfig>,
}

/// Snowflake authentication configuration
#[cfg(feature = "backend")]
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "auth_type", rename_all = "snake_case")]
pub enum SnowflakeAuth {
    Password {
        password: String,
    },
    PrivateKey {
        #[serde(flatten)]
        key_source: PrivateKeySource,
        #[serde(default)]
        private_key_password: Option<String>,
    },
}

/// Private key source (path or inline PEM)
#[cfg(feature = "backend")]
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PrivateKeySource {
    Path { private_key_path: String },
    Inline { private_key: String },
}

/// Extension provider configuration
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ExtensionProviderConfig {
    #[cfg(feature = "backend")]
    AwsRdsProvisioner {
        region: String,
        instance_size: String,
        disk_size: i32, // in GiB
        /// Template for RDS instance identifiers
        /// Available placeholders: {prefix}, {project_name}, {extension_name}
        /// Default: "{prefix}-{project_name}-{extension_name}"
        #[serde(default = "default_instance_id_template")]
        instance_id_template: String,
        /// Prefix for RDS instance identifiers
        /// Must match the IAM policy prefix configured in your Terraform infrastructure
        /// Default: "rise"
        #[serde(default = "default_instance_id_prefix")]
        instance_id_prefix: String,
        /// Default engine version to use if not specified in project extension spec
        /// Use AWS CLI to find versions: aws rds describe-db-engine-versions --engine postgres --query "DBEngineVersions[*].EngineVersion"
        #[serde(default = "default_engine_version")]
        default_engine_version: String,
        /// VPC security group IDs for the RDS instance
        #[serde(default)]
        vpc_security_group_ids: Option<Vec<String>>,
        /// DB subnet group name for VPC placement
        #[serde(default)]
        db_subnet_group_name: Option<String>,
        /// Backup retention period in days (1-35, default: 7)
        #[serde(default = "default_backup_retention_days")]
        backup_retention_days: i32,
        /// Preferred backup window in UTC (e.g., "03:00-04:00")
        #[serde(default)]
        backup_window: Option<String>,
        /// Preferred maintenance window (e.g., "sun:04:00-sun:05:00")
        #[serde(default)]
        maintenance_window: Option<String>,
        #[serde(default)]
        access_key_id: Option<String>,
        #[serde(default)]
        secret_access_key: Option<String>,
    },

    #[cfg(feature = "backend")]
    #[serde(rename = "snowflake-oauth-provisioner")]
    SnowflakeOAuthProvisioner {
        /// Snowflake account identifier (e.g., "myorg.us-east-1")
        account: String,
        /// Snowflake user with CREATE INTEGRATION privilege
        user: String,
        /// Snowflake role to use (must have CREATE INTEGRATION ON ACCOUNT privilege)
        /// Typically ACCOUNTADMIN or a custom role with appropriate grants
        #[serde(default)]
        role: Option<String>,
        /// Snowflake warehouse to use for queries
        /// Required for executing SQL statements
        #[serde(default)]
        warehouse: Option<String>,
        /// Authentication configuration (password, private key, or JWT)
        #[serde(flatten)]
        auth: SnowflakeAuth,
        /// Prefix for SECURITY INTEGRATION names (default: "rise")
        #[serde(default = "default_integration_name_prefix")]
        integration_name_prefix: String,
        /// Default blocked roles for OAuth (default: ["ACCOUNTADMIN", "ORGADMIN", "SECURITYADMIN"])
        #[serde(default = "default_blocked_roles")]
        default_blocked_roles: Vec<String>,
        /// Default OAuth scopes (default: ["refresh_token"])
        #[serde(default = "default_scopes")]
        default_scopes: Vec<String>,
        /// Refresh token validity in seconds (default: 7776000 = 90 days)
        #[serde(default = "default_refresh_token_validity_seconds")]
        refresh_token_validity_seconds: i64,
    },
}

#[allow(dead_code)]
fn default_instance_id_template() -> String {
    "{prefix}-{project_name}-{extension_name}".to_string()
}

#[allow(dead_code)]
fn default_instance_id_prefix() -> String {
    "rise".to_string()
}

#[allow(dead_code)]
fn default_engine_version() -> String {
    "18.2".to_string()
}

#[allow(dead_code)]
fn default_backup_retention_days() -> i32 {
    7 // 7 days of backup retention (reasonable default for production)
}

#[allow(dead_code)]
fn default_integration_name_prefix() -> String {
    "rise".to_string()
}

#[allow(dead_code)]
fn default_blocked_roles() -> Vec<String> {
    vec![
        "ACCOUNTADMIN".to_string(),
        "ORGADMIN".to_string(),
        "SECURITYADMIN".to_string(),
    ]
}

#[allow(dead_code)]
fn default_scopes() -> Vec<String> {
    vec!["refresh_token".to_string()]
}

#[allow(dead_code)]
fn default_refresh_token_validity_seconds() -> i64 {
    7776000 // 90 days
}

/// Platform access control configuration
#[derive(Debug, Deserialize, Clone, JsonSchema)]
pub struct PlatformAccessConfig {
    /// Policy: "allow_all" (default) or "restrictive"
    #[serde(default = "default_platform_access_policy")]
    pub policy: PlatformAccessPolicy,

    /// User emails explicitly granted platform access
    #[serde(default)]
    pub allowed_user_emails: Vec<String>,

    /// IdP groups whose members get platform access
    #[serde(default)]
    pub allowed_idp_groups: Vec<String>,
}

/// Platform access policy enum
#[derive(Debug, Deserialize, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAccessPolicy {
    AllowAll,    // Default: all authenticated users can use platform
    Restrictive, // Only allowlist matches can use platform
}

impl Default for PlatformAccessConfig {
    fn default() -> Self {
        Self {
            policy: PlatformAccessPolicy::AllowAll,
            allowed_user_emails: vec![],
            allowed_idp_groups: vec![],
        }
    }
}

fn default_platform_access_policy() -> PlatformAccessPolicy {
    PlatformAccessPolicy::AllowAll
}
