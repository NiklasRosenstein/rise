# Configuration Guide

Rise backend uses TOML configuration files with environment variable substitution support.

## Configuration Files

Configuration files are located in `rise-backend/config/` and loaded in this order:

1. `default.toml` - Base configuration with sensible defaults
2. `{RUN_MODE}.toml` - Environment-specific config (optional)
   - `development.toml` when `RUN_MODE=development`
   - `production.toml` when `RUN_MODE=production`
3. `local.toml` - Local overrides (not checked into git)

Later files override earlier ones.

## Environment Variable Substitution

Configuration values can reference environment variables using the syntax:

```toml
# Use environment variable, or default if not set
client_secret = "${RISE_AUTH_CLIENT_SECRET:-rise-backend-secret}"

# Use environment variable, error if not set
account_id = "${AWS_ACCOUNT_ID}"

# Multiple variables in one value
public_url = "https://${DOMAIN_NAME}:${PORT}"
```

### Syntax

- `${VAR_NAME}` - Use environment variable `VAR_NAME`, error if not set
- `${VAR_NAME:-default}` - Use `VAR_NAME` if set, otherwise use `default`

### How It Works

1. Configuration files are parsed as TOML
2. String values are scanned for `${...}` patterns
3. Patterns are replaced with environment variable values
4. Resulting configuration is deserialized into Settings struct

This happens **after** TOML parsing but **before** deserialization, so:
- ✅ Works in all string values (including nested tables and arrays)
- ✅ Preserves TOML structure and types
- ✅ Clear error messages if required variables are missing

## Configuration Precedence

Configuration is loaded in this order (later values override earlier ones):

1. `default.toml` - Base configuration with defaults
2. `{RUN_MODE}.toml` - Environment-specific (e.g., production.toml)
3. `local.toml` - Local overrides (not in git)
4. Environment variable substitution - `${VAR}` patterns are replaced
5. DATABASE_URL special case - Overrides `[database] url` if set

Example:
```toml
# In default.toml
client_secret = "${AUTH_SECRET:-default-secret}"

# In production.toml
client_secret = "${AUTH_SECRET}"  # Override: no default, required

# In local.toml
client_secret = "my-local-secret"  # Override: hardcoded value
```

### Special Cases

**DATABASE_URL**: For convenience, the DATABASE_URL environment variable is checked after config loading and will override any `[database] url` setting. This is optional - you can use `${DATABASE_URL}` in TOML instead:

```toml
# Option 1: Direct environment variable (checked after config loads)
[database]
url = ""  # Empty, DATABASE_URL env var will be used

# Option 2: Explicit substitution (recommended for consistency)
[database]
url = "${DATABASE_URL}"
```

**Note**: DATABASE_URL is only required at compile time for SQLX query verification. At runtime, you can set it via either method above.

## Examples

### Development (default.toml)

```toml
[server]
host = "0.0.0.0"
port = 3000
public_url = "http://localhost:3000"

[auth]
issuer = "http://localhost:5556/dex"
client_id = "rise-backend"
client_secret = "${RISE_AUTH_CLIENT_SECRET:-rise-backend-secret}"
```

### Production with Environment Variables

```toml
# production.toml
[server]
host = "0.0.0.0"
port = "${PORT:-3000}"
public_url = "${PUBLIC_URL}"  # Required, no default
cookie_secure = true

[auth]
issuer = "${DEX_ISSUER}"
client_id = "${OIDC_CLIENT_ID}"
client_secret = "${OIDC_CLIENT_SECRET}"  # Required
admin_users = ["${ADMIN_EMAIL}"]

[registry]
type = "ecr"
region = "${AWS_REGION:-us-east-1}"
account_id = "${AWS_ACCOUNT_ID}"
role_arn = "${ECR_CONTROLLER_ROLE_ARN}"
push_role_arn = "${ECR_PUSH_ROLE_ARN}"
```

Environment file:
```bash
# .env
PUBLIC_URL=https://rise.example.com
DEX_ISSUER=https://dex.example.com
OIDC_CLIENT_ID=rise-production
OIDC_CLIENT_SECRET=very-secret-value
ADMIN_EMAIL=admin@example.com
AWS_ACCOUNT_ID=123456789012
ECR_CONTROLLER_ROLE_ARN=arn:aws:iam::123456789012:policy/rise-ecr-controller
ECR_PUSH_ROLE_ARN=arn:aws:iam::123456789012:role/rise-ecr-push
DATABASE_URL=postgres://rise:${DB_PASSWORD}@db.example.com/rise
```

### Local Overrides (local.toml)

For local development, create `local.toml` (not checked into git):

```toml
# Override just what you need
[auth]
client_secret = "my-local-secret"

[registry]
type = "oci-client-auth"
registry_url = "localhost:5000"
```

## Configuration Reference

### Server Settings

```toml
[server]
host = "0.0.0.0"              # Bind address
port = 3000                    # HTTP port
public_url = "http://..."      # Public URL (for OAuth redirects)
cookie_domain = ""             # Cookie domain ("" = current host only)
cookie_secure = false          # Set true for HTTPS
```

### Auth Settings

```toml
[auth]
issuer = "http://..."          # OIDC issuer URL
client_id = "rise-backend"     # OAuth2 client ID
client_secret = "..."          # OAuth2 client secret
admin_users = ["email@..."]    # Admin user emails (array)
```

### Database Settings

```toml
[database]
url = "postgres://..."         # PostgreSQL connection string
                              # Or use DATABASE_URL env var
```

### Registry Settings

#### AWS ECR

```toml
[registry]
type = "ecr"
region = "us-east-1"
account_id = "123456789012"
repo_prefix = "rise/"
role_arn = "arn:aws:iam::..."
push_role_arn = "arn:aws:iam::..."
auto_remove = true
```

#### OCI Registry (Docker, Harbor, Quay)

```toml
[registry]
type = "oci-client-auth"
registry_url = "registry.example.com"
namespace = "rise-apps"
```

### Controller Settings (Optional)

```toml
[controller]
reconcile_interval_secs = 5
health_check_interval_secs = 5
termination_interval_secs = 5
cancellation_interval_secs = 5
expiration_interval_secs = 60
secret_refresh_interval_secs = 3600
```

## Validation

The backend validates configuration on startup:
- Required fields must be set
- Invalid values cause startup failure with clear error messages
- Environment variable substitution errors are reported

Run with `RUST_LOG=debug` to see configuration loading details:

```bash
RUST_LOG=debug cargo run --bin rise -- backend server
```
