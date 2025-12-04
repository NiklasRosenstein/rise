use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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

impl From<crate::db::models::ProjectVisibility> for ProjectVisibility {
    fn from(visibility: crate::db::models::ProjectVisibility) -> Self {
        match visibility {
            crate::db::models::ProjectVisibility::Public => ProjectVisibility::Public,
            crate::db::models::ProjectVisibility::Private => ProjectVisibility::Private,
        }
    }
}

impl From<ProjectVisibility> for crate::db::models::ProjectVisibility {
    fn from(visibility: ProjectVisibility) -> Self {
        match visibility {
            ProjectVisibility::Public => crate::db::models::ProjectVisibility::Public,
            ProjectVisibility::Private => crate::db::models::ProjectVisibility::Private,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum ProjectStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
    Deleting,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        ProjectStatus::Stopped
    }
}

impl From<crate::db::models::ProjectStatus> for ProjectStatus {
    fn from(status: crate::db::models::ProjectStatus) -> Self {
        match status {
            crate::db::models::ProjectStatus::Running => ProjectStatus::Running,
            crate::db::models::ProjectStatus::Stopped => ProjectStatus::Stopped,
            crate::db::models::ProjectStatus::Deploying => ProjectStatus::Deploying,
            crate::db::models::ProjectStatus::Failed => ProjectStatus::Failed,
            crate::db::models::ProjectStatus::Deleting => ProjectStatus::Deleting,
        }
    }
}

impl From<ProjectStatus> for crate::db::models::ProjectStatus {
    fn from(status: ProjectStatus) -> Self {
        match status {
            ProjectStatus::Running => crate::db::models::ProjectStatus::Running,
            ProjectStatus::Stopped => crate::db::models::ProjectStatus::Stopped,
            ProjectStatus::Deploying => crate::db::models::ProjectStatus::Deploying,
            ProjectStatus::Failed => crate::db::models::ProjectStatus::Failed,
            ProjectStatus::Deleting => crate::db::models::ProjectStatus::Deleting,
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_url: Option<String>,
    // Timestamps
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub visibility: Option<ProjectVisibility>,
    pub status: Option<ProjectStatus>,
    pub owner: Option<ProjectOwner>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateProjectResponse {
    pub project: Project,
}

// User information for expanded responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

// Team information for expanded responses
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TeamInfo {
    pub id: String,
    pub name: String,
}

// Owner information enum
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OwnerInfo {
    User(UserInfo),
    Team(TeamInfo),
}

// Project with expanded owner information
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectWithOwnerInfo {
    pub id: String,
    pub name: String,
    pub status: ProjectStatus,
    pub visibility: ProjectVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_url: Option<String>,
    pub created: String,
    pub updated: String,
}

// Error response with optional fuzzy match suggestions
#[derive(Debug, Serialize, Clone)]
pub struct ProjectErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
}

// Query parameters for project lookup
#[derive(Debug, Deserialize, Clone)]
pub struct GetProjectParams {
    #[serde(default)]
    pub by_id: bool,
    #[serde(default)]
    pub expand: String,  // Comma-separated list like "owner"
}

impl GetProjectParams {
    /// Check if a field should be expanded
    pub fn should_expand(&self, field: &str) -> bool {
        if self.expand.is_empty() {
            return false;
        }

        let fields: HashSet<&str> = self.expand.split(',').map(|s| s.trim()).collect();
        fields.contains(field)
    }
}