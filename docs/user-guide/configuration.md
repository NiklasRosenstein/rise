# Project Configuration

Rise projects are configured through a `rise.toml` file in your project directory and through CLI flags.

## rise.toml

The `rise.toml` file defines your project metadata and build settings. Both `rise.toml` and `.rise.toml` are supported — if both exist, `rise.toml` takes precedence (with a warning).

A [JSON Schema](https://rise.example.com/api/v1/schema/rise-toml/v1) is available for editor auto-completion and validation. To enable it in editors that support the [Taplo](https://taplo.tamasfe.dev/) TOML language server, add this comment as the first line of your `rise.toml`:

```toml
#:schema https://rise.example.com/api/v1/schema/rise-toml/v1
```

### `[project]` Section

```toml
[project]
name = "my-app"
access_class = "public"
custom_domains = ["myapp.example.com"]

[project.env]
LOG_LEVEL = "info"
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Project name (used for URLs, registry paths, and as default for `-p` flag) |
| `access_class` | String | Access class: `public` or `private` (default: `public`) |
| `custom_domains` | Array | Custom domains for the project |
| `env` | Object | Plain-text environment variables (set on backend during `project create` or `project update --sync`) |

### `[build]` Section

```toml
[build]
backend = "docker"
dockerfile = "Dockerfile.prod"
args = ["NODE_ENV=production", "BUILD_VERSION"]
```

| Field | Type | Description |
|-------|------|-------------|
| `backend` | String | Build backend: `docker`, `docker:build`, `docker:buildx`, `buildctl`, `docker:buildctl`, `pack`, `railpack`, `railpack:buildctl` |
| `dockerfile` | String | Path to Dockerfile, relative to `rise.toml` (default: `Dockerfile` or `Containerfile`) |
| `build_context` | String | Default build context path for Docker builds, relative to `rise.toml` |
| `build_contexts` | Object | Named build contexts for multi-stage Docker builds (format: `{ "name" = "path" }`) |
| `builder` | String | Buildpack builder image (pack backend only) |
| `buildpacks` | Array | Buildpacks to use (pack backend only) |
| `args` | Array | Build-time arguments (format: `KEY=VALUE` or `KEY` to read from shell). Alias: `env` for backward compat. |
| `container_cli` | String | Container CLI: `docker` or `podman` |
| `managed_buildkit` | Boolean | Enable/disable managed BuildKit daemon (auto-enables when `SSL_CERT_FILE` is set) |
| `no_cache` | Boolean | Disable build cache |

### `[environments.<name>]` Section

Define per-environment settings. Currently supports environment-scoped variables. Environments must already exist on the backend (create them with `rise environment create`).

```toml
[environments.staging.env]
DATABASE_URL = "postgres://staging-db/mydb"
LOG_LEVEL = "debug"

[environments.production.env]
DATABASE_URL = "postgres://prod-db/mydb"
```

| Field | Type | Description |
|-------|------|-------------|
| `env` | Object | Plain-text environment variables scoped to this environment |

See [Environment Variables](environment-variables.md#per-environment-variables-in-risetoml) for details.

### Full Example

```toml
[project]
name = "my-app"
access_class = "private"
custom_domains = ["myapp.example.com", "api.example.com"]

[project.env]
LOG_LEVEL = "info"
APP_MODE = "production"

[build]
backend = "pack"
builder = "heroku/builder:24"
buildpacks = ["heroku/nodejs", "heroku/procfile"]
args = ["BP_NODE_VERSION=20"]

[environments.staging.env]
DATABASE_URL = "postgres://staging-db/mydb"
LOG_LEVEL = "debug"

[environments.production.env]
DATABASE_URL = "postgres://prod-db/mydb"
```

## Project Creation Modes

When creating a project, you can control where configuration is stored:

```bash
# Auto-detect: remote if rise.toml exists, remote+local otherwise
rise project create my-app

# Backend only (no rise.toml created)
rise project create my-app --mode remote

# rise.toml only (no backend interaction)
rise project create my-app --mode local

# Both backend and rise.toml
rise project create my-app --mode remote+local
```

If a `rise.toml` already exists, `rise project create` reads the project name from it and defaults to `--mode remote`.

## Syncing Configuration

Push your `rise.toml` settings (name, access class, custom domains, env vars) to the backend:

```bash
rise project update --sync
```

This reads the current `rise.toml` and updates the backend project to match.

## Configuration Precedence

Settings are resolved in this order (highest to lowest priority):

1. **CLI flags** (e.g., `--backend pack`)
2. **Project config file** (`rise.toml` / `.rise.toml`)
3. **Environment variables** (e.g., `RISE_CONTAINER_CLI`, `RISE_MANAGED_BUILDKIT`)
4. **Global config** (`~/.config/rise/config.json`)
5. **Auto-detection / defaults**

For array fields (`buildpacks`, `args`), CLI values are **appended** to config file values rather than replacing them.

## Global CLI Config

The CLI stores global configuration in `~/.config/rise/config.json`, including:

- Authentication token (set by `rise login`)
- Backend URL
- Container CLI preference (`docker` or `podman`)
- Managed BuildKit setting

This file is created automatically on first `rise login`.
