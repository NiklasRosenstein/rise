use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{Deployment, DeploymentStatus, TerminationReason};
use crate::deployment::state_machine;

/// List deployments for a project
pub async fn list_for_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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
pub async fn find_by_deployment_id(
    pool: &PgPool,
    deployment_id: &str,
    project_id: Uuid,
) -> Result<Option<Deployment>> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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
    image: Option<&str>,
    image_digest: Option<&str>,
    deployment_group: &str,
    expires_at: Option<DateTime<Utc>>,
    http_port: i32,
) -> Result<Deployment> {
    let status_str = status.to_string();

    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        INSERT INTO deployments (deployment_id, project_id, created_by_id, status, image, image_digest, deployment_group, expires_at, http_port)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
                        deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        deployment_id,
        project_id,
        created_by_id,
        status_str,
        image,
        image_digest,
        deployment_group,
        expires_at,
        http_port
    )
    .fetch_one(pool)
    .await
    .context("Failed to create deployment")?;

    Ok(deployment)
}

/// Update deployment status
pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    status: DeploymentStatus,
) -> Result<Deployment> {
    let status_str = status.to_string();

    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET status = $2
        WHERE id = $1
          AND NOT is_protected(status)
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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

/// Mark deployment as failed
pub async fn mark_failed(pool: &PgPool, id: Uuid, error_message: &str) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET status = 'Failed', completed_at = NOW(), error_message = $2, deployment_url = NULL
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
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

/// Find deployments in non-terminal states for reconciliation
/// Excludes terminal states using is_terminal() PostgreSQL function
pub async fn find_non_terminal(pool: &PgPool, limit: i64) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
            created_at, updated_at
        FROM deployments
        WHERE NOT is_terminal(status)
        ORDER BY updated_at ASC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to find non-terminal deployments")?;

    Ok(deployments)
}

/// Find all non-terminal deployments for a specific project
pub async fn find_non_terminal_for_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
            created_at, updated_at
        FROM deployments
        WHERE project_id = $1
          AND NOT is_terminal(status)
        ORDER BY created_at DESC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to find non-terminal deployments for project")?;

    Ok(deployments)
}

/// Find all deployments with a specific status
pub async fn find_by_status(pool: &PgPool, status: DeploymentStatus) -> Result<Vec<Deployment>> {
    let status_str = status.to_string();

    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
            created_at, updated_at
        FROM deployments
        WHERE status = $1
        ORDER BY created_at DESC
        "#,
        status_str
    )
    .fetch_all(pool)
    .await
    .context("Failed to find deployments by status")?;

    Ok(deployments)
}

/// Update controller metadata
pub async fn update_controller_metadata(
    pool: &PgPool,
    id: Uuid,
    metadata: &serde_json::Value,
) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET controller_metadata = $2
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
            created_at, updated_at
        "#,
        id,
        metadata
    )
    .fetch_one(pool)
    .await
    .context("Failed to update controller metadata")?;

    Ok(deployment)
}

/// Update deployment URL
pub async fn update_deployment_url(pool: &PgPool, id: Uuid, url: &str) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET deployment_url = $2
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            termination_reason as "termination_reason: _",
            created_at, updated_at
        "#,
        id,
        url
    )
    .fetch_one(pool)
    .await
    .context("Failed to update deployment URL")?;

    Ok(deployment)
}

/// Find deployment by project_id and deployment_id (for CLI commands)
pub async fn find_by_project_and_deployment_id(
    pool: &PgPool,
    project_id: Uuid,
    deployment_id: &str,
) -> Result<Option<Deployment>> {
    // This is the same as find_by_deployment_id, but with explicit naming for CLI use
    find_by_deployment_id(pool, deployment_id, project_id).await
}

/// Mark deployment as cancelled
pub async fn mark_cancelled(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Cancelled',
            termination_reason = 'Cancelled',
            deployment_url = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as cancelled")?;

    Ok(deployment)
}

