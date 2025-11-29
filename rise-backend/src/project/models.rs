use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum ProjectVisibility {
    Public,
    Private,
}

impl Default for ProjectVisibility {
    fn default() -> Self {
        ProjectVisibility::Private
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum ProjectStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        ProjectStatus::Stopped
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOwner {
    User(String),  // User ID
    Team(String),  // Team ID
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Project {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: ProjectStatus,
    #[serde(default)]
    pub visibility: ProjectVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user: Option<String>,  // Relation to users collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_team: Option<String>,  // Relation to teams collection
}

impl Project {
    /// Compute the URL for this project based on its name
    pub fn url(&self) -> String {
        format!("https://{}.rise.net", self.name)
    }

    /// Get the owner as a ProjectOwner enum
    pub fn owner(&self) -> Option<ProjectOwner> {
        if let Some(ref user_id) = self.owner_user {
            Some(ProjectOwner::User(user_id.clone()))
        } else if let Some(ref team_id) = self.owner_team {
            Some(ProjectOwner::Team(team_id.clone()))
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateProjectRequest {
    pub name: String,
    pub visibility: ProjectVisibility,
    pub owner: ProjectOwner,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateProjectResponse {
    pub project: Project,
}