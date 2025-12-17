use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::models::ProjectExtension;

/// Create or update extension for project
pub async fn upsert(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
    spec: &Value,
) -> Result<ProjectExtension> {
    sqlx::query_as!(
        ProjectExtension,
        r#"
        INSERT INTO project_extensions (project_id, extension, spec)
        VALUES ($1, $2, $3)
        ON CONFLICT (project_id, extension)
        DO UPDATE SET
            spec = EXCLUDED.spec,
            updated_at = NOW()
        RETURNING project_id, extension,
                  spec as "spec: Value",
                  status as "status: Value",
                  created_at, updated_at, deleted_at
        "#,
        project_id,
        extension,
        spec
    )
    .fetch_one(pool)
    .await
    .context("Failed to upsert project extension")
}

/// List all extensions for a project
pub async fn list_by_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<ProjectExtension>> {
    sqlx::query_as!(
        ProjectExtension,
        r#"
        SELECT project_id, extension,
               spec as "spec: Value",
               status as "status: Value",
               created_at, updated_at, deleted_at
        FROM project_extensions
        WHERE project_id = $1 AND deleted_at IS NULL
        ORDER BY created_at ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list project extensions")
}

/// List all extensions with a specific extension name (across all projects)
pub async fn list_by_extension_name(
    pool: &PgPool,
    extension_name: &str,
) -> Result<Vec<ProjectExtension>> {
    sqlx::query_as!(
        ProjectExtension,
        r#"
        SELECT project_id, extension,
               spec as "spec: Value",
               status as "status: Value",
               created_at, updated_at, deleted_at
        FROM project_extensions
        WHERE extension = $1
        ORDER BY created_at ASC
        "#,
        extension_name
    )
    .fetch_all(pool)
    .await
    .context("Failed to list extensions by name")
}

/// Get extension by project and name
pub async fn find_by_project_and_name(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
) -> Result<Option<ProjectExtension>> {
    sqlx::query_as!(
        ProjectExtension,
        r#"
        SELECT project_id, extension,
               spec as "spec: Value",
               status as "status: Value",
               created_at, updated_at, deleted_at
        FROM project_extensions
        WHERE project_id = $1 AND extension = $2 AND deleted_at IS NULL
        "#,
        project_id,
        extension
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find project extension")
}

/// Mark extension for deletion
pub async fn mark_deleted(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
) -> Result<ProjectExtension> {
    sqlx::query_as!(
        ProjectExtension,
        r#"
        UPDATE project_extensions
        SET deleted_at = NOW()
        WHERE project_id = $1 AND extension = $2 AND deleted_at IS NULL
        RETURNING project_id, extension,
                  spec as "spec: Value",
                  status as "status: Value",
                  created_at, updated_at, deleted_at
        "#,
        project_id,
        extension
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark extension as deleted")
}

/// Update extension status
pub async fn update_status(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
    status: &Value,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE project_extensions
        SET status = $3, updated_at = NOW()
        WHERE project_id = $1 AND extension = $2
        "#,
        project_id,
        extension,
        status
    )
    .execute(pool)
    .await
    .context("Failed to update extension status")?;

    Ok(())
}

/// Find extension by project and name, including soft-deleted ones
#[allow(dead_code)]
pub async fn find_by_project_and_name_including_deleted(
    pool: &PgPool,
    project_id: Uuid,
    extension: &str,
) -> Result<Option<ProjectExtension>> {
    sqlx::query_as!(
        ProjectExtension,
        r#"
        SELECT project_id, extension,
               spec as "spec: Value",
               status as "status: Value",
               created_at, updated_at, deleted_at
        FROM project_extensions
        WHERE project_id = $1 AND extension = $2
        "#,
        project_id,
        extension
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find project extension (including deleted)")
}

/// Permanently delete extension record
pub async fn delete_permanently(pool: &PgPool, project_id: Uuid, extension: &str) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM project_extensions
        WHERE project_id = $1 AND extension = $2
        "#,
        project_id,
        extension
    )
    .execute(pool)
    .await
    .context("Failed to permanently delete extension")?;

    Ok(())
}
