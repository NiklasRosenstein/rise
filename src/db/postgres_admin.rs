use anyhow::{Context, Result};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tokio::process::Command;
use tracing::{info, warn};

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

/// Create a PostgreSQL user with a password
///
/// Note: Username must be sanitized before calling this function to prevent SQL injection
/// Password will be properly escaped for SQL
pub async fn create_user(pool: &PgPool, username: &str, password: &str) -> Result<()> {
    // Escape single quotes in password by doubling them
    let escaped_password = password.replace('\'', "''");
    let create_sql = format!(
        "CREATE USER {} WITH PASSWORD '{}'",
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

/// Dump a PostgreSQL database to a temporary file using pg_dump
///
/// This function creates a dump of the specified database in PostgreSQL custom format (-Fc)
/// which supports parallel restore and selective restore.
///
/// The dump file is created in a temporary location and must be cleaned up by the caller.
///
/// # Arguments
/// * `host` - Database host (e.g., "localhost" or RDS endpoint)
/// * `port` - Database port (typically 5432)
/// * `database` - Name of the database to dump
/// * `username` - Database username for authentication
/// * `password` - Database password for authentication
///
/// # Returns
/// Path to the temporary dump file
///
/// # Errors
/// Returns an error if pg_dump is not found in PATH or if the dump operation fails
pub async fn dump_database(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
) -> Result<PathBuf> {
    // Create a temporary file for the dump
    let temp_file =
        NamedTempFile::new().context("Failed to create temporary file for database dump")?;

    // Keep the temp file from being deleted (we'll clean it up manually)
    let (_, path) = temp_file
        .keep()
        .context("Failed to persist temporary dump file")?;

    info!(
        "Dumping database '{}' from {}:{} to {:?}",
        database, host, port, path
    );

    // Execute pg_dump with custom format
    let output = Command::new("pg_dump")
        .arg("-Fc") // Custom format (supports parallel restore)
        .arg("-h")
        .arg(host)
        .arg("-p")
        .arg(port.to_string())
        .arg("-U")
        .arg(username)
        .arg("-d")
        .arg(database)
        .arg("-f")
        .arg(&path)
        .env("PGPASSWORD", password)
        .output()
        .await
        .context("Failed to execute pg_dump (is it installed and in PATH?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "pg_dump failed with exit code {:?}: {}",
            output.status.code(),
            stderr
        );
    }

    // Log any warnings from pg_dump
    if !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("pg_dump warnings: {}", stderr);
    }

    info!("Successfully dumped database '{}' to {:?}", database, path);
    Ok(path)
}

/// Restore a PostgreSQL database from a dump file using pg_restore
///
/// This function restores a database from a pg_dump custom format file.
/// The target database must already exist and be empty.
///
/// # Arguments
/// * `host` - Database host (e.g., "localhost" or RDS endpoint)
/// * `port` - Database port (typically 5432)
/// * `database` - Name of the target database (must already exist)
/// * `username` - Database username for authentication
/// * `password` - Database password for authentication
/// * `dump_file` - Path to the pg_dump custom format file
///
/// # Errors
/// Returns an error if pg_restore is not found in PATH or if the restore operation fails
pub async fn restore_database(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
    dump_file: &Path,
) -> Result<()> {
    info!(
        "Restoring database '{}' from {:?} to {}:{}",
        database, dump_file, host, port
    );

    // Execute pg_restore
    let output = Command::new("pg_restore")
        .arg("-h")
        .arg(host)
        .arg("-p")
        .arg(port.to_string())
        .arg("-U")
        .arg(username)
        .arg("-d")
        .arg(database)
        .arg(dump_file)
        .env("PGPASSWORD", password)
        .output()
        .await
        .context("Failed to execute pg_restore (is it installed and in PATH?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "pg_restore failed with exit code {:?}: {}",
            output.status.code(),
            stderr
        );
    }

    // Log any warnings from pg_restore
    if !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("pg_restore warnings: {}", stderr);
    }

    info!("Successfully restored database '{}'", database);
    Ok(())
}
