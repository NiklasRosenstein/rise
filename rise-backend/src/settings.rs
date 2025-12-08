use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub database: DatabaseSettings,
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

    /// API domain for OAuth2 redirects (e.g., "api.rise.net")
    pub api_domain: String,

    /// Cookie domain for session cookies (e.g., ".rise.net" for all subdomains, "" for current host only)
    #[serde(default)]
    pub cookie_domain: String,

    /// Whether to set Secure flag on cookies (true for HTTPS, false for HTTP development)
    #[serde(default = "default_cookie_secure")]
    pub cookie_secure: bool,
}

fn default_cookie_secure() -> bool {
    true
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
    pub url: String,
}

fn default_repo_prefix() -> String {
    "rise/".to_string()
}

fn default_ingress_class() -> String {
    "nginx".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct KubernetesSettings {
    /// Optional kubeconfig path (defaults to in-cluster or ~/.kube/config)
    #[serde(default)]
    pub kubeconfig: Option<String>,

    /// Ingress class to use (e.g., "nginx")
    #[serde(default = "default_ingress_class")]
    pub ingress_class: String,

    /// Domain suffix for default deployment group (e.g., "apps.rise.net")
    /// Results in URLs like: https://{project}.apps.rise.net
    pub domain_suffix: String,

    /// Optional domain suffix for non-default deployment groups
    /// If not set, uses domain_suffix for all groups
    /// Results in URLs like: https://{project}-{group}.preview.rise.net
    #[serde(default)]
    pub non_default_domain_suffix: Option<String>,

    /// Backend URL for Nginx auth subrequests (e.g., "https://api.rise.net" or "http://host.minikube.internal:3000")
    /// This is the URL Nginx will use internally to validate authentication
    pub auth_backend_url: String,

    /// API domain for user-facing OAuth redirects (e.g., "api.rise.net")
    /// Used in auth-signin annotation for redirecting users to login
    pub api_domain: String,
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

        Config::builder()
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
            .try_deserialize()
    }
}
