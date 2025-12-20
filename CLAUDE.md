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

**Note**: The project is structured as a **single consolidated Rust crate** (`rise-deploy`) that produces the `rise` binary with both CLI and server capabilities enabled via feature flags.

### Crate Structure (`rise-deploy`)

The codebase is organized into functional modules:

- **`src/db/`**: Database access layer (PostgreSQL via SQLX) - shared by server modules
- **`src/server/`**: Backend server implementation with feature-gated modules:
   - **Authentication Module** (`auth/`): OAuth2/OIDC with Dex, JWT validation
   - **Project Management** (`project/`): Project CRUD and lifecycle management
   - **Team Management** (`team/`): Team and membership management
   - **Container Registry** (`registry/`): Temporary credentials for ECR registries
   - **Deployment Module** (`deployment/`): Kubernetes controller for deployments
   - **ECR Integration** (`ecr/`): AWS ECR repository management (feature: `aws`)
   - **Encryption** (`encryption/`): Local AES-GCM and AWS KMS providers
   - **OCI Client** (`oci/`): OCI registry interaction
   - **Frontend** (`frontend/`): Static web UI assets
   - **API Layer**: RESTful endpoints via Axum
- **`src/cli/`**: CLI command handlers (feature: `cli`)
   - Authentication, project, team, deployment, environment variable commands
   - Local dev OIDC issuer for testing
- **`src/build/`**: Container image build orchestration (feature: `cli`)
   - Support for Docker, Pack (buildpacks), and Railpack backends
   - BuildKit daemon management, SSL certificate handling
- **`src/api/`**: Client-side API interface for server communication (feature: `cli`)

### Feature Flags

The crate uses granular Cargo features for modular compilation:

- **`cli`** (default): CLI commands and client-side functionality
- **`server`**: HTTP server, controllers, and backend logic
- **`aws`**: AWS ECR registry and KMS encryption (requires `server`)
- **`k8s`**: Kubernetes deployment controller (requires `server`)

Examples:
```bash
cargo build                           # CLI-only build (smallest binary)
cargo build --features server,k8s     # Server with Kubernetes backend
cargo build --all-features            # Full build with all capabilities
```

## Implementation Steps

**Project Structure**: Consolidated into single `rise-deploy` crate (formerly separate `rise-backend` and `rise-cli` crates)

### Completed Implementation

1. **Core Infrastructure** ✅
   - [x] Single consolidated crate with feature flags (`cli`, `server`, `aws`, `k8s`)
   - [x] PostgreSQL database with SQLX (compile-time verified queries and migrations)
   - [x] Dex OAuth2/OIDC integration for authentication
   - [x] Docker Compose setup for local development (PostgreSQL, Dex, Registry)

2. **Server Implementation** (`--features server`) ✅
   - [x] Axum-based HTTP API with RESTful endpoints
   - [x] Authentication: OAuth2/OIDC with Dex, JWT validation, PKCE flow
   - [x] Project management: CRUD operations, ownership, visibility
   - [x] Team management: Team creation, membership, role-based access
   - [x] Deployment controller:
     - [x] Kubernetes controller (`--features k8s`) - K8s deployments with Ingress
   - [x] Container registry integration:
     - [x] AWS ECR provider (`--features aws`) with repository lifecycle management
   - [x] Encryption providers: Local AES-GCM and AWS KMS (`--features aws`)
   - [x] OCI client for image digest resolution
   - [x] Frontend static web UI
   - [x] Extensions system:
     - [x] Multiple instances per extension type
     - [x] Generic OAuth 2.0 provider for end-user authentication
       - [x] Fragment flow (default) - tokens in URL fragment for SPAs
       - [x] Exchange token flow - secure backend exchange for server-rendered apps
       - [x] Session-based token caching with automatic refresh
       - [x] Encrypted token storage (AES-GCM/KMS)
       - [x] Support for any OAuth 2.0 provider (Snowflake, Google, GitHub, custom SSO)
       - [x] Client secret stored as encrypted environment variables
     - [x] AWS RDS extension for database provisioning

3. **CLI Implementation** (`--features cli`, default) ✅
   - [x] OAuth2 authorization code flow with PKCE (browser-based, default)
   - [x] Project commands: `create`, `list`, `show`, `update`, `delete`
   - [x] Team commands: `create`, `list`, `show`, `update`, `delete`
   - [x] Deployment commands: `create`, `list`, `show`, `rollback`, `stop`
   - [x] Environment variable management
   - [x] Service account (workload identity) management
   - [x] Local dev OIDC issuer for testing

4. **Build System** (`--features cli`) ✅
   - [x] Docker backend: Standard Dockerfile builds
   - [x] Pack backend: Cloud Native Buildpacks integration
   - [x] Railpack backend: Schema.org Railpacks with BuildKit/Buildx
   - [x] Automatic build method detection
   - [x] BuildKit daemon management with SSL certificate handling
   - [x] `rise build` command for local image builds without deployment
   - [x] Pre-built image deployment support (`--image` flag)
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

## User-Facing Documentation

For user-facing documentation, see the [`/docs`](./docs) directory. Key topics include:
- Build backends (Docker, Pack, Railpack): [docs/builds.md](docs/builds.md)
- SSL certificate handling: [docs/builds.md](docs/builds.md)
- Architecture and process design: [docs/development.md](docs/development.md)
- Configuration: [docs/configuration.md](docs/configuration.md)
- OAuth extension (end-user authentication): [docs/oauth.md](docs/oauth.md)

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

The project `visibility` field (Public/Private) is currently stored but not enforced at the ingress level. This field is intended for ingress-level authentication:

- **Public projects**: The ingress will serve the application without requiring authentication
- **Private projects**: The ingress will require user authentication AND verify project access authorization before serving the application

**Current State**: The visibility field is stored in the database and returned via the API, but does NOT affect:
- API authorization (all projects require ownership/team membership to access via API)
- Ingress routing (authentication not yet configured in ingress annotations)

**Implementation Plan**:
- The Kubernetes controller will configure ingress resources based on the visibility field
- Public projects will have standard ingress rules
- Private projects will have OAuth2 proxy or similar authentication middleware configured in the ingress
- The authentication layer will validate both user identity AND project access permissions before proxying requests to the application
- When removing a feature, do a comprehensive check on the codebase to ensure any remaining references to that feature are removed or updated. This includes documentation files/READMEs, config files, code comments, etc.
- Run `mise sqlx:check` and `mise sqlx:prepare` (if needed) as part of the finalizing steps
- The CLI should first and foremost always accept the names of things (e.g. project names, or project names + deployment timestamp). The UUIDs in our tables are only for internal book-keeping.
- The admin user(s) should always have full access to perform any operation. When we work on a new API endpoint, we make sure admin users don't need to pass regular permission checks.
- Any SQLX queries are to be wrapped by helper functions in the rise_deploy::db crate. No SQLX queries outside of this crate are allowed
- When we log errors and don't handle them further, we should include a sensible amount of information on the error. Often logging the error with `{:?}` is good enough.