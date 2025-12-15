use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to create a new service account
#[derive(Debug, Deserialize)]
pub struct CreateWorkloadIdentityRequest {
    pub issuer_url: String,
    pub claims: HashMap<String, String>,
}

/// Request to update an existing service account
#[derive(Debug, Deserialize)]
pub struct UpdateWorkloadIdentityRequest {
    pub issuer_url: Option<String>,
    pub claims: Option<HashMap<String, String>>,
}

/// Response for a single workload identity
#[derive(Debug, Serialize)]
pub struct WorkloadIdentityResponse {
    pub id: String,
    pub email: String,
    pub project_name: String,
    pub issuer_url: String,
    pub claims: HashMap<String, String>,
    pub created_at: String,
}

/// Response for listing workload identities
#[derive(Debug, Serialize)]
pub struct ListWorkloadIdentitiesResponse {
    pub workload_identities: Vec<WorkloadIdentityResponse>,
}
