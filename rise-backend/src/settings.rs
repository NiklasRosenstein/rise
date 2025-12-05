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
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub public_url: String,
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

/// Registry provider configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RegistrySettings {
    Ecr {
        region: String,
        account_id: String,
        #[serde(default)]
        access_key_id: Option<String>,
        #[serde(default)]
        secret_access_key: Option<String>,
    },
    Docker {
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
