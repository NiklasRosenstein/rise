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

/// Check if a PostgreSQL role/user exists
pub async fn user_exists(pool: &PgPool, username: &str) -> Result<bool> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(username)
            .fetch_one(pool)
            .await
            .context("Failed to check if user exists")?;

    Ok(exists)
}

/// Create a PostgreSQL user with a password and CREATEDB privilege
///
/// Note: Username must be sanitized before calling this function to prevent SQL injection
/// Password will be properly escaped for SQL
///
/// CREATEDB privilege is required for users to create database copies from databases they own
pub async fn create_user(pool: &PgPool, username: &str, password: &str) -> Result<()> {
    // Escape single quotes in password by doubling them
    let escaped_password = password.replace('\'', "''");
    let create_sql = format!(
        "CREATE USER {} WITH PASSWORD '{}' CREATEDB",
        username, escaped_password
    );

    sqlx::query(&create_sql)
        .execute(pool)
        .await
        .context("Failed to create user")?;

    Ok(())
}

/// Change the owner of a database
///
/// Note: Database and owner must be sanitized before calling this function
pub async fn change_database_owner(
    pool: &PgPool,
    database_name: &str,
    new_owner: &str,
) -> Result<()> {
    let alter_sql = format!("ALTER DATABASE {} OWNER TO {}", database_name, new_owner);

    sqlx::query(&alter_sql)
        .execute(pool)
        .await
        .context("Failed to change database owner")?;

    Ok(())
}
