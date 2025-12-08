use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub database: DatabaseSettings,
    #[serde(default)]
    pub controller: ControllerSettings,
    #[serde(default)]
    pub registry: Option<RegistrySettings>,
    #[serde(default)]
    pub kubernetes: Option<KubernetesSettings>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub public_url: String,

    /// Cookie domain for session cookies (e.g., ".rise.dev" for all subdomains, "" for current host only)
    #[serde(default)]
    pub cookie_domain: String,

    /// Whether to set Secure flag on cookies (true for HTTPS, false for HTTP development)
    #[serde(default = "default_cookie_secure")]
    pub cookie_secure: bool,
}

fn default_cookie_secure() -> bool {
    true
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

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct AuthSettings {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// List of admin user emails (have full permissions)
    #[serde(default)]
    pub admin_users: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    #[serde(default)]
    pub url: String,
}

fn default_repo_prefix() -> String {
    "rise/".to_string()
}

fn default_ingress_class() -> String {
    "nginx".to_string()
}

fn default_namespace_format() -> String {
    "rise-{project_name}".to_string()
}

/// Kubernetes deployment controller configuration
///
/// # Ingress Authentication Architecture
///
/// For cookie-based authentication to work with private projects, the backend API
/// must be accessible on the same parent domain as the deployed applications:
///
/// **Required Setup:**
/// 1. Deploy backend with Ingress at a subdomain (e.g., `rise.dev`)
/// 2. Configure apps to use sibling subdomains (e.g., `{project}.apps.rise.dev`)
/// 3. Set `cookie_domain` to parent domain (e.g., `.rise.dev`)
/// 4. Set `public_url` to API ingress URL (e.g., `https://rise.dev`)
///
/// **Example Production Configuration:**
/// ```toml
/// [server]
/// public_url = "https://rise.dev"
/// cookie_domain = ".rise.dev"  # Shared across rise.dev and *.apps.rise.dev
/// cookie_secure = true
///
/// [kubernetes]
/// hostname_format = "{project_name}.apps.rise.dev"
/// namespace_format = "rise-{project_name}"
/// auth_backend_url = "http://rise-backend.default.svc.cluster.local:3000"  # Internal cluster URL
/// auth_signin_url = "https://rise.dev"  # Public API URL for browser redirects
/// ```
///
/// **How it works:**
/// 1. User visits `myapp.apps.rise.dev` (deployed application)
/// 2. Nginx ingress checks authentication via `auth_backend_url` (cluster-internal)
/// 3. If unauthenticated, redirects browser to `auth_signin_url` (public API URL)
/// 4. Backend sets session cookie with `domain=.rise.dev`
/// 5. Browser redirects back to `myapp.apps.rise.dev` with cookie
/// 6. Cookie is sent by browser (same parent domain) and Nginx auth succeeds
///
/// **Development Setup (Minikube):**
/// - Create Ingress for backend at `rise.dev`
/// - Add `/etc/hosts` entries pointing to Minikube IP
/// - Use `auth_backend_url = "http://172.17.0.1:3000"` (Docker bridge IP)
/// - Use `auth_signin_url = "http://rise.dev"` (through ingress)
#[derive(Debug, Clone, Deserialize)]
pub struct KubernetesSettings {
    /// Optional kubeconfig path (defaults to in-cluster or ~/.kube/config)
    #[serde(default)]
    pub kubeconfig: Option<String>,

    /// Ingress class to use (e.g., "nginx")
    #[serde(default = "default_ingress_class")]
    pub ingress_class: String,

    /// Hostname format for default deployment group
    /// Template variables: {project_name}
    /// Example: "{project_name}.apps.rise.dev" → hostname "myapp.apps.rise.dev" for project "myapp"
    /// Must contain {project_name} placeholder
    pub hostname_format: String,

    /// Hostname format for non-default deployment groups
    /// Template variables: {project_name}, {deployment_group}
    /// Example: "{project_name}-{deployment_group}.preview.rise.dev"
    /// If not set, uses hostname_format with "-{deployment_group}" suffix before domain
    /// Must contain {project_name} placeholder
    #[serde(default)]
    pub nondefault_hostname_format: Option<String>,

    /// Backend URL for Nginx auth subrequests (internal cluster URL)
    /// Example: "http://rise-backend.default.svc.cluster.local:3000"
    /// This is the URL Nginx will use internally within the cluster to validate authentication.
    /// For Minikube development, use "http://172.17.0.1:3000" (Docker bridge IP) to reach host.
    pub auth_backend_url: String,

    /// Public backend URL for browser redirects during authentication
    /// Example: "https://rise.dev"
    /// This must be the public URL where the backend is accessible via Ingress.
    /// The domain should share a parent with app domains for cookie sharing (see struct docs).
    pub auth_signin_url: String,

    /// Namespace format template for deployed applications
    /// Template variables: {project_name}
    /// Example: "rise-{project_name}" → namespace "rise-myapp" for project "myapp"
    /// Defaults to "rise-{project_name}"
    #[serde(default = "default_namespace_format")]
    pub namespace_format: String,

    /// Ingress annotations to apply to all deployed application ingresses
    /// Example: {"cert-manager.io/cluster-issuer": "letsencrypt-prod"}
    #[serde(default)]
    pub ingress_annotations: std::collections::HashMap<String, String>,

    /// TLS secret name for ingress certificates
    /// If set, enables TLS on all ingresses with this secret
    /// Example: "rise-apps-tls" (secret must exist in each namespace)
    #[serde(default)]
    pub ingress_tls_secret_name: Option<String>,
}

/// Registry provider configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RegistrySettings {
    Ecr {
        region: String,
        account_id: String,
        /// Literal prefix for ECR repository names (e.g., "rise/" → repos named "rise/{project}")
        #[serde(default = "default_repo_prefix")]
        repo_prefix: String,
        /// IAM role ARN for ECR controller operations (create/delete/tag repositories)
        role_arn: String,
        /// IAM role ARN for push operations (assumed to generate scoped credentials)
        push_role_arn: String,
        /// Whether to automatically delete ECR repos when projects are deleted
        #[serde(default)]
        auto_remove: bool,
        #[serde(default)]
        access_key_id: Option<String>,
        #[serde(default)]
        secret_access_key: Option<String>,
    },
    #[serde(rename = "oci-client-auth", alias = "docker")]
    OciClientAuth {
        registry_url: String,
        #[serde(default)]
        namespace: String,
    },
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());
        let config_dir = env::var("RISE_CONFIG_DIR").unwrap_or_else(|_| "/config".into());

        let mut settings: Settings = Config::builder()
            // Start off by merging in the "default" configuration file
            .add_source(File::with_name(&format!("{}/default.toml", config_dir)))
            // Add in the current environment file
            // Default to 'development' env
            // Note that this file is optional
            .add_source(File::with_name(&format!("{}/{}", config_dir, run_mode)).required(false))
            // Add in a local configuration file
            // This file shouldn't be checked in to git
            .add_source(File::with_name(&format!("{}/local", config_dir)).required(false))
            // Add in settings from the environment (with a prefix of APP)
            // Eg.. `APP_DEBUG=1` would set the `debug` key
            .add_source(Environment::with_prefix("RISE").separator("__"))
            .build()?
            .try_deserialize()?;

        // Special handling for DATABASE_URL environment variable (common convention)
        // This takes precedence over both TOML config and RISE_DATABASE__URL
        if let Ok(database_url) = env::var("DATABASE_URL") {
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

        // Validate Kubernetes settings if configured
        if let Some(ref k8s) = settings.kubernetes {
            Self::validate_format_string(&k8s.namespace_format, "namespace_format", "{project_name}")?;
            Self::validate_format_string(&k8s.hostname_format, "hostname_format", "{project_name}")?;

            if let Some(ref nondefault_format) = k8s.nondefault_hostname_format {
                Self::validate_format_string(nondefault_format, "nondefault_hostname_format", "{project_name}")?;
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