/// Mark deployment as stopped (user-initiated termination)
pub async fn mark_stopped(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Stopped',
            termination_reason = 'UserStopped',
            deployment_url = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as stopped")?;

    Ok(deployment)
}

/// Mark deployment as superseded (replaced by newer deployment)
pub async fn mark_superseded(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Superseded',
            termination_reason = 'Superseded',
            deployment_url = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as superseded")?;

    Ok(deployment)
}

/// Mark a deployment as expired (terminal state for deployments that timed out)
pub async fn mark_expired(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Expired',
            termination_reason = 'Expired',
            deployment_url = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as expired")?;

    Ok(deployment)
}

/// Mark deployment as healthy
pub async fn mark_healthy(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Healthy',
            error_message = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as healthy")?;

    Ok(deployment)
}

/// Mark deployment as unhealthy with reason
pub async fn mark_unhealthy(pool: &PgPool, id: Uuid, reason: String) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Unhealthy',
            error_message = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id,
        reason
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as unhealthy")?;

    Ok(deployment)
}

/// Mark deployment as terminating with reason
pub async fn mark_terminating(
    pool: &PgPool,
    id: Uuid,
    reason: TerminationReason,
) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Terminating',
            termination_reason = $2,
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id,
        reason as TerminationReason
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as terminating")?;

    Ok(deployment)
}

/// Mark deployment as cancelling
pub async fn mark_cancelling(pool: &PgPool, id: Uuid) -> Result<Deployment> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        UPDATE deployments
        SET
            status = 'Cancelling',
            termination_reason = 'Cancelled',
            updated_at = NOW()
        WHERE id = $1
        RETURNING
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark deployment as cancelling")?;

    Ok(deployment)
}

/// Find all cancellable deployments for a project (for auto-cancellation)
pub async fn find_cancellable_for_project(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
            deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        FROM deployments
        WHERE project_id = $1
          AND is_cancellable(status)
        ORDER BY created_at DESC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to find cancellable deployments")?;

    Ok(deployments)
}

/// Update deployment status with transition validation
pub async fn update_status_checked(
    pool: &PgPool,
    id: Uuid,
    new_status: DeploymentStatus,
) -> Result<Deployment> {
    // Get current deployment
    let current = find_by_id(pool, id)
        .await?
        .context("Deployment not found")?;

    // Validate transition
    state_machine::validate_transition(&current.status, &new_status)?;

    // Update status
    update_status(pool, id, new_status).await
}

/// Find active deployment for a project in a specific group
/// Active = most recent Healthy deployment in the group
pub async fn find_active_for_project_and_group(
    pool: &PgPool,
    project_id: Uuid,
    group: &str,
) -> Result<Option<Deployment>> {
    let deployment = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
                        deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        FROM deployments
        WHERE project_id = $1
          AND deployment_group = $2
          AND status = 'Healthy'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        project_id,
        group
    )
    .fetch_optional(pool)
    .await
    .context("Failed to find active deployment for project and group")?;

    Ok(deployment)
}

/// Find non-terminal deployments for a project in a specific group
pub async fn find_non_terminal_for_project_and_group(
    pool: &PgPool,
    project_id: Uuid,
    group: &str,
) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
                        deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        FROM deployments
        WHERE project_id = $1
          AND deployment_group = $2
          AND NOT is_terminal(status)
        ORDER BY created_at DESC
        "#,
        project_id,
        group
    )
    .fetch_all(pool)
    .await
    .context("Failed to find non-terminal deployments for project and group")?;

    Ok(deployments)
}

