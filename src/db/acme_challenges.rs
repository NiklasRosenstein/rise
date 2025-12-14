use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{AcmeChallenge, ChallengeStatus, ChallengeType};

/// Create a new ACME challenge
pub async fn create(
    pool: &PgPool,
    domain_id: Uuid,
    challenge_type: ChallengeType,
    record_name: &str,
    record_value: &str,
    authorization_url: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Result<AcmeChallenge> {
    let challenge = sqlx::query_as!(
        AcmeChallenge,
        r#"
        INSERT INTO acme_challenges (
            domain_id, challenge_type, record_name, record_value,
            status, authorization_url, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING
            id, domain_id,
            challenge_type as "challenge_type: ChallengeType",
            record_name, record_value,
            status as "status: ChallengeStatus",
            authorization_url, validated_at, expires_at,
            created_at, updated_at
        "#,
        domain_id,
        challenge_type as ChallengeType,
        record_name,
        record_value,
        ChallengeStatus::Pending as ChallengeStatus,
        authorization_url,
        expires_at,
    )
    .fetch_one(pool)
    .await?;

    Ok(challenge)
}

/// List all challenges for a domain
pub async fn list_by_domain(pool: &PgPool, domain_id: Uuid) -> Result<Vec<AcmeChallenge>> {
    let challenges = sqlx::query_as!(
        AcmeChallenge,
        r#"
        SELECT
            id, domain_id,
            challenge_type as "challenge_type: ChallengeType",
            record_name, record_value,
            status as "status: ChallengeStatus",
            authorization_url, validated_at, expires_at,
            created_at, updated_at
        FROM acme_challenges
        WHERE domain_id = $1
        ORDER BY created_at DESC
        "#,
        domain_id
    )
    .fetch_all(pool)
    .await?;

    Ok(challenges)
}

/// Get a challenge by ID
pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<AcmeChallenge>> {
    let challenge = sqlx::query_as!(
        AcmeChallenge,
        r#"
        SELECT
            id, domain_id,
            challenge_type as "challenge_type: ChallengeType",
            record_name, record_value,
            status as "status: ChallengeStatus",
            authorization_url, validated_at, expires_at,
            created_at, updated_at
        FROM acme_challenges
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(challenge)
}

/// Get the latest pending challenge for a domain
pub async fn get_latest_pending(pool: &PgPool, domain_id: Uuid) -> Result<Option<AcmeChallenge>> {
    let challenge = sqlx::query_as!(
        AcmeChallenge,
        r#"
        SELECT
            id, domain_id,
            challenge_type as "challenge_type: ChallengeType",
            record_name, record_value,
            status as "status: ChallengeStatus",
            authorization_url, validated_at, expires_at,
            created_at, updated_at
        FROM acme_challenges
        WHERE domain_id = $1
          AND status = 'Pending'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        domain_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(challenge)
}

/// Update challenge status
pub async fn update_status(pool: &PgPool, id: Uuid, status: ChallengeStatus) -> Result<()> {
    let validated_at = if status == ChallengeStatus::Valid {
        Some(Utc::now())
    } else {
        None
    };

    sqlx::query!(
        r#"
        UPDATE acme_challenges
        SET status = $1, validated_at = $2
        WHERE id = $3
        "#,
        status as ChallengeStatus,
        validated_at,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete a challenge
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM acme_challenges
        WHERE id = $1
        "#,
        id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Delete all challenges for a domain
pub async fn delete_by_domain(pool: &PgPool, domain_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM acme_challenges
        WHERE domain_id = $1
        "#,
        domain_id
    )
    .execute(pool)
    .await?;

    Ok(())
}
