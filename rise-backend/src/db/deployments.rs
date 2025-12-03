use sqlx::PgPool;
use uuid::Uuid;
use anyhow::{Result, Context};

use crate::db::models::{Deployment, DeploymentStatus};

/// List deployments for a project
pub async fn list_for_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        FROM deployments
        WHERE project_id = $1
        ORDER BY created_at DESC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list deployments for project")?;

    Ok(deployments)
}

/// Find deployment by deployment_id and project_id
pub async fn find_by_deployment_id(pool: &PgPool, deployment_id: &str, project_id: Uuid) -> Result<Option<Deployment>> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        FROM deployments
        WHERE deployment_id = $1 AND project_id = $2
        "#,
        deployment_id,
        project_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find deployment by deployment_id")?;

    Ok(deployment)
}

/// Find deployment by UUID
pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Deployment>> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        FROM deployments
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find deployment by ID")?;

    Ok(deployment)
}

/// Create a new deployment
pub async fn create(
    pool: &PgPool,
    deployment_id: &str,
    project_id: Uuid,
    created_by_id: Uuid,
    status: DeploymentStatus,
) -> Result<Deployment> {
    let status_str = status.to_string();

    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        INSERT INTO deployments (deployment_id, project_id, created_by_id, status)
        VALUES ($1, $2, $3, $4)
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        "#,
        deployment_id,
        project_id,
        created_by_id,
        status_str
    )
    .fetch_one(pool)
    .await
    .context("Failed to create deployment")?;

    Ok(deployment)
}

/// Update deployment status
pub async fn update_status(pool: &PgPool, id: Uuid, status: DeploymentStatus) -> Result<Deployment> {
    let status_str = status.to_string();

    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET status = $2
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        "#,
        id,
        status_str
    )
    .fetch_one(pool)
    .await
    .context("Failed to update deployment status")?;

    Ok(deployment)
}

/// Mark deployment as completed
pub async fn mark_completed(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET status = 'Completed', completed_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as completed")?;

    Ok(deployment)
}

/// Mark deployment as failed
pub async fn mark_failed(pool: &PgPool, id: Uuid, error_message: &str) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET status = 'Failed', completed_at = NOW(), error_message = $2
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        "#,
        id,
        error_message
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as failed")?;

    Ok(deployment)
}

/// Update deployment build logs
pub async fn update_build_logs(pool: &PgPool, id: Uuid, build_logs: &str) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET build_logs = $2
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        "#,
        id,
        build_logs
    )
    .fetch_one(pool)
    .await
    .context("Failed to update deployment build logs")?;

    Ok(deployment)
}

/// Get latest deployment for a project
pub async fn get_latest_for_project(pool: &PgPool, project_id: Uuid) -> Result<Option<Deployment>> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            completed_at, error_message, build_logs,
            created_at, updated_at
        FROM deployments
        WHERE project_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        project_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get latest deployment")?;

    Ok(deployment)
}
