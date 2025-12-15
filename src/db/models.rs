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
    pub project_url: Option<String>,
    /// Finalizers that must be removed before the project can be deleted.
    /// Each controller adds its own finalizer when it creates external resources.
    pub finalizers: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Project status enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ProjectStatus {
    Stopped,
    Running,
    Failed,
    Deploying,
    Deleting,
    Terminated,
}

impl std::fmt::Display for ProjectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectStatus::Stopped => write!(f, "Stopped"),
            ProjectStatus::Running => write!(f, "Running"),
            ProjectStatus::Failed => write!(f, "Failed"),
            ProjectStatus::Deploying => write!(f, "Deploying"),
            ProjectStatus::Deleting => write!(f, "Deleting"),
            ProjectStatus::Terminated => write!(f, "Terminated"),
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
    /// Whether this team is managed by an Identity Provider
    /// When true, membership is controlled by IdP groups claim
    pub idp_managed: bool,
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

/// Service Account model - represents workload identity for CI/CD automation
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServiceAccount {
    pub id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub issuer_url: String,
    pub claims: serde_json::Value, // JSONB stored as serde_json::Value
    pub sequence: i32,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deployment model - represents application deployments
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Deployment {
    pub id: Uuid,
    pub deployment_id: String,
    pub project_id: Uuid,
    pub created_by_id: Uuid,
    pub status: DeploymentStatus,
    pub deployment_group: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub termination_reason: Option<TerminationReason>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub build_logs: Option<String>,
    pub controller_metadata: serde_json::Value,
    pub deployment_url: Option<String>,
    pub image: Option<String>,
    pub image_digest: Option<String>,
    pub http_port: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deployment status enum - tracks lifecycle of deployment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
pub enum DeploymentStatus {
    // Build/Deploy states (pre-infrastructure)
    Pending,
    Building,
    Pushing,
    Pushed, // Handoff point between CLI and controller
    Deploying,

    // Running states (post-infrastructure)
    Healthy,   // Running and passing health checks
    Unhealthy, // Running but failing health checks

    // Cancellation states (pre-infrastructure)
    Cancelling, // Being cancelled before infrastructure provisioned
    Cancelled,  // Terminal: cancelled before infrastructure provisioned

    // Termination states (post-infrastructure)
    Terminating, // Being gracefully terminated
    Stopped,     // Terminal: user-initiated termination
    Superseded,  // Terminal: replaced by newer deployment

    // Terminal states
    Failed,  // Terminal: could not reach Healthy
    Expired, // Terminal: deployment expired after reaching Healthy
}

impl std::fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentStatus::Pending => write!(f, "Pending"),
            DeploymentStatus::Building => write!(f, "Building"),
            DeploymentStatus::Pushing => write!(f, "Pushing"),
            DeploymentStatus::Pushed => write!(f, "Pushed"),
            DeploymentStatus::Deploying => write!(f, "Deploying"),
            DeploymentStatus::Healthy => write!(f, "Healthy"),
            DeploymentStatus::Unhealthy => write!(f, "Unhealthy"),
            DeploymentStatus::Cancelling => write!(f, "Cancelling"),
            DeploymentStatus::Cancelled => write!(f, "Cancelled"),
            DeploymentStatus::Terminating => write!(f, "Terminating"),
            DeploymentStatus::Stopped => write!(f, "Stopped"),
            DeploymentStatus::Superseded => write!(f, "Superseded"),
            DeploymentStatus::Failed => write!(f, "Failed"),
            DeploymentStatus::Expired => write!(f, "Expired"),
        }
    }
}

/// Termination reason enum - tracks why deployment was terminated
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "termination_reason")]
pub enum TerminationReason {
    UserStopped, // User explicitly stopped the deployment
    Superseded,  // Replaced by newer deployment
    Cancelled,   // Cancelled before infrastructure provisioned
    Failed,      // Deployment timed out or failed to become healthy
    Expired,     // Deployment expired after specified time limit
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationReason::UserStopped => write!(f, "UserStopped"),
            TerminationReason::Superseded => write!(f, "Superseded"),
            TerminationReason::Cancelled => write!(f, "Cancelled"),
            TerminationReason::Failed => write!(f, "Failed"),
            TerminationReason::Expired => write!(f, "Expired"),
        }
    }
}

/// Project environment variable
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectEnvVar {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    /// Encrypted value if is_secret = true
    pub value: String,
    pub is_secret: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deployment environment variable
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeploymentEnvVar {
    pub id: Uuid,
    pub deployment_id: Uuid,
    pub key: String,
    /// Encrypted value if is_secret = true
    pub value: String,
    pub is_secret: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Custom domain model - represents custom domains for projects
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CustomDomain {
    pub id: Uuid,
    pub project_id: Uuid,
    pub domain_name: String,
    pub verification_status: DomainVerificationStatus,
    pub verified_at: Option<DateTime<Utc>>,
    pub certificate_status: CertificateStatus,
    pub certificate_issued_at: Option<DateTime<Utc>>,
    pub certificate_expires_at: Option<DateTime<Utc>>,
    pub certificate_pem: Option<String>,
    pub certificate_key_pem: Option<String>,
    pub acme_order_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domain verification status enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum DomainVerificationStatus {
    Pending,
    Verified,
    Failed,
}

impl std::fmt::Display for DomainVerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainVerificationStatus::Pending => write!(f, "Pending"),
            DomainVerificationStatus::Verified => write!(f, "Verified"),
            DomainVerificationStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// Certificate status enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum CertificateStatus {
    None,
    Pending,
    Issued,
    Failed,
    Expired,
}

impl std::fmt::Display for CertificateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CertificateStatus::None => write!(f, "None"),
            CertificateStatus::Pending => write!(f, "Pending"),
            CertificateStatus::Issued => write!(f, "Issued"),
            CertificateStatus::Failed => write!(f, "Failed"),
            CertificateStatus::Expired => write!(f, "Expired"),
        }
    }
}

/// ACME challenge model - represents DNS-01 challenges for domain verification
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AcmeChallenge {
    pub id: Uuid,
    pub domain_id: Uuid,
    pub challenge_type: ChallengeType,
    pub record_name: String,
    pub record_value: String,
    pub status: ChallengeStatus,
    pub authorization_url: Option<String>,
    pub validated_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Challenge type enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ChallengeType {
    #[sqlx(rename = "dns-01")]
    Dns01,
    #[sqlx(rename = "http-01")]
    Http01,
}

impl std::fmt::Display for ChallengeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChallengeType::Dns01 => write!(f, "dns-01"),
            ChallengeType::Http01 => write!(f, "http-01"),
        }
    }
}

/// Challenge status enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ChallengeStatus {
    Pending,
    Valid,
    Invalid,
    Expired,
}

impl std::fmt::Display for ChallengeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChallengeStatus::Pending => write!(f, "Pending"),
            ChallengeStatus::Valid => write!(f, "Valid"),
            ChallengeStatus::Invalid => write!(f, "Invalid"),
            ChallengeStatus::Expired => write!(f, "Expired"),
        }
    }
}
