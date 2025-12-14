use serde::{Deserialize, Serialize};

/// Domain verification status enum for API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum DomainVerificationStatus {
    Pending,
    Verified,
    Failed,
}

impl From<crate::db::models::DomainVerificationStatus> for DomainVerificationStatus {
    fn from(status: crate::db::models::DomainVerificationStatus) -> Self {
        match status {
            crate::db::models::DomainVerificationStatus::Pending => {
                DomainVerificationStatus::Pending
            }
            crate::db::models::DomainVerificationStatus::Verified => {
                DomainVerificationStatus::Verified
            }
            crate::db::models::DomainVerificationStatus::Failed => {
                DomainVerificationStatus::Failed
            }
        }
    }
}

/// Certificate status enum for API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum CertificateStatus {
    None,
    Pending,
    Issued,
    Failed,
    Expired,
}

impl From<crate::db::models::CertificateStatus> for CertificateStatus {
    fn from(status: crate::db::models::CertificateStatus) -> Self {
        match status {
            crate::db::models::CertificateStatus::None => CertificateStatus::None,
            crate::db::models::CertificateStatus::Pending => CertificateStatus::Pending,
            crate::db::models::CertificateStatus::Issued => CertificateStatus::Issued,
            crate::db::models::CertificateStatus::Failed => CertificateStatus::Failed,
            crate::db::models::CertificateStatus::Expired => CertificateStatus::Expired,
        }
    }
}

/// Custom domain API model
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CustomDomain {
    pub id: String,
    pub project_id: String,
    pub domain_name: String,
    pub cname_target: String,
    pub verification_status: DomainVerificationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    pub certificate_status: CertificateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_issued_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_expires_at: Option<String>,
    pub created: String,
    pub updated: String,
}

impl CustomDomain {
    /// Create from database model with computed CNAME target
    pub fn from_db(
        domain: crate::db::models::CustomDomain,
        cname_target: String,
    ) -> Self {
        CustomDomain {
            id: domain.id.to_string(),
            project_id: domain.project_id.to_string(),
            domain_name: domain.domain_name,
            cname_target,
            verification_status: DomainVerificationStatus::from(domain.verification_status),
            verified_at: domain.verified_at.map(|dt| dt.to_rfc3339()),
            certificate_status: CertificateStatus::from(domain.certificate_status),
            certificate_issued_at: domain.certificate_issued_at.map(|dt| dt.to_rfc3339()),
            certificate_expires_at: domain.certificate_expires_at.map(|dt| dt.to_rfc3339()),
            created: domain.created_at.to_rfc3339(),
            updated: domain.updated_at.to_rfc3339(),
        }
    }
}

/// Helper to compute CNAME target from project URL
pub fn compute_cname_target(project_url: Option<&str>, project_name: &str, default_domain: &str) -> String {
    match project_url {
        Some(url) => {
            // Extract hostname from project_url (e.g., "https://myapp.rise.dev" -> "myapp.rise.dev")
            url.trim_start_matches("http://")
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or(project_name)
                .to_string()
        }
        None => {
            // Fall back to project name + default domain
            format!("{}.{}", project_name, default_domain)
        }
    }
}

/// Request to add a custom domain
#[derive(Debug, Deserialize)]
pub struct AddDomainRequest {
    pub domain_name: String,
}

/// Response after adding a custom domain
#[derive(Debug, Serialize)]
pub struct AddDomainResponse {
    pub domain: CustomDomain,
    pub instructions: DomainSetupInstructions,
}

/// Instructions for setting up a custom domain
#[derive(Debug, Serialize)]
pub struct DomainSetupInstructions {
    pub cname_record: CnameRecord,
    pub message: String,
}

/// CNAME record to configure
#[derive(Debug, Serialize)]
pub struct CnameRecord {
    pub name: String,
    pub value: String,
}

/// Challenge type enum for API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum ChallengeType {
    #[serde(rename = "dns-01")]
    Dns01,
    #[serde(rename = "http-01")]
    Http01,
}

impl From<crate::db::models::ChallengeType> for ChallengeType {
    fn from(challenge_type: crate::db::models::ChallengeType) -> Self {
        match challenge_type {
            crate::db::models::ChallengeType::Dns01 => ChallengeType::Dns01,
            crate::db::models::ChallengeType::Http01 => ChallengeType::Http01,
        }
    }
}

/// Challenge status enum for API
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum ChallengeStatus {
    Pending,
    Valid,
    Invalid,
    Expired,
}

impl From<crate::db::models::ChallengeStatus> for ChallengeStatus {
    fn from(status: crate::db::models::ChallengeStatus) -> Self {
        match status {
            crate::db::models::ChallengeStatus::Pending => ChallengeStatus::Pending,
            crate::db::models::ChallengeStatus::Valid => ChallengeStatus::Valid,
            crate::db::models::ChallengeStatus::Invalid => ChallengeStatus::Invalid,
            crate::db::models::ChallengeStatus::Expired => ChallengeStatus::Expired,
        }
    }
}

/// ACME challenge API model
#[derive(Debug, Serialize)]
pub struct AcmeChallenge {
    pub id: String,
    pub domain_id: String,
    pub challenge_type: ChallengeType,
    pub record_name: String,
    pub record_value: String,
    pub status: ChallengeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created: String,
}

impl From<crate::db::models::AcmeChallenge> for AcmeChallenge {
    fn from(challenge: crate::db::models::AcmeChallenge) -> Self {
        AcmeChallenge {
            id: challenge.id.to_string(),
            domain_id: challenge.domain_id.to_string(),
            challenge_type: ChallengeType::from(challenge.challenge_type),
            record_name: challenge.record_name,
            record_value: challenge.record_value,
            status: ChallengeStatus::from(challenge.status),
            validated_at: challenge.validated_at.map(|dt| dt.to_rfc3339()),
            expires_at: challenge.expires_at.map(|dt| dt.to_rfc3339()),
            created: challenge.created_at.to_rfc3339(),
        }
    }
}

/// Domain verification response
#[derive(Debug, Serialize)]
pub struct VerifyDomainResponse {
    pub domain: CustomDomain,
    pub verification_result: VerificationResult,
}

/// Verification result
#[derive(Debug, Serialize)]
pub struct VerificationResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_value: Option<String>,
}
