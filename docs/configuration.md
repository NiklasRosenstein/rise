# Configuration Guide

Rise backend uses YAML configuration files with environment variable substitution support. TOML is also supported for backward compatibility.

## Configuration Files

Configuration files are located in `rise-backend/config/` and loaded in this order:

1. `default.{toml,yaml,yml}` - Base configuration with sensible defaults
2. `{RISE_CONFIG_RUN_MODE}.{toml,yaml,yml}` - Environment-specific config (optional)
   - `development.toml` or `development.yaml` when `RISE_CONFIG_RUN_MODE=development`
   - `production.toml` or `production.yaml` when `RISE_CONFIG_RUN_MODE=production`
3. `local.{toml,yaml,yml}` - Local overrides (not checked into git)

Later files override earlier ones.

**File Format**: The backend supports both YAML and TOML formats. When multiple formats exist for the same config file (e.g., both `default.yaml` and `default.toml`), TOML takes precedence. YAML is the recommended format as it integrates seamlessly with Kubernetes/Helm deployments.

## Environment Variable Substitution

Configuration values can reference environment variables using the syntax:

```toml
# TOML example
client_secret = "${RISE_AUTH_CLIENT_SECRET:-rise-backend-secret}"
account_id = "${AWS_ACCOUNT_ID}"
public_url = "https://${DOMAIN_NAME}:${PORT}"
```

```yaml
# YAML example
auth:
  client_secret: "${RISE_AUTH_CLIENT_SECRET:-rise-backend-secret}"
registry:
  account_id: "${AWS_ACCOUNT_ID}"
server:
  public_url: "https://${DOMAIN_NAME}:${PORT}"
```

### Syntax

- `${VAR_NAME}` - Use environment variable `VAR_NAME`, error if not set
- `${VAR_NAME:-default}` - Use `VAR_NAME` if set, otherwise use `default`

### How It Works

1. Configuration files are parsed as TOML or YAML
2. String values are scanned for `${...}` patterns
3. Patterns are replaced with environment variable values
4. Resulting configuration is deserialized into Settings struct

This happens **after** TOML/YAML parsing but **before** deserialization, so:
- ✅ Works in all string values (including nested tables/maps and arrays)
- ✅ Preserves structure and types
- ✅ Clear error messages if required variables are missing

## Configuration Precedence

Configuration is loaded in this order (later values override earlier ones):

1. `default.{toml,yaml,yml}` - Base configuration with defaults
2. `{RISE_CONFIG_RUN_MODE}.{toml,yaml,yml}` - Environment-specific (e.g., production.yaml)
3. `local.{toml,yaml,yml}` - Local overrides (not in git)
4. Environment variable substitution - `${VAR}` patterns are replaced
5. DATABASE_URL special case - Overrides `[database] url` if set

**Note**: When multiple file formats exist for the same config file, TOML takes precedence over YAML.

Example (TOML):
```toml
# In default.toml
client_secret = "${AUTH_SECRET:-default-secret}"

# In production.toml
client_secret = "${AUTH_SECRET}"  # Override: no default, required

# In local.toml
client_secret = "my-local-secret"  # Override: hardcoded value
```

Example (YAML):
```yaml
# In default.yaml
auth:
  client_secret: "${AUTH_SECRET:-default-secret}"

# In production.yaml (overrides default.yaml)
auth:
  client_secret: "${AUTH_SECRET}"  # No default, required
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

### Production with Environment Variables (TOML)

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

### Production with Environment Variables (YAML)

```yaml
# production.yaml - ideal for Kubernetes/Helm deployments
server:
  host: "0.0.0.0"
  port: "${PORT:-3000}"
  public_url: "${PUBLIC_URL}"  # Required, no default
  cookie_secure: true

auth:
  issuer: "${DEX_ISSUER}"
  client_id: "${OIDC_CLIENT_ID}"
  client_secret: "${OIDC_CLIENT_SECRET}"
  admin_users:
    - "${ADMIN_EMAIL}"

database:
  url: "${DATABASE_URL}"

registry:
  type: "ecr"
  region: "${AWS_REGION:-us-east-1}"
  account_id: "${AWS_ACCOUNT_ID}"
  role_arn: "${ECR_CONTROLLER_ROLE_ARN}"
  push_role_arn: "${ECR_PUSH_ROLE_ARN}"
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
ECR_CONTROLLER_ROLE_ARN=arn:aws:iam::123456789012:policy/rise-backend
ECR_PUSH_ROLE_ARN=arn:aws:iam::123456789012:role/rise-backend-ecr-push
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
