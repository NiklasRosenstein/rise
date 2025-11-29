use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Team {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub members: Vec<String>,  // User IDs
    #[serde(default)]
    pub owners: Vec<String>,   // User IDs
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTeamRequest {
    pub name: String,
    pub members: Vec<String>,  // User IDs to add as members
    pub owners: Vec<String>,   // User IDs to add as owners (must include authenticated user)
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
