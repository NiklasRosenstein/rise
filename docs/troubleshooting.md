# Troubleshooting

Common issues and solutions for Rise.

## Build Issues

### Buildpack: CA Certificate Verification Errors

**Symptoms:**
```
===> ANALYZING
[analyzer] ERROR: failed to initialize analyzer: validating registry read access to <registry>
ERROR: failed to build: executing lifecycle: failed with status code: 1
```

**Cause**: Pack lifecycle container cannot verify SSL certificates when accessing the registry.

**Solution:**
```bash
export SSL_CERT_FILE=/path/to/your/ca-cert.crt
rise deployment create my-app
```

Rise CLI automatically injects the certificate into the pack lifecycle container.

**Manual workaround (pack CLI directly):**
```bash
pack build my-image \
  --builder paketobuildpacks/builder-jammy-base \
  --env SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
  --volume $SSL_CERT_FILE:/etc/ssl/certs/ca-certificates.crt:ro
```

**Still failing?**
1. Verify certificate format: `openssl x509 -in /path/to/ca-cert.crt -text -noout`
2. Use verbose logging: `rise deployment create my-app --verbose`
3. Test registry access: `curl --cacert /path/to/ca-cert.crt https://your-registry.example.com/v2/`

### Railpack: BuildKit Experimental Feature Error

**Symptom:**
```
ERROR: failed to build: failed to solve: requested experimental feature mergeop has been disabled on the build server: only enabled with containerd image store backend
```

**Cause**: Docker Desktop's default builder doesn't support experimental features needed by Railpack.

**Solution:**
```bash
docker buildx create --use
```

See [Building Images](builds.md) for more details on SSL certificates and managed BuildKit daemon.

## Authentication Issues

### "Failed to start local callback server"

**Cause**: Ports 8765, 8766, and 8767 are all in use.

**Solution:**
1. Close applications using these ports
2. Use device flow (if using a compatible OAuth2 provider): `rise login --device`

### "Code exchange failed"

**Common causes:**
1. Backend is not running
2. Dex is not configured properly
3. Network connectivity issues

**Check:**
```bash
# Backend logs
docker-compose logs rise-backend

# Dex logs
docker-compose logs dex
```

### Token Expired

**Symptom**: `401 Unauthorized` on API requests.

**Solution:**
```bash
rise login
```

Tokens expire after 1 hour (default).

## Service Account Issues

### "The 'aud' claim is required"

Add `--claim aud=<unique-value>`:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo
```

### "At least one claim in addition to 'aud' is required"

Add authorization claims:
```bash
rise sa create my-project \
  --issuer https://gitlab.com \
  --claim aud=rise-project-my-project \
  --claim project_path=myorg/myrepo  # Required
```

### "Multiple service accounts matched this token"

**Cause**: Ambiguous claim configuration.

**Solution**: Make claims unique:
```bash
# Unprotected branches
rise sa create dev --claim aud=rise-project-app-dev \
  --claim project_path=myorg/app --claim ref_protected=false

# Protected branches
rise sa create prod --claim aud=rise-project-app-prod \
  --claim project_path=myorg/app --claim ref_protected=true
```

### "No service account matched the token claims"

**Debug steps:**
1. Check token claims (CI/CD systems usually log them)
2. Verify exact match (case-sensitive)
3. Check issuer URL (must match exactly, no trailing slash)
4. Ensure ALL service account claims are present in the token

### "403 Forbidden" (Service Account)

Service accounts can only deploy, not manage projects. Use a regular user account for project operations.

## Database Issues

### "Connection refused" to PostgreSQL

**Check if running:**
```bash
docker-compose ps postgres
docker-compose logs postgres
```

**Restart:**
```bash
docker-compose restart postgres
```

**Verify health:**
```bash
docker-compose exec postgres pg_isready -U rise
```

## Registry Issues

### "Access Denied" when pushing (ECR)

**Causes:**
1. Controller role can't assume push role
2. Push role permissions insufficient
3. Repository doesn't exist with correct prefix
4. STS session policy scope incorrect

**Debug:**
```bash
# Check IAM role trust policy
aws iam get-role --role-name rise-backend-ecr-push

# Check repository exists
aws ecr describe-repositories --repository-names rise/my-app
```

### "Connection refused" to registry (Docker)

**Check if running:**
```bash
docker-compose ps registry
docker-compose logs registry
```

**Restart:**
```bash
docker-compose restart registry
```

### Images not persisting (Docker)

**Check volume:**
```bash
docker volume ls | grep registry
```

**Warning**: `docker-compose down -v` removes volumes and deletes all images!

## Development Environment Issues

### "Address already in use" on port 3000

**Find process:**
```bash
lsof -i :3000
```

**Solution:** Kill the process or change port in `rise-backend/config/local.toml`

### Overmind won't start processes

**Solution:**
```bash
# Stop overmind
overmind quit

# Ensure dependencies running
mise backend:deps

# Run migrations
mise db:migrate

# Try again
mise backend:run
```

### Docker Compose services won't start

**Check logs:**
```bash
docker-compose logs
```

**Reset (deletes data):**
```bash
docker-compose down -v
mise backend:deps
```

## Reset Everything

Complete reset of development environment:

```bash
# Stop all processes
overmind quit
docker-compose down -v

# Remove build artifacts
cargo clean

# Start fresh
mise install
mise backend:run
```

## Getting Help

- Check logs: `docker-compose logs <service>`
- Verbose CLI output: `rise <command> --verbose`
- Backend logs: `RUST_LOG=debug cargo run --bin rise -- backend server`
- Report issues: https://github.com/anthropics/rise/issues
