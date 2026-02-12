# Rise <img src="static/assets/favicon-32x32.png" height="32px" align="right"/> 

<p align="center">
    <p align="center">Rise is a Kubernetes-based platform for deploying containerized apps.</p>
    <p align="center"><small><em>DISCLAIMER: Rise is an early work-in-progress project that mostly uses AI-generated code.</em></small></p>
    <img src="./screenshot.png" alt="Rise Web Dashboard Screenshot"/>
</p>

[Go to Documentation → ](https://niklasrosenstein.github.io/rise/)

## What is Rise?

  [pack]: https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/
  [railpack]: https://railpack.com/

Rise simplifies container deployment by providing:

- **Simple CLI** for building and deploying apps
    - **Buildpack support** with [pack] and [railpack]
    - **Enterprise ready** with support for corparate MITM proxies (handles `SSL_CERT_FILE` and `HTTPS_PROXY` forwarding)
- **Web dashboard** for monitoring deployments
- **Project & Team Management**: Organize apps and collaborate with teams
- **OAuth2/OIDC Authentication**: Secure authentication for Rise and deployed apps
- **Multi-tenant projects** with team collaboration
- **Automatic OCI repository provisioning**: Push images to AWS ACR with secure temporary credentials without per-project infrastructure setup
- **Service Accounts**: Workload identity for GitHub Actions, GitLab CI, etc. to deploy from CI/CD

## Install CLI from crates.io

```bash
# Install the CLI and backend from crates.io
cargo install rise-deploy

# Verify installation
rise --version
```

Note that this does not include server code unless you use `--features cli,server`.

## Local Development

### Prerequisites

- Docker and Docker Compose
- Rust 1.91+
- [mise](https://mise.jdx.dev/) (recommended for development)

### Start Services

```bash
direnv allow
# or else use `. .envrc`

# Install development tools
mise install

# Terminal (1): Start Minikube
mise minikube:launch

# Terminal (2): Start the frontend
mise frontend:dev

# Terminal (3) Start the backend (will also start required containers with docker compose)
mise backend:run
```

Services will be available at:
- **Rise server**: http://localhost:3000
- **PostgreSQL**: localhost:5432
- **Minikube HTTP/HTTPS Ingress**: http://localhost:8080, https://localhost:8443
- **Vite.js Frontend Server**: http://localhost:5731

However, you need to configure your `/etc/hosts` on your host to ensure consistent name resolution between the involved network namespace:

```
127.0.0.1 rise-registry
127.0.0.1 rise.local
127.0.0.1 {project}.rise.local # One for each Rise-deployed project you want to access
```

**Default credentials**:
- Email: `admin@example.com` or `test@example.com`
- Password: `password`

## Deploy your first app

```bash
# Build the CLI
cargo build
# `rise` binary should be available from direnv, otherwise use `cargo run`

rise login # Add --url http://rise.local:3000 if you've logged into another backend before

cd examples/hello-world
rise project create hello-world
rise deploy
```
