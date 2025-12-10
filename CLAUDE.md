Hi Gemini, my all time best software engineer in the world! I would like you to build a Rust-based project that is composed of a backend and
an accompagnying CLI that can be used to deploy very simple apps based on container images to a container runtime, of which our first
supported runtime will be Kubernetes but one could also imagine other runtimes such AWS Lambda or ECS, DO Apps, Docker, etc.

The idea is that the CLI makes it extremely easy to deploy such apps from the most minimal of configurations, for example by building
container images using buildpacks or nixpacks, but also supporting Dockerfiles, and possibly other methods.

For now we'll except that the container image will be built locally as part of the CLI call, but a future extension could be that we pass
through the details to a Buildkit daemon to be used when building the container image.

Container images are pushed to an internal container registry through temporary credentials that are passed to the frontend from the
backend.

The CLI allows creating and managing "projects" which represent an app that can be published. A project has a name, and the name defines how
the app is accessible under the common domain name for it (e.g. https://my-project.rise.dev). Users need to authenticate with the backend to
get access to manage projects. The backend must support local authentication (most useful for development) and OIDC (either login via OAuth2
and/or accepting JWT from a set of configured trusted issuers).

An example interaction with the `rise` CLI that a user might perform might be:

  $ rise login
  Please login to rise at https://rise.dev/oauth/login?code=1234-abcd
  Login successful! Welcome back, Niklas!

  $ rise p c secret-app --visibility private --owner team:devopsy
  Team 'devopsy' does not exist or you do not have permission to create projects for it. Did you mean 'devops'?

  $ rise p c secret-app --visibility private --owner team:devops
  Created project 'secret-app' with private visibility owned by team:devops.

  $ rise p ls
  PROJECT         STATUS        URL                             VISIBILITY    OWNER
  my-first-app    running       https://my-first-app.rise.dev   public        user:niklas
  secret-app      stopped       https://secret-app.rise.dev     private       team:devops

  $ cat .rise.toml
  project = "secret-app"

  [build]
  backend = "buildpacks"

  $ rise d c secret-app
  Building container image 'registry.rise.dev/secret-app:latest' using buildpacks...
  Pushing container image to registry.rise.dev...
  Deploying 'secret-app' ...
  Deployment successful! Your app is now running at https://secret-app.rise.dev

The backend and CLI should be designed with extensibility in mind, allowing for future support of additional container
runtimes, build methods, and authentication mechanisms. The CLI should provide clear and concise feedback to the user
at each step of the process, ensuring a smooth and user-friendly experience.

Let's outline the architecture and components needed for this Rust-based project, including both the backend and CLI.

## Architecture Overview

1. **Backend Service**:
   - **Authentication Module**: Uses **PostgreSQL** database with **Dex OAuth2/OIDC** for user management and authentication. Handles JWT validation from Dex.
   - **Project Management Module**: Manages project creation, listing, and ownership using **PostgreSQL** as the data store.
   - **Container Registry Module**: Generates temporary credentials for pushing images to a container registry that
      the CLI can use. The container registry itself is out of scope, but access to a container registry with
      permissions to manage credentials will be supplied to the backend.
   - **Deployment Module**: Interfaces with Kubernetes (and future runtimes) to deploy applications.
   - **API Layer**: Exposes RESTful endpoints for the CLI to interact with.
   - **Configuration Module**: Handles deserialization and validation of the backend server configuration.
   - **Database**: **PostgreSQL** is used as the primary database with **SQLX** for compile-time verified queries and migrations. **Dex** handles OAuth2/OIDC authentication.

2. **CLI Tool**:
    - **Authentication Commands**: Implements the `login` command to authenticate users against the backend (which authenticates via Dex OAuth2/OIDC).
    - **Project Commands**: Implements `project` (alias: `p`) with subcommands: `create/c/new`, `list/ls/l`, `show/s`, `update/u/edit`, `delete/del/rm`.
    - **Team Commands**: Implements `team` (alias: `t`) with subcommands: `create/c/new`, `list/ls/l`, `show/s`, `update/u/edit`, `delete/del/rm`.
    - **Deployment Commands**: Implements `deployment` (alias: `d`) with subcommands: `create/c/new`, `list/ls/l`, `show/s`, `rollback`, `stop`.
    - **Build Module**: Supports building container images using buildpacks, Dockerfiles, and Railpacks with automatic detection or explicit backend selection.
    - **Configuration Module**: Handles reading and writing of `.rise.toml` configuration files.

## Implementation Steps

1. **Set Up the Backend**:
   - [x] Initialize a new Rust project for the backend using `cargo new rise-backend`.
   - [x] **Infrastructure**: Create a `docker-compose.yml` to run local PostgreSQL and Dex instances.
   - [x] **Database & Auth**: Integrate SQLX for PostgreSQL and implement JWT validation for Dex.
   - [x] Implement the authentication module with Dex OAuth2/OIDC integration.
   - [x] Create the project management module using PostgreSQL with SQLX migrations.
   - [x] Integrate with a container registry to generate temporary credentials (Docker registry with DockerProvider).
   - [x] Implement the deployment module with Docker controller (MVP runtime, Kubernetes future).
   - [x] Set up the API layer using a web framework like Actix-web or Rocket (using Axum).
   - [x] Implement configuration handling for the backend server.
   - [x] Add deployment controller with reconciliation loop and health checks.
   - [x] Support for pre-built image deployments with digest pinning.
   - [x] Implement deployment rollback functionality.
   - [x] Replace custom device flow with standard OAuth2 flows:
     - [x] Implement OAuth2 authorization code flow with PKCE
     - [x] Remove custom backend device flow implementation
     - [x] Remove password grant flow (deprecated in OAuth 2.1)
     - [x] Add `/auth/code/exchange` endpoint for PKCE flow

2. **Set Up the CLI**:
    - [x] Initialize a new Rust project for the CLI using `cargo new rise-cli`.
    - [x] Implement authentication commands to interact with the backend.
    - [x] Implement standard OAuth2 authentication flows:
      - [x] OAuth2 authorization code flow with PKCE (default) ✅ **WORKING**
      - [x] Native Dex device authorization flow (via `--device` flag) ⚠️ **NOT COMPATIBLE WITH DEX**
        - Note: Dex's device flow implementation doesn't follow RFC 8628 properly
        - Dex uses a hybrid approach incompatible with pure CLI implementation
        - Browser flow is recommended and is the default
      - [x] Remove password authentication
      - [x] Local HTTP callback server for authorization code flow
    - [x] Create project management commands for creating and listing projects.
    - [x] Develop the build module to support buildpacks (via pack CLI), Dockerfiles (via docker/podman), and Railpacks (via railpack CLI).
    - [x] Implement automatic build method detection (Dockerfile vs buildpacks) with optional `--backend` flag for explicit selection.
    - [x] Add `rise build` command for building images locally without deployment.
    - [x] Support Railpacks build method with both buildx (default) and buildctl via `--backend railpack` or `--backend railpack:buildctl`.
    - [x] Implement deployment commands to handle the build, push, and deploy process.
    - [x] Set up configuration handling for the CLI tool (.rise-config.toml).
    - [x] Add deployment management commands (list, show, rollback).
    - [x] Implement `--image` flag for deploying pre-built images without builds.
    - [x] Add deployment following with auto-refresh and timeout support.

3. **Testing and Documentation**:
    - Write unit and integration tests for both the backend and CLI.
    - Document the API endpoints and CLI commands for user reference.
    - Provide examples and usage guides in the project README files.

By following this outline, we can create a robust Rust-based project that meets the requirements for deploying simple
apps to container runtimes using a user-friendly CLI.

## Process Architecture

The Rise backend runs as a single process with all controllers running as concurrent tasks within the same process:

- **HTTP Server + Controllers** (`rise backend server`): Single process that handles API requests, authentication, and runs all controller loops concurrently

### Controllers

All controllers run automatically as background tokio tasks when the server starts:

- **Deployment Controller**: Reconciles deployment state, monitors health, and manages deployments (Docker or Kubernetes backend based on configuration)
- **Project Controller**: Handles project lifecycle (deletion and cleanup)
- **ECR Controller**: Manages ECR repository lifecycle (only enabled when ECR registry is configured)

Controllers are enabled automatically based on configuration:
- **Deployment**: Always enabled (backend determined by presence of `kubernetes` config)
- **Project**: Always enabled
- **ECR**: Enabled only when `registry.type = "ecr"`

### Benefits of Single-Process Design

1. **Simpler Deployment**: Only one process to manage and monitor
2. **Easier Local Development**: Start everything with a single command
3. **Shared Resources**: Controllers can share state and connections efficiently
4. **Unified Logging**: All logs in one stream with component prefixes

### Running Locally

Start the backend with all controllers:

```bash
mise run start  # Starts server via overmind using Procfile.dev
```

Or directly:

```bash
cargo run --bin rise -- backend server  # HTTP server + all controllers
```

Environment variables are centralized in `.envrc` (loaded by direnv):
- `DATABASE_URL`: PostgreSQL connection string
- `RISE_CONFIG_RUN_MODE`: development/production

Server configuration (host, port, etc.) is specified in `rise-backend/config/default.toml` and can be overridden in `local.toml` or using environment variable substitution in config files.

## Build Backends

The CLI supports three build methods for creating container images:

### Docker (Dockerfile)
Uses `docker build` or `podman build` to build from a Dockerfile:
```bash
rise build myapp:latest --backend docker
rise deployment create myproject --backend docker
```

### Pack (Cloud Native Buildpacks)
Uses `pack build` with Cloud Native Buildpacks:
```bash
rise build myapp:latest --backend pack
rise build myapp:latest --backend pack --builder paketobuildpacks/builder:base
rise deployment create myproject --backend pack
```

### Railpack (Railway Railpacks)
Uses Railway's Railpacks with BuildKit (buildx or buildctl):
```bash
# Railpack with buildx (default)
rise build myapp:latest --backend railpack
rise deployment create myproject --backend railpack

# Railpack with buildctl
rise build myapp:latest --backend railpack:buildctl
rise deployment create myproject --backend railpack:buildctl
```

**Troubleshooting**: If railpack builds fail with the error:
```
ERROR: failed to build: failed to solve: requested experimental feature mergeop has been disabled on the build server: only enabled with containerd image store backend
```

This occurs when using Docker Desktop's default builder. Create a custom buildx builder to work around this:
```bash
docker buildx create --use
```

After creating the custom builder, railpack builds should work correctly.

**Auto-detection**: When `--backend` is omitted, the CLI automatically detects the build method:
- If `Dockerfile` exists → uses `docker` backend
- Otherwise → uses `pack` backend

**Examples**:
```bash
# Auto-detect (has Dockerfile → uses docker)
rise build myapp:latest

# Auto-detect (no Dockerfile → uses pack)
rise build myapp:latest

# Explicit backend selection
rise build myapp:latest --backend railpack
```

## SSL Certificate Handling (Managed BuildKit Daemon)

When building with BuildKit-based backends (`docker`, `railpack`) on macOS behind corporate proxies (Cloudflare, Zscaler, etc.) or environments with custom CA certificates, builds may fail with SSL certificate verification errors.

### The Problem

BuildKit runs as a separate daemon and requires CA certificates to be available at daemon startup. This affects two scenarios:
1. **BuildKit daemon operations**: Pulling base images, accessing registries
2. **Build-time operations**: Application builds (RUN instructions) downloading packages, cloning repos

### Solution: Managed BuildKit Daemon

Rise CLI provides an opt-in managed BuildKit daemon feature that automatically creates and manages a BuildKit daemon with SSL certificate support.

**Enable via CLI flag:**
```bash
rise build myapp:latest --backend railpack --managed-buildkit
rise deployment create myproject --backend railpack --managed-buildkit
```

**Or set environment variable:**
```bash
export RISE_MANAGED_BUILDKIT=true
rise build myapp:latest --backend railpack
```

**Or configure permanently:**
```bash
# Set in config file
rise config set managed_buildkit true
```

### How It Works

When `--managed-buildkit` is enabled and `SSL_CERT_FILE` environment variable is set:
1. Rise CLI creates a `rise-buildkit` daemon container with the certificate mounted at `/etc/ssl/certs/ca-certificates.crt`
2. The daemon is configured with `--platform linux/amd64` for Mac compatibility
3. Subsequent builds use this managed daemon via `BUILDKIT_HOST` environment variable
4. If `SSL_CERT_FILE` changes, the daemon is automatically recreated

### Warning When Not Enabled

If `SSL_CERT_FILE` is set but managed BuildKit is disabled, you'll see:
```
Warning: SSL_CERT_FILE is set but managed BuildKit daemon is disabled.

Railpack builds may fail with SSL certificate errors in corporate environments.

To enable automatic BuildKit daemon management:
  rise build --managed-buildkit ...

Or set environment variable:
  export RISE_MANAGED_BUILDKIT=true

For manual setup, see: https://github.com/NiklasRosenstein/rise/issues/18
```

### Affected Build Backends

- ✅ `pack` - Already supports `SSL_CERT_FILE` natively (no managed daemon needed)
- ⚠️ `docker` - Benefits from managed daemon when using BuildKit
- ⚠️ `railpack` / `railpack:buildx` - Benefits from managed daemon
- ⚠️ `railpack:buildctl` - Benefits from managed daemon

### Manual Setup (Advanced)

For users who prefer manual control, you can create your own BuildKit daemon:

```bash
# Start BuildKit daemon with certificate
docker run --platform linux/amd64 --privileged --name my-buildkit --rm -d \
  --volume $SSL_CERT_FILE:/etc/ssl/certs/ca-certificates.crt:ro \
  moby/buildkit

# Point Rise CLI to your daemon
export BUILDKIT_HOST=docker-container://my-buildkit
rise build myapp:latest --backend railpack
```

For more details, see [Issue #18](https://github.com/NiklasRosenstein/rise/issues/18).

## Guidelines

- You must focus on building any given feature at a time in small increments and commit your changes often.
- You must be able to use the Git commit history as a reference to help remember what you did and why.
- You must ensure to keep this document updated as the project evolves, keeping track of the features that have been
  implemented and any changes to the architecture or design decisions that may have been made along the way.
- You must write clean, maintainable, and well-documented code, following Rust best practices.
- You must ensure that the project is modular and extensible, allowing for future enhancements and additions.
- You must prioritize user experience in the CLI, providing clear feedback and error messages.
- You must embrace modern design practices, such as modular controller design, dependency injection, and separation of concerns.
- Don't commit the .claude directory
- Axum capture groups are formatted as `{capture}`
- Keep the documentation updated. Don't be overly verbose when documenting the project. People can read the code, but things that are not obvious or help getting started and context are usually helpful in documentation, as well as well-placed and lean examples.
- Your todo lists should always include tasks for ensuring formatting and linting are addressed and creating commits of reasonable size (related changes in one commit)

## Future Enhancements

### Ingress Authentication (Kubernetes Controller)

The project `visibility` field (Public/Private) is currently stored but not enforced at the API level. This field is intended for future ingress-level authentication when deploying to Kubernetes:

- **Public projects**: The ingress will serve the application without requiring authentication
- **Private projects**: The ingress will require user authentication AND verify project access authorization before serving the application

**Current State**: The visibility field is stored in the database and returned via the API, but does NOT affect:
- API authorization (all projects require ownership/team membership to access via API)
- Docker controller deployments (no ingress authentication layer)

**Implementation Plan**:
- When the Kubernetes controller is implemented, it will configure ingress resources based on the visibility field
- Public projects will have standard ingress rules
- Private projects will have OAuth2 proxy or similar authentication middleware configured in the ingress
- The authentication layer will validate both user identity AND project access permissions before proxying requests to the application

This feature is specifically for the Kubernetes controller and will not be implemented for the Docker controller.
- When removing a feature, do a comprehensive check on the codebase to ensure any remaining references to that feature are removed or updated. This includes documentation files/READMEs, config files, code comments, etc.
- Run `mise sqlx:check` and `mise sqlx:prepare` (if needed) as part of the finalizing steps
- The CLI should first and foremost always accept the names of things (e.g. project names, or project names + deployment timestamp). The UUIDs in our tables are only for internal book-keeping.