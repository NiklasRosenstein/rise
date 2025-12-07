# Docker (Local)

For local development, Rise includes a Docker registry running in `docker-compose` for storing container images.

## Overview

The local Docker registry allows you to:

- Push and pull images locally without external dependencies
- Test image builds and deployments
- Develop offline
- Avoid rate limits from Docker Hub

## Configuration

The registry is defined in `docker-compose.yml`:

```yaml
registry:
  image: registry:2
  container_name: rise-registry
  restart: unless-stopped
  ports:
    - "5000:5000"
  volumes:
    - registry_data:/var/lib/registry
  environment:
    - REGISTRY_STORAGE_DELETE_ENABLED=true
```

**Key settings**:
- **Port**: 5000 (HTTP, insecure for local development)
- **Storage**: Docker volume `registry_data` for persistence
- **Delete enabled**: Allows image deletion via API

## Starting the Registry

The registry starts automatically with other dependencies:

```bash
mise backend:deps
```

Or start it individually:

```bash
docker-compose up -d registry
```

## Accessing the Registry

### Registry API

The registry is available at `http://localhost:5000`:

```bash
# List repositories
curl http://localhost:5000/v2/_catalog

# List tags for a repository
curl http://localhost:5000/v2/my-app/tags/list

# Get manifest
curl http://localhost:5000/v2/my-app/manifests/latest
```

### Registry UI

A web UI is available at `http://localhost:5001` for browsing images:

```bash
docker-compose up -d registry-ui
```

The UI provides:
- Repository browser
- Tag listing
- Image layer inspection
- Deletion interface (when enabled)

## Using the Local Registry

### Pushing Images

Tag and push images to the local registry:

```bash
# Tag image
docker tag my-app:latest localhost:5000/my-app:latest

# Push to local registry
docker push localhost:5000/my-app:latest
```

### Pulling Images

Pull images from the local registry:

```bash
docker pull localhost:5000/my-app:latest
```

### Deploying with Rise

Rise is configured to use the local registry in development:

```bash
# This automatically uses localhost:5000
rise deployment create my-app
```

The backend configuration (`rise-backend/config/local.toml`) specifies the Docker registry provider.

## Persistence

Registry data is stored in a Docker volume:

```bash
# View volume
docker volume inspect rise_registry_data

# Remove volume (deletes all images!)
docker-compose down -v
```

## Troubleshooting

### "Connection refused" to registry

Ensure the registry is running:

```bash
docker-compose ps registry
docker-compose logs registry
```

Restart if needed:

```bash
docker-compose restart registry
```

### Images not persisting

Check that the volume is configured:

```bash
docker volume ls | grep registry
```

### Registry UI not showing images

Ensure the registry UI can reach the registry:

```bash
docker-compose logs registry-ui
```

## Production Considerations

**Warning**: This local registry is for development only. For production:

- **Use AWS ECR, GCR, or similar**: See [AWS ECR](./aws-ecr.md)
- **Enable TLS**: The local registry uses HTTP (insecure)
- **Add authentication**: The local registry has no auth
- **Use persistent storage**: Cloud-backed storage, not Docker volumes
- **Enable monitoring**: Track usage and errors

## Next Steps

- **Deploy to AWS ECR**: See [AWS ECR](./aws-ecr.md)
- **Understand registries**: See [Container Registry](../features/registry.md)
- **Local development**: See [Local Development](../getting-started/local-development.md)
