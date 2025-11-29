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
pub struct Project {
    pub id: String,
    pub name: String,
    pub status: ProjectStatus,
    pub url: String,
    pub visibility: ProjectVisibility,
    pub owner: String,
    pub created: String,
    pub updated: String,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            id: "".to_string(),
            name: "".to_string(),
            status: ProjectStatus::default(),
            url: "".to_string(),
            visibility: ProjectVisibility::default(),
            owner: "".to_string(),
            created: "".to_string(),
            updated: "".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateProjectRequest {
    pub name: String,
    pub visibility: ProjectVisibility,
    pub owner: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateProjectResponse {
    pub project: Project,
}