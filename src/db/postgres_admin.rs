use anyhow::{Context, Result};
use sqlx::PgPool;

/// Check if a PostgreSQL database exists
pub async fn database_exists(pool: &PgPool, database_name: &str) -> Result<bool> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(database_name)
            .fetch_one(pool)
            .await
            .context("Failed to check if database exists")?;

    Ok(exists)
}

/// Create a PostgreSQL database with a specific owner
///
/// Note: Database and owner names must be sanitized before calling this function
/// to prevent SQL injection
pub async fn create_database(pool: &PgPool, database_name: &str, owner: &str) -> Result<()> {
    let create_sql = format!("CREATE DATABASE {} OWNER {}", database_name, owner);

    sqlx::query(&create_sql)
        .execute(pool)
        .await
        .context("Failed to create database")?;

    Ok(())
}

/// Create a PostgreSQL database from a template with a specific owner
///
/// Note: Database and owner names must be sanitized before calling this function
/// to prevent SQL injection
pub async fn create_database_from_template(
    pool: &PgPool,
    database_name: &str,
    template_name: &str,
    owner: &str,
) -> Result<()> {
    let create_sql = format!(
        "CREATE DATABASE {} WITH TEMPLATE {} OWNER {}",
        database_name, template_name, owner
    );

    sqlx::query(&create_sql)
        .execute(pool)
        .await
        .context("Failed to create database from template")?;

    Ok(())
}
