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

**Use IAM roles instead of access keys**:
- Prefer IRSA (IAM Roles for Service Accounts) on EKS
- Use EC2 instance profiles on EC2
- Avoid long-lived IAM user credentials

**Rotate credentials regularly**:
- If using access keys, rotate them monthly
- Use AWS Secrets Manager or similar for storage
- Never commit credentials to version control

**Use scoped credentials**:
- Rise generates scoped push credentials limited to single repositories
- Credentials expire after 12 hours maximum

### Network Isolation

**Use private subnets**:
- Deploy Rise backend in private subnets
- Use Application Load Balancer in public subnets
- Restrict database access to backend security group only

**Enable TLS**:
- Use HTTPS for all external communication
- Configure TLS termination at load balancer
- Use cert-manager or ACM for certificate management

**Firewall rules**:
```
Inbound:
  - 443 (HTTPS) from 0.0.0.0/0
  - 5432 (PostgreSQL) from backend security group only

Outbound:
  - 443 (HTTPS) to ECR, Dex, external APIs
  - 5432 (PostgreSQL) to RDS
```

### Secrets Management

**Store secrets securely**:
- Use AWS Secrets Manager, HashiCorp Vault, or similar
- Inject secrets as environment variables at runtime
- Never log secrets or expose in error messages

**Example secrets to manage**:
- Database connection string (`DATABASE_URL`)
- OAuth2 client secrets (Dex configuration)
- Registry credentials (if using IAM user)
- JWT signing keys (future)

### Authentication

**Configure OAuth2/OIDC properly**:
- Use trusted OIDC providers (Dex, Auth0, Okta, etc.)
- Configure allowed redirect URLs
- Enable PKCE for browser-based flows
- Set appropriate token expiration times

