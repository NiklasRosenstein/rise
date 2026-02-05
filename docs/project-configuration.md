# Project Configuration with rise.toml

Rise supports declarative project configuration via `rise.toml` (or `.rise.toml`). This allows you to define project metadata, build settings, environment variables, custom domains, and service accounts in a single configuration file.

## Basic Structure

A `rise.toml` file has three main sections:

```toml
version = 1

[project]
# Project metadata

[build]
# Build configuration
```

## Project Metadata

The `[project]` section defines project-level settings that can be synchronized to the backend using `rise project update --sync`.

### Basic Settings

```toml
[project]
name = "my-app"
access_class = "public"  # or "private"
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Project name (required) |
| `access_class` | String | Access level: `public` or `private` (default: `public`) |

### Custom Domains

Define custom domains for your project:

```toml
[project]
name = "my-app"
custom_domains = ["example.com", "www.example.com"]
```

### Environment Variables

Define non-secret environment variables (plain-text only):

```toml
[project]
name = "my-app"

[project.env]
NODE_ENV = "production"
API_BASE_URL = "https://api.example.com"
LOG_LEVEL = "info"
```

**Important**: Only non-secret, non-retrievable environment variables should be defined in `rise.toml`. For secrets, use `rise env set --secret` from the CLI.

### Service Accounts (Workload Identities)

Define service accounts declaratively for CI/CD integration:

```toml
[project]
name = "my-app"

[project.service_accounts.ci]
issuer = "https://token.actions.githubusercontent.com"
claims = { aud = "https://rise.example.com", repo = "owner/my-app" }

[project.service_accounts.staging]
issuer = "https://gitlab.com"
claims = { aud = "https://rise.example.com", project_path = "group/project" }
```

Each service account is defined by an **identifier** (e.g., `ci`, `staging`) and has two required fields:

| Field | Type | Description |
|-------|------|-------------|
| `issuer` | String | OIDC issuer URL (e.g., GitHub, GitLab, custom OIDC provider) |
| `claims` | Object | JWT claims that must match for authentication |

**Identifiers**:
- Must start with a letter or digit
- Can contain lowercase letters, digits, hyphens, and underscores
- Used to generate email addresses: `{project_name}-sa+{identifier}@rise.local`

**Claims**:
- Must include the `aud` (audience) claim
- Must include at least one additional claim (e.g., `repo`, `project_path`, `ref_protected`)
- All specified claims must exactly match the JWT token claims for authentication

#### Common Examples

**GitHub Actions:**
```toml
[project.service_accounts.github-ci]
issuer = "https://token.actions.githubusercontent.com"
claims = { 
  aud = "https://rise.example.com",
  repo = "owner/my-app",
  ref = "refs/heads/main"
}
```

**GitLab CI:**
```toml
[project.service_accounts.gitlab-ci]
issuer = "https://gitlab.com"
claims = {
  aud = "https://rise.example.com",
  project_path = "group/project",
  ref_protected = "true"
}
```

**Google Cloud Workload Identity:**
```toml
[project.service_accounts.gcp-deploy]
issuer = "https://accounts.google.com"
claims = {
  aud = "https://rise.example.com",
  email = "deploy-sa@project.iam.gserviceaccount.com"
}
```

## Complete Example

```toml
version = 1

[project]
name = "my-production-app"
access_class = "private"
custom_domains = ["app.example.com", "www.example.com"]

[project.env]
NODE_ENV = "production"
API_BASE_URL = "https://api.example.com"
ENABLE_ANALYTICS = "true"

[project.service_accounts.github-ci]
issuer = "https://token.actions.githubusercontent.com"
claims = { 
  aud = "https://rise.example.com",
  repo = "myorg/my-production-app",
  ref = "refs/heads/main"
}

[project.service_accounts.staging]
issuer = "https://token.actions.githubusercontent.com"
claims = { 
  aud = "https://rise.example.com",
  repo = "myorg/my-production-app",
  ref = "refs/heads/staging"
}

[build]
backend = "docker"
dockerfile = "Dockerfile.prod"
```

## Synchronizing Configuration

Use `rise project update --sync` to synchronize your `rise.toml` configuration to the backend:

```bash
# Sync all project metadata from rise.toml
rise project update my-app --sync

# This will:
# - Update project name and access_class
# - Add missing custom domains
# - Set/update environment variables
# - Create/update service accounts
```

### What Gets Synced

When using `--sync`, Rise will:

1. **Update project metadata**: name and access_class
2. **Sync custom domains**: Add domains defined in `rise.toml` (warns about unmanaged domains)
3. **Sync environment variables**: Set/update non-secret env vars (warns about unmanaged variables)
4. **Sync service accounts**: Create/update service accounts with identifiers (warns about unmanaged accounts)

### Warnings for Unmanaged Resources

Resources not defined in `rise.toml` are considered "unmanaged" and will generate warnings:

```
⚠️  WARN: Domain 'old-domain.com' exists in backend but not in rise.toml.
         This domain is not managed by rise.toml.
         Run 'rise domain remove my-app old-domain.com' to remove it.

⚠️  WARN: Service account 'old-ci' exists in backend but not in rise.toml.
         This service account is not managed by rise.toml.
         Run 'rise service-account delete my-app old-ci' to remove it.
```

These warnings help identify resources that were created manually and may need cleanup.

### Imperative vs Declarative

You can mix imperative and declarative management:

**Declarative (via rise.toml)**:
- Service accounts with identifiers (managed by `rise.toml`)
- Environment variables defined in `[project.env]`
- Custom domains in `custom_domains` array

**Imperative (via CLI)**:
- Service accounts without identifiers (created via `rise service-account create`)
- Secret environment variables (created via `rise env set --secret`)
- Additional custom domains not in `rise.toml`

Resources created imperatively will not be deleted by `--sync` but will show warnings to help you identify them.

## Best Practices

1. **Use rise.toml for non-secret configuration**: Keep non-sensitive project metadata, domains, and service accounts in version control
2. **Use CLI for secrets**: Never put secrets in `rise.toml` - use `rise env set --secret` instead
3. **Service account identifiers**: Use descriptive names like `github-ci`, `staging-deploy`, or `production-deploy`
4. **Regular syncs**: Run `rise project update --sync` after updating `rise.toml` to keep your backend in sync
5. **Review warnings**: Pay attention to warnings about unmanaged resources - they may indicate orphaned configuration

## See Also

- [Build Configuration](builds.md) - Detailed build settings for `[build]` section
- [CLI Reference](cli.md) - Command-line interface documentation
- [Service Accounts](authentication.md) - Authentication and authorization details
