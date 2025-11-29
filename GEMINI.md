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
the app is accessible under the common domain name for it (e.g. https://my-project.rise.net). Users need to authenticate with the backend to
get access to manage projects. The backend must support local authentication (most useful for development) and OIDC (either login via OAuth2
and/or accepting JWT from a set of configured trusted issuers).

An example interaction with the `rise` CLI that a user might perform might be:

  $ rise login
  Please login to rise at https://rise.net/oauth/login?code=1234-abcd
  Login successful! Wlecome back, Niklas!

  $ rise create secret-app --visibility private --owner team:devopsy
  Team 'devopsy' does not exist or you do not have permission to create projects for it. Did you mean 'devops'?

  $ rise create secret-app --visibility private --owner team:devops
  Created project 'secret-app' with private visibility owned by team:devops.

  $ rise ls
  PROJECT         STATUS        URL                             VISIBILITY    OWNER
  my-first-app    running       https://my-first-app.rise.net   public        user:niklas
  secret-app      stopped       https://secret-app.rise.net     private       team:devops

  $ cat .rise.toml
  project = "secret-app"

  [build]
  backend = "buildpacks"

  $ rise deploy
  Building container image 'registry.rise.net/secret-app:latest' using buildpacks...
  Pushing container image to registry.rise.net...
  Deploying 'secret-app' ...
  Deployment successful! Your app is now running at https://secret-app.rise.net

The backend and CLI should be designed with extensibility in mind, allowing for future support of additional container
runtimes, build methods, and authentication mechanisms. The CLI should provide clear and concise feedback to the user
at each step of the process, ensuring a smooth and user-friendly experience.

Let's outline the architecture and components needed for this Rust-based project, including both the backend and CLI.

## Architecture Overview

1. **Backend Service**:
   - **Authentication Module**: Handles local authentication and OIDC.
   - **Project Management Module**: Manages project creation, listing, and ownership.
   - **Container Registry Module**: Generates temporary credentials for pushing images to a container registry that
      the CLI can use. The container registry itself is out of scope, but access to a container registry with
      permissions to manage credentials will be supplied to the backend.
   - **Deployment Module**: Interfaces with Kubernetes (and future runtimes) to deploy applications.
   - **API Layer**: Exposes RESTful endpoints for the CLI to interact with.
   - **Configuration Module**: Handles deserialization and validation of the backend server configuration.

2. **CLI Tool**:
    - **Authentication Commands**: Implements the `login` command to authenticate users.
    - **Project Commands**: Implements `create`, `ls`, and other project management commands.
    - **Build Module**: Supports building container images using buildpacks, nixpacks, and Dockerfiles.
    - **Deployment Commands**: Implements the `deploy` command to build, push, and deploy applications.
    - **Configuration Module**: Handles reading and writing of `.rise.toml` configuration files.

## Implementation Steps

1. **Set Up the Backend**:
   - Initialize a new Rust project for the backend using `cargo new rise-backend`.
   - Implement the authentication module with support for local and OIDC authentication.
   - Create the project management module to handle project creation and listing.
   - Integrate with a container registry to generate temporary credentials.
   - Implement the deployment module to interface with Kubernetes.
   - Set up the API layer using a web framework like Actix-web or Rocket.
   - Implement configuration handling for the backend server.

2. **Set Up the CLI**:
    - Initialize a new Rust project for the CLI using `cargo new rise-cli`.
    - Implement authentication commands to interact with the backend.
    - Create project management commands for creating and listing projects.
    - Develop the build module to support different build methods.
    - Implement deployment commands to handle the build, push, and deploy process.
    - Set up configuration handling for the CLI tool.

3. **Testing and Documentation**:
    - Write unit and integration tests for both the backend and CLI.
    - Document the API endpoints and CLI commands for user reference.
    - Provide examples and usage guides in the project README files.

By following this outline, we can create a robust Rust-based project that meets the requirements for deploying simple
apps to container runtimes using a user-friendly CLI.

## Guidelines

- You must focus on building any given feature at a time in small increments and commit your changes often.
- You must be able to use the Git commit history as a reference to help remember what you did and why.
- You must ensure to keep this document updated as the project evolves, keeping track of the features that have been
  implemented and any changes to the architecture or design decisions that may have been made along the way.
- You must write clean, maintainable, and well-documented code, following Rust best practices.
- You must ensure that the project is modular and extensible, allowing for future enhancements and additions.
- You must prioritize user experience in the CLI, providing clear feedback and error messages.
- You must embrace modern design practices, such as modular controller design, dependency injection, and separation of concerns.