**Dex in production**:
- Use external storage backend (PostgreSQL, etcd)
- Configure connectors for corporate SSO (LDAP, SAML, GitHub, etc.)
- Enable TLS for Dex endpoints
- See [Dex documentation](https://dexidp.io/docs/kubernetes/) for Kubernetes deployment

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

**Use managed database**:
- AWS RDS PostgreSQL (recommended)
- Google Cloud SQL
- Azure Database for PostgreSQL
- Self-managed PostgreSQL with backups

**Recommended RDS settings**:
- Instance class: `db.t3.medium` or larger
- Storage: 100 GB GP3 with autoscaling
- Multi-AZ: Enabled for high availability
- Backup retention: 7-30 days
- Encryption: Enable at-rest encryption
- PostgreSQL version: 16 or later

### Running Migrations

Migrations must run before starting the backend:

```bash
# Set DATABASE_URL
export DATABASE_URL="postgres://rise:password@rds-endpoint:5432/rise"

# Run migrations
cd rise-backend
sqlx migrate run
```

**In CI/CD**:
```yaml
- name: Run database migrations
  run: |
    cd rise-backend
    sqlx migrate run
  env:
    DATABASE_URL: ${{ secrets.DATABASE_URL }}
```

### Database Backups

**Automated backups** (RDS):
- Enable automated backups with 7+ day retention
- Take manual snapshots before major changes
- Test restore procedures regularly

**Point-in-time recovery**:
- Enable if using RDS
- Allows recovery to any point within backup retention period

### Connection Pooling

Rise uses SQLx with connection pooling. Configure pool size based on load in `config/production.toml` if needed.

## High Availability

### Multi-Process Architecture

Rise uses separate processes for different responsibilities:

| Process | Purpose | Scaling |
|---------|---------|---------|
| `backend-server` | HTTP API, OAuth | Horizontal (multiple instances) |
| `backend-deployment` | Deployment controller | Single instance (leader election future) |
| `backend-project` | Project lifecycle | Single instance (leader election future) |
| `backend-ecr` | ECR management | Single instance (leader election future) |

**Current limitations**:
- Controllers assume single instance (no leader election yet)
- Scale server processes horizontally
- Controllers should run on single instance

**Future improvements**:
- Add leader election for controllers
- Support multiple controller instances
- Distributed locking for reconciliation

### Health Checks

Configure health check endpoints:

```
GET /health - Overall health
GET /ready - Readiness probe (checks database connectivity)
```

**Load balancer configuration**:
```
Health check path: /health
Health check interval: 30s
Healthy threshold: 2
Unhealthy threshold: 3
Timeout: 5s
```

### Database Failover

If using RDS Multi-AZ:
- Automatic failover to standby (1-2 minutes)
- Rise backend reconnects automatically
- No manual intervention required

## Monitoring

### Metrics to Track

**Application metrics**:
- Request rate and latency (P50, P95, P99)
- Error rate (4xx, 5xx responses)
- Active deployments
- Container build times
- Image push durations

**Infrastructure metrics**:
- CPU and memory usage
- Database connection pool utilization
- Disk I/O and storage usage

**Business metrics**:
- Projects created
- Deployments per day
- Active users

### Logging

**Structured logging**:
Rise uses structured logs (JSON format) for easier parsing.

**Log aggregation**:
- CloudWatch Logs (AWS)
- Google Cloud Logging
- ELK Stack (Elasticsearch, Logstash, Kibana)
- Loki + Grafana

**Key log fields**:
- `timestamp`: ISO 8601 timestamp
- `level`: `info`, `warn`, `error`
- `target`: Module path
- `message`: Log message
- `process`: Process name (`backend-server`, `backend-deployment`, etc.)

### Alerting

**Critical alerts**:
- Database connection failures
- High error rates (>5% 5xx responses)
- Deployment controller not reconciling
- Disk space low (<20%)

**Warning alerts**:
- Slow database queries (>1s)
- High CPU usage (>80%)
- Increasing memory usage
- Old deployments not being cleaned up

## Disaster Recovery

### Backup Strategy

**What to backup**:
- Database (automated RDS snapshots)
- Configuration files (store in git)
- Secrets (backup from Secrets Manager)

**Do NOT backup**:
- Container images (stored in ECR)
- Temporary credentials
- Compiled binaries

### Recovery Procedures

**Database restoration**:
```bash
# Restore from RDS snapshot
aws rds restore-db-instance-from-db-snapshot \
  --db-instance-identifier rise-restored \
  --db-snapshot-identifier rise-snapshot-20241205

# Update DATABASE_URL to point to restored instance
```

**Full system recovery**:
1. Restore database from snapshot
2. Deploy Rise backend from git
3. Run migrations: `sqlx migrate run`
4. Restore configuration from git/Secrets Manager
5. Start backend processes
6. Verify health checks pass

## Operational Tasks

### Updating Rise

```bash
# Pull latest code
git pull origin main

# Build new binaries
cargo build --release --bin rise

# Run migrations
cd rise-backend && sqlx migrate run

# Restart processes (with zero-downtime deployment)
# This depends on your deployment method (systemd, Kubernetes, etc.)
```

### Cleaning Up Old Deployments

Deployments with `--expire` auto-delete. Manual cleanup:

```bash
# List old deployments
rise deployment list my-app --limit 100

# Stop old deployments
rise deployment stop my-app:20241105-1234
```

### Monitoring Database Size

```sql
-- Check database size
SELECT pg_size_pretty(pg_database_size('rise'));

-- Check table sizes
SELECT
  tablename,
  pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename))
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```

## Cost Optimization

**Database**:
- Use appropriate instance size (start with `db.t3.medium`)
- Enable auto-scaling storage
- Use Reserved Instances for predictable workloads

**ECR**:
- Enable lifecycle policies to delete old images
- Use image compression
- Delete unused repositories

**Compute**:
- Right-size EC2 instances or Kubernetes nodes
- Use spot instances for non-critical workloads
- Enable auto-scaling

## Next Steps

- **Configure authentication**: See [Authentication](authentication.md)
- **Set up CI/CD**: See [Authentication](authentication.md#service-accounts-workload-identity)
- **Container registries**: See [Container Registries](registries.md)
