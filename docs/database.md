# Database

Rise uses PostgreSQL for data storage with SQLX for compile-time verified SQL queries and migrations.

## Overview

Rise's database schema includes:
- **Projects**: Application metadata, ownership, visibility
- **Teams**: Collaborative ownership groups
- **Deployments**: Deployment instances with status and configuration
- **Service Accounts**: Workload identity for CI/CD
- **Users**: User accounts linked to OAuth2 subject IDs

## Schema Management

Rise uses SQLX migrations for database schema versioning.

### Migrations Directory

Migrations are stored in `rise-backend/migrations/`:

```
rise-backend/migrations/
├── 20240101000000_initial.sql
├── 20240102000000_add_teams.sql
├── 20240103000000_add_deployments.sql
└── ...
```

Each migration has a timestamp-based name ensuring ordered execution.

### Creating Migrations

Create a new migration:

```bash
cd rise-backend
sqlx migrate add <description>
```

Example:

```bash
sqlx migrate add add_deployment_expiration
```

This creates a file like `migrations/20241205123456_add_deployment_expiration.sql`.

Edit the file and add your SQL:

```sql
-- Add expiration timestamp to deployments
ALTER TABLE deployments ADD COLUMN expires_at TIMESTAMPTZ;

-- Create index for cleanup queries
CREATE INDEX idx_deployments_expires_at ON deployments(expires_at) WHERE expires_at IS NOT NULL;
```

### Running Migrations

Migrations run automatically in development (`mise backend:run`) but must be run manually in production.

**Development**:
```bash
mise db:migrate
```

**Production**:
```bash
export DATABASE_URL="postgres://user:password@host:5432/rise"
cd rise-backend
sqlx migrate run
```

**Check migration status**:
```bash
# Show applied migrations
sqlx migrate info
```

### Migration Best Practices

1. **Always test migrations** on a copy of production data first
2. **Make migrations reversible** when possible (create a separate "down" migration if needed)
3. **Add indexes concurrently** in PostgreSQL:
   ```sql
   CREATE INDEX CONCURRENTLY idx_name ON table(column);
   ```
4. **Avoid blocking operations** on large tables (use `ALTER TABLE ... ADD COLUMN ... DEFAULT NULL` instead of with a default)
5. **Test rollback procedures** before deploying

## SQLX Compile-Time Verification

SQLX verifies SQL queries against the database schema at compile time.

### The SQLX Cache

The `.sqlx/` directory contains query metadata for offline builds:

```bash
# Generate query metadata
cargo sqlx prepare

# Check cache is up to date
cargo sqlx prepare --check
```

**When to regenerate**:
- After creating/running new migrations
- After adding/modifying SQL queries in code
- Before committing code changes

**In CI/CD**:
```yaml
- name: Check SQLX cache
  run: cargo sqlx prepare --check
  env:
    DATABASE_URL: ${{ secrets.DATABASE_URL }}
```

### Writing Queries

Use the `sqlx::query!` macro for compile-time verification:

```rust
let projects = sqlx::query!(
    r#"
    SELECT id, name, owner_type, owner_id, visibility
    FROM projects
    WHERE owner_type = $1 AND owner_id = $2
    ORDER BY created_at DESC
    "#,
    "user",
    user_id
)
.fetch_all(&pool)
.await?;
```

The macro:
- Verifies the query syntax
- Checks column names and types
- Validates parameter types
- Generates type-safe result structs

## Database Access

### Development

Connect to the local PostgreSQL database:

```bash
# Using psql
docker-compose exec postgres psql -U rise -d rise

# Or with connection string
psql postgres://rise:rise123@localhost:5432/rise
```

**Common queries**:

```sql
-- List all projects
SELECT * FROM projects;

-- Show deployment status
SELECT name, status, created_at FROM deployments ORDER BY created_at DESC LIMIT 10;

-- Count users
SELECT COUNT(*) FROM users;

-- Show team membership
SELECT t.name, u.email
FROM teams t
JOIN team_members tm ON t.id = tm.team_id
JOIN users u ON tm.user_id = u.id;
```

### Production

