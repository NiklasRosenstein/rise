use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// User model - represents authenticated users from Dex
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Project model - represents deployable applications
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub status: ProjectStatus,
    pub visibility: ProjectVisibility,
    pub owner_user_id: Option<Uuid>,
    pub owner_team_id: Option<Uuid>,
    pub active_deployment_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Project status enum
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ProjectStatus {
    Stopped,
    Running,
    Failed,
    Deploying,
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectStatus::Stopped => write!(f, "Stopped"),
            ProjectStatus::Running => write!(f, "Running"),
            ProjectStatus::Failed => write!(f, "Failed"),
            ProjectStatus::Deploying => write!(f, "Deploying"),
        }
    }
}

/// Project visibility enum
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ProjectVisibility {
    Public,
    Private,
}

impl std::fmt::Display for ProjectVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectVisibility::Public => write!(f, "Public"),
            ProjectVisibility::Private => write!(f, "Private"),
        }
    }
}

/// Team model - represents groups of users
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Team member model - junction table for team membership
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TeamMember {
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: TeamRole,
    pub created_at: DateTime<Utc>,
}

/// Team role enum
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
pub enum TeamRole {
    #[sqlx(rename = "owner")]
    Owner,
    #[sqlx(rename = "member")]
    Member,
}

impl std::fmt::Display for TeamRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeamRole::Owner => write!(f, "owner"),
            TeamRole::Member => write!(f, "member"),
        }
    }
}

/// Deployment model - represents application deployments
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Deployment {
    pub id: Uuid,
    pub deployment_id: String,
    pub project_id: Uuid,
    pub created_by_id: Uuid,
    pub status: DeploymentStatus,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub build_logs: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deployment status enum
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum DeploymentStatus {
    Pending,
    Building,
    Pushing,
    Deploying,
    Completed,
    Failed,
}

impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentStatus::Pending => write!(f, "Pending"),
            DeploymentStatus::Building => write!(f, "Building"),
            DeploymentStatus::Pushing => write!(f, "Pushing"),
            DeploymentStatus::Deploying => write!(f, "Deploying"),
            DeploymentStatus::Completed => write!(f, "Completed"),
            DeploymentStatus::Failed => write!(f, "Failed"),
        }
    }
}
