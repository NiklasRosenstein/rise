use config::{Config, ConfigError, File, Environment};
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerSettings,
    pub auth: AuthSettings,
    pub pocketbase: PocketbaseSettings,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthSettings {
    pub secret: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PocketbaseSettings {
    pub url: String,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        Config::builder()
            // Start off by merging in the "default" configuration file
            .add_source(File::with_name("config/default"))
            // Add in the current environment file
            // Default to 'development' env
            // Note that this file is optional
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            // Add in a local configuration file
            // This file shouldn't be checked in to git
            .add_source(File::with_name("config/local").required(false))
            // Add in settings from the environment (with a prefix of APP)
            // Eg.. `APP_DEBUG=1` would set the `debug` key
            .add_source(Environment::with_prefix("RISE").separator("__"))
            .build()?
            .try_deserialize()
    }
}
