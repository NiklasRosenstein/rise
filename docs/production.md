# Production Setup

Guidelines for deploying Rise in production environments.

## Overview

This guide covers security, configuration, database setup, monitoring, and operational considerations for running Rise in production.

## Deployment Backend

Rise supports multiple deployment backends for running applications:

| Backend | Description | Use Case |
|---------|-------------|----------|
| **Docker** | Deploys to local Docker daemon | Development, single-server deployments |
| **Kubernetes** | Deploys to Kubernetes clusters | Production, multi-server, cloud deployments |

**Choosing a backend**:
- Use **Docker** for simple, single-server deployments or development
- Use **Kubernetes** for production environments requiring scalability, high availability, and cloud integration

See [Kubernetes Backend](./kubernetes.md) for Kubernetes-specific configuration and operation.

## Security Best Practices

### Registry Credentials

- Use IAM roles (IRSA on EKS, instance profiles on EC2)
- Avoid long-lived IAM user credentials
- Rise generates scoped push credentials (single repo, 12-hour max)

### Network Isolation

- Deploy backend in private subnets with ALB in public subnets
- Enable TLS (HTTPS), terminate at load balancer
- Restrict database to backend security group only

### Secrets Management

Use AWS Secrets Manager or HashiCorp Vault for: `DATABASE_URL`, OAuth2 client secrets, registry credentials, JWT signing keys.

### Authentication

- Use trusted OIDC providers (Dex, Auth0, Okta)
- Configure redirect URLs, enable PKCE
- Dex production: external storage backend (PostgreSQL/etcd), configure SSO connectors, enable TLS. See [Dex docs](https://dexidp.io/docs/kubernetes/)

## Environment Variables

Key environment variables for production:

```bash
# Database
DATABASE_URL="postgres://rise:password@rds-endpoint:5432/rise"

# Backend configuration
RISE_CONFIG_DIR="/etc/rise/config"
RISE_CONFIG_RUN_MODE="production"
RISE_SERVER__HOST="0.0.0.0"
RISE_SERVER__PORT="3000"

# Registry (ECR)
RISE_REGISTRY__TYPE="ecr"
RISE_REGISTRY__REGION="us-east-1"
RISE_REGISTRY__ACCOUNT_ID="123456789012"
RISE_REGISTRY__REPO_PREFIX="rise/"
RISE_REGISTRY__ROLE_ARN="arn:aws:iam::123456789012:role/rise-backend"
RISE_REGISTRY__PUSH_ROLE_ARN="arn:aws:iam::123456789012:role/rise-backend-ecr-push"

# Kubernetes (if using Kubernetes backend)
RISE_KUBERNETES__INGRESS_CLASS="nginx"
RISE_KUBERNETES__HOSTNAME_FORMAT="{project_name}.apps.rise.dev"
RISE_KUBERNETES__NONDEFAULT_HOSTNAME_FORMAT="{project_name}-{deployment_group}.preview.rise.dev"
RISE_KUBERNETES__NAMESPACE_FORMAT="rise-{project_name}"
# RISE_KUBERNETES__KUBECONFIG="/path/to/kubeconfig"  # Optional, defaults to in-cluster
```

## Database Setup

### PostgreSQL Configuration

Use managed database (AWS RDS, Cloud SQL, Azure Database).

**RDS settings**: `db.t3.medium`+, 100GB GP3 with autoscaling, Multi-AZ, 7-30 day backup retention, encryption, PostgreSQL 16+

### Running Migrations

```bash
export DATABASE_URL="postgres://rise:password@rds-endpoint:5432/rise"
sqlx migrate run
```

### Database Backups

Enable automated backups (7+ days), take manual snapshots before major changes, enable point-in-time recovery.

### Connection Pooling

Rise uses SQLx with connection pooling. Configure pool size based on load in `config/production.toml` if needed.

## High Availability

### Multi-Process Architecture

| Process | Purpose | Scaling |
|---------|---------|---------|
| `backend-server` | HTTP API, OAuth | Horizontal |
| `backend-deployment` | Deployment controller | Single instance* |
| `backend-project` | Project lifecycle | Single instance* |
| `backend-ecr` | ECR management | Single instance* |

*Leader election for controllers planned for future.

### Health Checks

- `GET /health` - Overall health
- `GET /ready` - Readiness (database connectivity)

**LB config**: `/health`, 30s interval, 2/3 thresholds, 5s timeout

### Database Failover

RDS Multi-AZ: automatic failover (1-2 min), backend reconnects automatically.

## Monitoring

### Key Metrics

- Request rate/latency (P50, P95, P99), error rate (4xx/5xx)
- Active deployments, build/push times
- CPU/memory, DB connection pool, disk I/O
- Projects created, deployments/day, active users

### Logging

Rise uses structured JSON logs. Aggregate with CloudWatch, Cloud Logging, ELK, or Loki+Grafana.

### Alerting

**Critical**: DB connection failures, >5% 5xx rate, controller not reconciling, low disk space
**Warning**: Slow queries (>1s), high CPU (>80%), memory leaks, old deployments

## Disaster Recovery

### Backup Strategy

**Backup**: Database (RDS snapshots), config (git), secrets (Secrets Manager)
**Don't backup**: Container images (in ECR), credentials, binaries

### Recovery

1. Restore database from snapshot
2. Deploy backend from git
3. Run migrations: `sqlx migrate run`
4. Restore config/secrets
5. Start processes, verify health

## Operational Tasks

### Updating Rise

```bash
git pull origin main
cargo build --release --bin rise
sqlx migrate run
# Restart processes (method depends on deployment: systemd, K8s, etc.)
```

### Cleanup

Deployments with `--expire` auto-delete. Manual: `rise deployment stop my-app:20241105-1234`

### Monitoring Database Size

```sql
SELECT pg_size_pretty(pg_database_size('rise'));
SELECT tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename))
FROM pg_tables WHERE schemaname = 'public' ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```

## Cost Optimization

- **Database**: Right-size instances (start `db.t3.medium`), auto-scale storage, use Reserved Instances
- **ECR**: Lifecycle policies, image compression, cleanup unused repos
- **Compute**: Right-size instances/nodes, spot instances for non-critical, auto-scaling

## Next Steps

- **Configure authentication**: See [Authentication](authentication.md)
- **Set up CI/CD**: See [Authentication](authentication.md#service-accounts-workload-identity)
- **Container registries**: See [Container Registries](registries.md)
