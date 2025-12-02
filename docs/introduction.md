# Introduction

Rise is a Rust-based platform for deploying containerized applications with minimal configuration.

## What it does

- **Manage projects** - Create and organize deployable applications
- **Team collaboration** - Share projects with teams, transfer ownership
- **Registry integration** - Push images to AWS ECR or JFrog Artifactory
- **Authentication** - JWT-based auth with device flow or password login

## Architecture

```
┌─────────────┐
│  rise-cli   │  ← Command-line interface
└──────┬──────┘
       │ HTTP/JSON
┌──────▼──────────┐
│  rise-backend   │  ← Axum REST API
└──────┬──────────┘
       │
┌──────▼──────────┐
│   PocketBase    │  ← Database & Auth
└─────────────────┘
```

## Current Status

**Implemented:**
- User authentication (device flow, password)
- Project CRUD with ownership model
- Team management with fuzzy search
- Multi-provider registry abstraction (ECR, Artifactory)

**In Progress:**
- Container image building (buildpacks, nixpacks, Dockerfile)
- Kubernetes deployment integration
