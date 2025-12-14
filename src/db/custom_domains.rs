use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    CertificateStatus, CustomDomain, DomainVerificationStatus,
};

/// Create a new custom domain
pub async fn create(
    pool: &PgPool,
    project_id: Uuid,
    domain_name: &str,
    cname_target: &str,
) -> Result<CustomDomain> {
    let domain = sqlx::query_as!(
        CustomDomain,
        r#"
        INSERT INTO custom_domains (
            project_id, domain_name, cname_target,
            verification_status, certificate_status
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            id, project_id, domain_name, cname_target,
            verification_status as "verification_status: DomainVerificationStatus",
            verified_at,
            certificate_status as "certificate_status: CertificateStatus",
            certificate_issued_at, certificate_expires_at,
            certificate_pem, certificate_key_pem, acme_order_url,
            created_at, updated_at
        "#,
        project_id,
        domain_name,
        cname_target,
        DomainVerificationStatus::Pending as DomainVerificationStatus,
        CertificateStatus::None as CertificateStatus,
    )
    .fetch_one(pool)
    .await?;

    Ok(domain)
}

/// List all custom domains for a project
pub async fn list_by_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<CustomDomain>> {
    let domains = sqlx::query_as!(
        CustomDomain,
        r#"
        SELECT
            id, project_id, domain_name, cname_target,
            verification_status as "verification_status: DomainVerificationStatus",
            verified_at,
            certificate_status as "certificate_status: CertificateStatus",
            certificate_issued_at, certificate_expires_at,
            certificate_pem, certificate_key_pem, acme_order_url,
            created_at, updated_at
        FROM custom_domains
        WHERE project_id = $1
        ORDER BY created_at DESC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await?;

    Ok(domains)
}

/// Get a custom domain by ID
pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<CustomDomain>> {
    let domain = sqlx::query_as!(
        CustomDomain,
        r#"
        SELECT
            id, project_id, domain_name, cname_target,
            verification_status as "verification_status: DomainVerificationStatus",
            verified_at,
            certificate_status as "certificate_status: CertificateStatus",
            certificate_issued_at, certificate_expires_at,
            certificate_pem, certificate_key_pem, acme_order_url,
            created_at, updated_at
        FROM custom_domains
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(domain)
}

/// Get a custom domain by domain name
pub async fn get_by_domain_name(pool: &PgPool, domain_name: &str) -> Result<Option<CustomDomain>> {
    let domain = sqlx::query_as!(
        CustomDomain,
        r#"
        SELECT
            id, project_id, domain_name, cname_target,
            verification_status as "verification_status: DomainVerificationStatus",
            verified_at,
            certificate_status as "certificate_status: CertificateStatus",
            certificate_issued_at, certificate_expires_at,
            certificate_pem, certificate_key_pem, acme_order_url,
            created_at, updated_at
        FROM custom_domains
        WHERE domain_name = $1
        "#,
        domain_name
    )
    .fetch_optional(pool)
    .await?;

    Ok(domain)
}

/// Update domain verification status
pub async fn update_verification_status(
    pool: &PgPool,
    id: Uuid,
    status: DomainVerificationStatus,
) -> Result<()> {
    let verified_at = if status == DomainVerificationStatus::Verified {
        Some(Utc::now())
    } else {
        None
    };

    sqlx::query!(
        r#"
        UPDATE custom_domains
        SET verification_status = $1, verified_at = $2
        WHERE id = $3
        "#,
        status as DomainVerificationStatus,
        verified_at,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Update certificate status
pub async fn update_certificate_status(
    pool: &PgPool,
    id: Uuid,
    status: CertificateStatus,
    certificate_pem: Option<&str>,
    certificate_key_pem: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Result<()> {
    let issued_at = if status == CertificateStatus::Issued {
        Some(Utc::now())
    } else {
        None
    };

    sqlx::query!(
        r#"
        UPDATE custom_domains
        SET certificate_status = $1,
            certificate_issued_at = $2,
            certificate_expires_at = $3,
            certificate_pem = $4,
            certificate_key_pem = $5
        WHERE id = $6
        "#,
        status as CertificateStatus,
        issued_at,
        expires_at,
        certificate_pem,
        certificate_key_pem,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Update ACME order URL
pub async fn update_acme_order_url(
    pool: &PgPool,
    id: Uuid,
    acme_order_url: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE custom_domains
        SET acme_order_url = $1
        WHERE id = $2
        "#,
        acme_order_url,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a custom domain
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM custom_domains
        WHERE id = $1
        "#,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// List domains that need certificate renewal (expiring within 30 days)
pub async fn list_expiring_certificates(pool: &PgPool) -> Result<Vec<CustomDomain>> {
    let domains = sqlx::query_as!(
        CustomDomain,
        r#"
        SELECT
            id, project_id, domain_name, cname_target,
            verification_status as "verification_status: DomainVerificationStatus",
            verified_at,
            certificate_status as "certificate_status: CertificateStatus",
            certificate_issued_at, certificate_expires_at,
            certificate_pem, certificate_key_pem, acme_order_url,
            created_at, updated_at
        FROM custom_domains
        WHERE certificate_status = 'Issued'
          AND certificate_expires_at IS NOT NULL
          AND certificate_expires_at < NOW() + INTERVAL '30 days'
        ORDER BY certificate_expires_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(domains)
}
