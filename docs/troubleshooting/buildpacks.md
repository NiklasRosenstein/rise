# Troubleshooting Buildpack Builds

## CA Certificate Verification Errors

### Symptoms

When running `rise deployment create`, the build fails with:

```
===> ANALYZING
[analyzer] ERROR: failed to initialize analyzer: validating registry read access to <registry>
ERROR: failed to build: executing lifecycle: failed with status code: 1
```

Additional symptoms:
- Error mentions "validating registry read access"
- Occurs in corporate networks with HTTPS-intercepting proxies
- Occurs with internal registries using custom CA certificates

### Root Cause

This error occurs when the **pack lifecycle container** cannot verify SSL certificates while accessing the container registry.

**How pack builds work**:
1. Pack CLI starts a lifecycle container (e.g., `pack.local/lifecycle/...`)
2. The lifecycle analyzer makes HTTPS calls to the registry API to check for existing images
3. The lifecycle container doesn't have your custom CA certificates installed
4. SSL verification fails with the cryptic "validating registry read access" error

**Key insight**: The lifecycle container makes direct HTTPS calls to the registry API (not via Docker daemon), so it needs CA certificates in its own filesystem.

### Solution: Set SSL_CERT_FILE

Rise CLI automatically detects and uses the standard `SSL_CERT_FILE` environment variable:

```bash
# Export your CA certificate path
export SSL_CERT_FILE=/path/to/your/ca-cert.crt

# Build and deploy normally
rise deployment create my-app
```

Rise CLI will automatically:
1. Inject the certificate into the pack lifecycle container via `--volume`
2. Set `SSL_CERT_FILE` env var in the pack command
3. Build locally (without `--publish`)
4. Push the image to the registry after pack succeeds

### Manual Workaround (Without Rise CLI)

If using pack CLI directly, you can manually inject CA certificates:

```bash
# Build locally without --publish
pack build my-image \
  --builder paketobuildpacks/builder:base \
  --env SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
  --volume $SSL_CERT_FILE:/etc/ssl/certs/ca-certificates.crt:ro

# Then push manually
docker push my-image
```

### Troubleshooting

**Certificate not being picked up?**

Different base images look for CA certificates in different locations. Try these alternatives:

```bash
# Debian/Ubuntu-based images
export SSL_CERT_FILE=/path/to/ca-cert.crt

# If that doesn't work, try system CA bundle
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
```

**Still failing?**

1. Verify your CA certificate is valid PEM format:
   ```bash
   openssl x509 -in /path/to/ca-cert.crt -text -noout
   ```

2. Check pack build with verbose logging:
   ```bash
   rise deployment create my-app --verbose
   ```

3. Test registry access with curl using the same certificate:
   ```bash
   curl --cacert /path/to/ca-cert.crt https://your-registry.example.com/v2/
   ```

### Additional Resources

- [Pack CLI Volume Mounts](https://buildpacks.io/docs/for-app-developers/how-to/build-inputs/use-volume-mounts/)
- [Cloud Native Buildpacks Lifecycle](https://buildpacks.io/docs/concepts/components/lifecycle/)
