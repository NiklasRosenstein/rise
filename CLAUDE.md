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
    - **Build Module**: Supports building container images using buildpacks, nixpacks, and Dockerfiles.
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
    - [x] Develop the build module to support buildpacks (via pack CLI).
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

The Rise backend uses a multi-process architecture where the HTTP server and controllers run as separate processes:

- **HTTP Server** (`rise backend server`): Handles API requests, authentication, and user interactions
- **Deployment Controller** (`rise backend controller deployment-docker`): Background process that reconciles deployment state, monitors health, and manages Docker containers
- **Project Controller** (`rise backend controller project`): Background process that handles project lifecycle (deletion and cleanup)

### Benefits of Multi-Process Design

1. **Better Log Separation**: Each process has distinct logs with process name prefixes
2. **Independent Scaling**: Controllers can run multiple instances independently
3. **Resource Optimization**: Each process only allocates resources it needs (e.g., controllers don't initialize auth/registry components)
4. **Clearer Operations**: Start/stop/restart components independently
5. **No God Objects**: Components get only the state they need (ControllerState vs AppState)

### State Design

- **AppState**: Full state for HTTP server (db_pool, jwt_validator, oauth_client, registry_provider, oci_client)
- **ControllerState**: Minimal state for controllers (db_pool only)

### Running Locally

All processes are defined in `Procfile.dev` and can be started together:

```bash
mise run start  # Starts all processes via overmind
```

Or individually:

```bash
cargo run --bin rise -- backend server                          # HTTP server
cargo run --bin rise -- backend controller deployment-docker    # Deployment controller
cargo run --bin rise -- backend controller project              # Project controller
```

Environment variables are centralized in `.envrc` (loaded by direnv):
- `DATABASE_URL`: PostgreSQL connection string
- `RISE_CONFIG_RUN_MODE`: development/production

Server configuration (host, port, etc.) is specified in `rise-backend/config/default.toml` and can be overridden in `local.toml` or using environment variable substitution in config files.

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
- Keep the ECR controller disabled by default in Procfile.dev, we only currently have it enabled for development
- When removing a feature, do a comprehensive check on the codebase to ensure any remaining references to that feature are removed or updated. This includes documentation files/READMEs, config files, code comments, etc.
- Run `mise sqlx:check` and `mise sqlx:prepare` (if needed) as part of the finalizing steps