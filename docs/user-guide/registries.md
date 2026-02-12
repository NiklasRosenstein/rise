# Container Registries

This page covers registry usage from an application team perspective.

For platform-level setup and backend architecture (IAM roles, Terraform modules, backend config, provider internals), see:

- [Operator Guide: Registry Backend Operations](../operator-registry-operations.md)

## What Rise Does for You

When you deploy with Rise, it resolves registry credentials and push targets based on backend configuration.

In daily usage, you typically just run deployment commands:

```bash
rise deployment create my-app
```

Rise handles pushing images using the configured registry provider.

## Common User Scenarios

### Deploy to a configured registry

```bash
rise deployment create my-app
```

If your project has a custom build/deploy flow, you can still deploy pre-built images:

```bash
rise deployment create --project my-app --image my-registry.example.com/team/my-app:2026-02-12
```

### Use local registry in development

For local workflows, the compose stack usually provides a registry on `localhost:5000`.

```bash
mise backend:deps
rise deployment create my-app
```

You can inspect local registry state with:

```bash
curl http://localhost:5000/v2/_catalog
```

## Provider Notes (User View)

### AWS ECR

In ECR mode, Rise returns temporary push credentials scoped for project usage.

### Docker-compatible registries

For Docker/OCI-compatible registries, Rise uses standard client auth behavior (for example credentials from Docker login state).

## Troubleshooting (User-Level)

### Push failed with access errors

1. Confirm your project/deployment command and image reference are correct.
2. Confirm you can authenticate to Rise (`rise login`).
3. If issue persists, contact your platform operator with the full CLI error output.

### Local registry not reachable

```bash
docker compose ps registry
docker compose logs registry
```

If the local stack is broken, restart dependencies:

```bash
mise backend:deps
```