/// Find expired deployments that need cleanup
pub async fn find_expired(pool: &PgPool, limit: i64) -> Result<Vec<Deployment>> {
    let deployments = sqlx::query_as!(
        Deployment,
        r#"
        SELECT
            id, deployment_id, project_id, created_by_id,
            status as "status: DeploymentStatus",
                        deployment_group, expires_at,
            termination_reason as "termination_reason: _",
            completed_at, error_message, build_logs,
            controller_metadata as "controller_metadata: serde_json::Value",
            deployment_url,
            image, image_digest,
            http_port,
            created_at, updated_at
        FROM deployments
        WHERE expires_at IS NOT NULL
          AND expires_at <= NOW()
          AND NOT is_terminal(status)
        ORDER BY expires_at ASC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to find expired deployments")?;

    Ok(deployments)
}

/// List deployments for a project with optional group filter
pub async fn list_for_project_and_group(
    pool: &PgPool,
    project_id: Uuid,
    group: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Deployment>> {
    let limit_value = limit.unwrap_or(10);
    let offset_value = offset.unwrap_or(0);

    let deployments = if let Some(g) = group {
        sqlx::query_as!(
            Deployment,
            r#"
            SELECT
                id, deployment_id, project_id, created_by_id,
                status as "status: DeploymentStatus",
                deployment_group, expires_at,
                termination_reason as "termination_reason: _",
                completed_at, error_message, build_logs,
                controller_metadata as "controller_metadata: serde_json::Value",
                deployment_url,
                image, image_digest,
                http_port,
                created_at, updated_at
            FROM deployments
            WHERE project_id = $1 AND deployment_group = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            project_id,
            g,
            limit_value,
            offset_value
        )
        .fetch_all(pool)
        .await?
    } else {
        // No group filter - return all for project with pagination
        sqlx::query_as!(
            Deployment,
            r#"
            SELECT
                id, deployment_id, project_id, created_by_id,
                status as "status: DeploymentStatus",
                deployment_group, expires_at,
                termination_reason as "termination_reason: _",
                completed_at, error_message, build_logs,
                controller_metadata as "controller_metadata: serde_json::Value",
                deployment_url,
                image, image_digest,
                http_port,
                created_at, updated_at
            FROM deployments
            WHERE project_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            project_id,
            limit_value,
            offset_value
        )
        .fetch_all(pool)
        .await?
    };

    Ok(deployments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment::state_machine;

    fn str_to_status(s: &str) -> DeploymentStatus {
        match s {
            "Pending" => DeploymentStatus::Pending,
            "Building" => DeploymentStatus::Building,
            "Pushing" => DeploymentStatus::Pushing,
            "Pushed" => DeploymentStatus::Pushed,
            "Deploying" => DeploymentStatus::Deploying,
            "Healthy" => DeploymentStatus::Healthy,
            "Unhealthy" => DeploymentStatus::Unhealthy,
            "Cancelling" => DeploymentStatus::Cancelling,
            "Cancelled" => DeploymentStatus::Cancelled,
            "Terminating" => DeploymentStatus::Terminating,
            "Stopped" => DeploymentStatus::Stopped,
            "Superseded" => DeploymentStatus::Superseded,
            "Failed" => DeploymentStatus::Failed,
            "Expired" => DeploymentStatus::Expired,
            _ => panic!("Unknown status: {}", s),
        }
    }

    /// Test that PostgreSQL is_terminal() function matches Rust is_terminal() function
    #[sqlx::test]
    async fn db_is_terminal_matches_rust_is_terminal(pool: PgPool) {
        // Test all deployment statuses
        let statuses = vec![
            ("Pending", false),
            ("Building", false),
            ("Pushing", false),
            ("Pushed", false),
            ("Deploying", false),
            ("Healthy", false),
            ("Unhealthy", false),
            ("Cancelling", false),
            ("Terminating", false),
            ("Cancelled", true),
            ("Stopped", true),
            ("Superseded", true),
            ("Failed", true),
            ("Expired", true),
        ];

        for (status_str, expected) in statuses {
            // Test PostgreSQL function
            let result: bool = sqlx::query_scalar("SELECT is_terminal($1)")
                .bind(status_str)
                .fetch_one(&pool)
                .await
                .unwrap();

            assert_eq!(
                result, expected,
                "is_terminal({}) returned {} but expected {}",
                status_str, result, expected
            );

            // Also verify Rust function matches
            let status = str_to_status(status_str);
            assert_eq!(
                state_machine::is_terminal(&status),
                expected,
                "Rust is_terminal mismatch for {}",
                status_str
            );
        }
    }

    /// Test that PostgreSQL is_cancellable() function matches Rust is_cancellable() function
    #[sqlx::test]
    async fn db_is_cancellable_matches_rust_is_cancellable(pool: PgPool) {
        let statuses = vec![
            ("Pending", true),
            ("Building", true),
            ("Pushing", true),
            ("Pushed", true),
            ("Deploying", true),
            ("Healthy", false),
            ("Unhealthy", false),
            ("Cancelling", false),
            ("Terminating", false),
            ("Cancelled", false),
            ("Stopped", false),
            ("Superseded", false),
            ("Failed", false),
            ("Expired", false),
        ];

        for (status_str, expected) in statuses {
            // Test PostgreSQL function
            let result: bool = sqlx::query_scalar("SELECT is_cancellable($1)")
                .bind(status_str)
                .fetch_one(&pool)
                .await
                .unwrap();

            assert_eq!(
                result, expected,
                "is_cancellable({}) returned {} but expected {}",
                status_str, result, expected
            );

            // Also verify Rust function matches
            let status = str_to_status(status_str);
            assert_eq!(
                state_machine::is_cancellable(&status),
                expected,
                "Rust is_cancellable mismatch for {}",
                status_str
            );
        }
    }

    /// Test that PostgreSQL is_active() function matches Rust is_active() function
    #[sqlx::test]
    async fn db_is_active_matches_rust_is_active(pool: PgPool) {
        let statuses = vec![
            ("Pending", false),
            ("Building", false),
            ("Pushing", false),
            ("Pushed", false),
            ("Deploying", false),
            ("Healthy", true),
            ("Unhealthy", true),
            ("Cancelling", false),
            ("Terminating", false),
            ("Cancelled", false),
            ("Stopped", false),
            ("Superseded", false),
            ("Failed", false),
            ("Expired", false),
        ];

        for (status_str, expected) in statuses {
            // Test PostgreSQL function
            let result: bool = sqlx::query_scalar("SELECT is_active($1)")
                .bind(status_str)
                .fetch_one(&pool)
                .await
                .unwrap();

            assert_eq!(
                result, expected,
                "is_active({}) returned {} but expected {}",
                status_str, result, expected
            );

            // Also verify Rust function matches
            let status = str_to_status(status_str);
            assert_eq!(
                state_machine::is_active(&status),
                expected,
                "Rust is_active mismatch for {}",
                status_str
            );
        }
    }

    /// Test that PostgreSQL is_protected() function includes all terminal and cleanup states
    #[sqlx::test]
    async fn db_is_protected_includes_terminal_and_cleanup(pool: PgPool) {
        let statuses = vec![
            ("Pending", false),
            ("Building", false),
            ("Pushing", false),
            ("Pushed", false),
            ("Deploying", false),
            ("Healthy", false),
            ("Unhealthy", false),
            ("Cancelling", true),  // Cleanup state
            ("Terminating", true), // Cleanup state
            ("Cancelled", true),   // Terminal
            ("Stopped", true),     // Terminal
            ("Superseded", true),  // Terminal
            ("Failed", true),      // Terminal
            ("Expired", true),     // Terminal
        ];

        for (status_str, expected) in statuses {
            let result: bool = sqlx::query_scalar("SELECT is_protected($1)")
                .bind(status_str)
                .fetch_one(&pool)
                .await
                .unwrap();

            assert_eq!(
                result, expected,
                "is_protected({}) returned {} but expected {}",
                status_str, result, expected
            );
        }
    }
}
