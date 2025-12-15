use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::CustomDomain;

/// List all custom domains for a project
pub async fn list_project_custom_domains(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<CustomDomain>> {
    let domains = sqlx::query_as!(
        CustomDomain,
        r#"
        SELECT id, project_id, domain, created_at, updated_at
        FROM project_custom_domains
        WHERE project_id = $1
        ORDER BY domain ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list project custom domains")?;

    Ok(domains)
}

/// Get a specific custom domain for a project
pub async fn get_custom_domain(
    pool: &PgPool,
    project_id: Uuid,
    domain: &str,
) -> Result<Option<CustomDomain>> {
    let domain = sqlx::query_as!(
        CustomDomain,
        r#"
        SELECT id, project_id, domain, created_at, updated_at
        FROM project_custom_domains
        WHERE project_id = $1 AND domain = $2
        "#,
        project_id,
        domain
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get custom domain")?;

    Ok(domain)
}

/// Add a new custom domain to a project
pub async fn add_custom_domain(
    pool: &PgPool,
    project_id: Uuid,
    domain: &str,
) -> Result<CustomDomain> {
    let domain = sqlx::query_as!(
        CustomDomain,
        r#"
        INSERT INTO project_custom_domains (project_id, domain)
        VALUES ($1, $2)
        RETURNING id, project_id, domain, created_at, updated_at
        "#,
        project_id,
        domain
    )
    .fetch_one(pool)
    .await
    .context("Failed to add custom domain")?;

    Ok(domain)
}

/// Delete a custom domain from a project
pub async fn delete_custom_domain(pool: &PgPool, project_id: Uuid, domain: &str) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM project_custom_domains
        WHERE project_id = $1 AND domain = $2
        "#,
        project_id,
        domain
    )
    .execute(pool)
    .await
    .context("Failed to delete custom domain")?;

    Ok(result.rows_affected() > 0)
}
