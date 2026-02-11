use anyhow::{Context, Result};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{DeploymentEnvVar, ProjectEnvVar};

/// List all environment variables for a project
pub async fn list_project_env_vars(pool: &PgPool, project_id: Uuid) -> Result<Vec<ProjectEnvVar>> {
    let env_vars = sqlx::query_as!(
        ProjectEnvVar,
        r#"
        SELECT id, project_id, key, value, is_secret, created_at, updated_at
        FROM project_env_vars
        WHERE project_id = $1
        ORDER BY key ASC
        "#,
        project_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list project environment variables")?;

    Ok(env_vars)
}

/// Create or update a project environment variable (upsert)
pub async fn upsert_project_env_var(
    pool: &PgPool,
    project_id: Uuid,
    key: &str,
    value: &str,
    is_secret: bool,
) -> Result<ProjectEnvVar> {
    let env_var = sqlx::query_as!(
        ProjectEnvVar,
        r#"
        INSERT INTO project_env_vars (project_id, key, value, is_secret)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (project_id, key)
        DO UPDATE SET
            value = EXCLUDED.value,
            is_secret = EXCLUDED.is_secret,
            updated_at = NOW()
        RETURNING id, project_id, key, value, is_secret, created_at, updated_at
        "#,
        project_id,
        key,
        value,
        is_secret
    )
    .fetch_one(pool)
    .await
    .context("Failed to upsert project environment variable")?;

    Ok(env_var)
}

/// Delete a project environment variable by key
pub async fn delete_project_env_var(pool: &PgPool, project_id: Uuid, key: &str) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM project_env_vars
        WHERE project_id = $1 AND key = $2
        "#,
        project_id,
        key
    )
    .execute(pool)
    .await
    .context("Failed to delete project environment variable")?;

    Ok(result.rows_affected() > 0)
}

/// Copy all project environment variables to a deployment
/// This creates a snapshot of the project's env vars at deployment creation time
pub async fn copy_project_env_vars_to_deployment(
    pool: &PgPool,
    project_id: Uuid,
    deployment_id: Uuid,
) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        INSERT INTO deployment_env_vars (deployment_id, key, value, is_secret)
        SELECT $1, key, value, is_secret
        FROM project_env_vars
        WHERE project_id = $2
        "#,
        deployment_id,
        project_id
    )
    .execute(pool)
    .await
    .context("Failed to copy project environment variables to deployment")?;

    Ok(result.rows_affected())
}

/// Copy all environment variables from one deployment to another
/// This is used when creating a deployment from an existing deployment (e.g., redeploy with same env vars)
pub async fn copy_deployment_env_vars_to_deployment(
    pool: &PgPool,
    source_deployment_id: Uuid,
    target_deployment_id: Uuid,
) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        INSERT INTO deployment_env_vars (deployment_id, key, value, is_secret)
        SELECT $1, key, value, is_secret
        FROM deployment_env_vars
        WHERE deployment_id = $2
        "#,
        target_deployment_id,
        source_deployment_id
    )
    .execute(pool)
    .await
    .context("Failed to copy deployment environment variables to deployment")?;

    Ok(result.rows_affected())
}

/// List all environment variables for a deployment
/// This is used by the controller to get env vars for container injection
pub async fn list_deployment_env_vars(
    pool: &PgPool,
    deployment_id: Uuid,
) -> Result<Vec<DeploymentEnvVar>> {
    let env_vars = sqlx::query_as!(
        DeploymentEnvVar,
        r#"
        SELECT id, deployment_id, key, value, is_secret, created_at, updated_at
        FROM deployment_env_vars
        WHERE deployment_id = $1
        ORDER BY key ASC
        "#,
        deployment_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to list deployment environment variables")?;

    Ok(env_vars)
}

/// Create or update a deployment environment variable (upsert)
pub async fn upsert_deployment_env_var(
    pool: &PgPool,
    deployment_id: Uuid,
    key: &str,
    value: &str,
    is_secret: bool,
) -> Result<DeploymentEnvVar> {
    let env_var = sqlx::query_as!(
        DeploymentEnvVar,
        r#"
        INSERT INTO deployment_env_vars (deployment_id, key, value, is_secret)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (deployment_id, key)
        DO UPDATE SET
            value = EXCLUDED.value,
            is_secret = EXCLUDED.is_secret,
            updated_at = NOW()
        RETURNING id, deployment_id, key, value, is_secret, created_at, updated_at
        "#,
        deployment_id,
        key,
        value,
        is_secret
    )
    .fetch_one(pool)
    .await
    .context("Failed to upsert deployment environment variable")?;

    Ok(env_var)
}

/// Load deployment environment variables with decryption
///
/// This is a shared helper for controllers to load environment variables
/// with automatic decryption of secrets using the provided encryption provider.
///
/// Returns a vector of (key, value) tuples that can be formatted by the caller
/// as needed (e.g., KEY=VALUE for Docker, EnvVar objects for Kubernetes).
#[cfg(feature = "backend")]
pub async fn load_deployment_env_vars_decrypted(
    pool: &PgPool,
    deployment_id: Uuid,
    encryption_provider: Option<&dyn crate::server::encryption::EncryptionProvider>,
) -> Result<Vec<(String, String)>> {
    // Fetch deployment environment variables from database
    let db_env_vars = list_deployment_env_vars(pool, deployment_id).await?;

    let mut env_vars = Vec::new();

    for var in db_env_vars {
        let value = if var.is_secret {
            // Decrypt secret values
            match encryption_provider {
                Some(provider) => provider
                    .decrypt(&var.value)
                    .await
                    .with_context(|| format!("Failed to decrypt secret variable '{}'", var.key))?,
                None => {
                    // This should not happen - secrets should only be stored with encryption enabled
                    tracing::error!(
                        "Encountered secret variable '{}' but no encryption provider configured",
                        var.key
                    );
                    return Err(anyhow::anyhow!(
                        "Cannot decrypt secret variable '{}': no encryption provider",
                        var.key
                    ));
                }
            }
        } else {
            // Plain text value
            var.value
        };

        env_vars.push((var.key, value));
    }

    Ok(env_vars)
}
