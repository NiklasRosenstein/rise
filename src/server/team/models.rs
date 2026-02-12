use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Team {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub members: Vec<UserInfo>,
    #[serde(default)]
    pub owners: Vec<UserInfo>,
    /// Whether this team is managed by an Identity Provider
    #[serde(default)]
    pub idp_managed: bool,
    // Timestamps
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTeamRequest {
    pub name: String,
    pub members: Vec<String>, // User IDs to add as members
    pub owners: Vec<String>,  // User IDs to add as owners (must include authenticated user)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTeamResponse {
    pub team: Team,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateTeamRequest {
    pub name: Option<String>,
    pub members: Option<Vec<String>>,
    pub owners: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateTeamResponse {
    pub team: Team,
}

// User information for team responses
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

// Error response with optional fuzzy match suggestions
#[derive(Debug, Serialize, Clone)]
pub struct TeamErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
}

// Query parameters for team lookup
#[derive(Debug, Deserialize, Clone)]
pub struct GetTeamParams {
    #[serde(default)]
    pub by_id: bool,
}
