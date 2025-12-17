use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Extension API response model
#[derive(Debug, Serialize, Deserialize)]
pub struct Extension {
    pub extension: String,
    pub spec: Value,
    pub status: Value,
    /// Human-readable status summary formatted by the extension provider
    pub status_summary: String,
    pub created: String,
    pub updated: String,
}

/// Request to create an extension
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateExtensionRequest {
    pub spec: Value,
}

/// Response after creating an extension
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateExtensionResponse {
    pub extension: Extension,
}

/// Request to update an extension
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateExtensionRequest {
    pub spec: Value,
}

/// Response after updating an extension
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateExtensionResponse {
    pub extension: Extension,
}

/// Response listing extensions
#[derive(Debug, Serialize, Deserialize)]
pub struct ListExtensionsResponse {
    pub extensions: Vec<Extension>,
}