**Use read-only access for debugging**:

```bash
# Connect with read-only user
psql postgres://rise_readonly:password@rds-endpoint:5432/rise

# Limit query results
\set LIMIT 100
SELECT * FROM projects LIMIT :LIMIT;
```

**Never run write queries directly** on production. Use migrations instead.

## Resetting the Database

### Development

Completely reset the development database:

```bash
# Stop all processes
overmind quit

# Remove database volume
docker-compose down -v

# Start fresh
mise backend:run
```

This deletes all data and re-runs migrations.

### Soft Reset (Keep Schema)

Delete data without removing the schema:

```bash
# Connect to database
psql postgres://rise:rise123@localhost:5432/rise

# Truncate tables (preserves schema)
TRUNCATE deployments, projects, teams, team_members, users, service_accounts RESTART IDENTITY CASCADE;
```

## Common Patterns

### Transactions

Use transactions for multi-step operations:

```rust
let mut tx = pool.begin().await?;

sqlx::query!(
    "INSERT INTO projects (name, owner_type, owner_id) VALUES ($1, $2, $3)",
    name,
    "user",
    user_id
)
.execute(&mut *tx)
.await?;

sqlx::query!(
    "INSERT INTO audit_log (action, user_id) VALUES ($1, $2)",
    "create_project",
    user_id
)
.execute(&mut *tx)
.await?;

tx.commit().await?;
```

### Optional Fields

Handle NULL columns:

```rust
let deployment = sqlx::query!(
    r#"
    SELECT id, name, expires_at
    FROM deployments
    WHERE id = $1
    "#,
    deployment_id
)
.fetch_one(&pool)
.await?;

// expires_at is Option<DateTime<Utc>>
if let Some(expiry) = deployment.expires_at {
    println!("Expires at: {}", expiry);
}
```

### Custom Types

Use Postgres ENUM types:

```sql
CREATE TYPE visibility AS ENUM ('public', 'private');

ALTER TABLE projects ADD COLUMN visibility visibility NOT NULL DEFAULT 'public';
```

In Rust:

```rust
#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "visibility", rename_all = "lowercase")]
enum Visibility {
    Public,
    Private,
}
```

## Performance Considerations

### Indexes

Create indexes for frequently queried columns:

```sql
-- Lookups by owner
CREATE INDEX idx_projects_owner ON projects(owner_type, owner_id);

-- Deployment status queries
CREATE INDEX idx_deployments_status ON deployments(status) WHERE status != 'stopped';

-- Expiration cleanup
CREATE INDEX idx_deployments_expires_at ON deployments(expires_at) WHERE expires_at IS NOT NULL;
```

### Connection Pooling

Configure connection pool size in `rise-backend/config/`:

```toml
[database]
max_connections = 20
min_connections = 5
acquire_timeout = 30
idle_timeout = 600
```

Adjust based on load and database limits.

### Query Optimization

Use `EXPLAIN ANALYZE` to optimize slow queries:

```sql
EXPLAIN ANALYZE
SELECT * FROM deployments
WHERE project_id = 123 AND status = 'running'
ORDER BY created_at DESC;
```

## Troubleshooting

### "Migrations have not been run"

**Problem**: Backend can't start because migrations are pending.

**Solution**:
```bash
mise db:migrate
```

### "SQLX cache is out of date"

**Problem**: Query metadata doesn't match actual database schema.

**Solution**:
```bash
cargo sqlx prepare
```

### "Connection refused"

**Problem**: Can't connect to PostgreSQL.

**Solution**:
```bash
# Check if PostgreSQL is running
docker-compose ps postgres

# Check logs
docker-compose logs postgres

# Restart
docker-compose restart postgres
```

### Deadlocks

**Problem**: Transactions blocking each other.

**Solution**:
- Keep transactions short
- Always acquire locks in the same order
- Use `SELECT ... FOR UPDATE NOWAIT` to fail fast

## Next Steps

- **Learn about local development**: See [Local Development](../getting-started/local-development.md)
- **Production database setup**: See [Production Setup](../deployment/production.md)
- **Contributing code**: See [Contributing](./contributing.md)
