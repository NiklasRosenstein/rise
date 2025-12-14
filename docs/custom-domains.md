# Custom Domains

Rise supports configuring custom domain names for your projects. This allows you to serve your applications from your own domain (e.g., `app.example.com`) instead of the default Rise domain.

## Overview

Custom domains work by configuring DNS CNAME records that point to your project's default hostname. Rise verifies domain ownership through DNS lookups before allowing the domain to be used.

## Features

- **Multiple Domains**: Add multiple custom domains to a single project
- **DNS Verification**: Automatic verification of CNAME configuration
- **Status Tracking**: Monitor verification and certificate status
- **Simple CLI**: Easy-to-use commands for domain management

## Quick Start

### 1. Add a Custom Domain

```bash
rise domain add my-project example.com
```

This command will:
- Register the domain with your project
- Provide CNAME configuration instructions
- Return the domain status

### 2. Configure DNS

Add a CNAME record in your DNS provider:

```
Name:  example.com (or the specific subdomain)
Type:  CNAME
Value: my-project.rise.dev (or your project's hostname)
TTL:   3600 (or as appropriate)
```

**Note**: DNS propagation can take anywhere from a few minutes to 48 hours depending on your DNS provider.

### 3. Wait for Automatic Verification

Rise automatically verifies pending domains every 5 minutes. Once your DNS configuration propagates, the domain will be automatically verified without any manual action.

You can check the current status:

```bash
rise domain list my-project
```

**Optional Manual Verification**: If you want to verify immediately without waiting:

```bash
rise domain verify my-project example.com
```

This will check that the CNAME record is properly configured and mark the domain as verified if successful.

### 4. List Your Domains

View all configured domains for a project:

```bash
rise domain list my-project
```

## CLI Commands

### Add Domain

```bash
rise domain add <project> <domain>
```

Adds a new custom domain to the specified project.

**Example**:
```bash
rise domain add my-app www.example.com
```

### List Domains

```bash
rise domain list <project>
```

Lists all custom domains configured for a project with their verification and certificate status.

**Example**:
```bash
rise domain list my-app
```

### Delete Domain

```bash
rise domain delete <project> <domain>
```

Removes a custom domain from the project.

**Example**:
```bash
rise domain delete my-app www.example.com
```

### Verify Domain

```bash
rise domain verify <project> <domain>
```

Verifies that the domain's DNS configuration is correct.

**Example**:
```bash
rise domain verify my-app www.example.com
```

## Domain Status

Custom domains have two status fields:

### Verification Status

- **Pending**: Domain added but not yet verified
- **Verified**: DNS configuration confirmed
- **Failed**: Verification failed (check DNS configuration)

### Certificate Status

- **None**: No certificate requested
- **Pending**: Certificate issuance in progress (future feature)
- **Issued**: Valid SSL certificate active (future feature)
- **Failed**: Certificate issuance failed (future feature)
- **Expired**: Certificate needs renewal (future feature)

## API Endpoints

For programmatic access, use the following REST API endpoints:

### Add Domain

```http
POST /projects/{project}/domains
Content-Type: application/json

{
  "domain_name": "example.com"
}
```

### List Domains

```http
GET /projects/{project}/domains
```

### Delete Domain

```http
DELETE /projects/{project}/domains/{domain}
```

### Verify Domain

```http
POST /projects/{project}/domains/{domain}/verify
```

### Get ACME Challenges

```http
GET /projects/{project}/domains/{domain}/challenges
```

## Kubernetes Integration

When using the Kubernetes deployment backend, verified custom domains are automatically added to the Ingress resource. This allows traffic from your custom domain to reach your application.

### Ingress Configuration

The Kubernetes controller creates Ingress rules for:
1. The default project hostname (e.g., `my-project.rise.dev`)
2. All verified custom domains

All domains route to the same service, ensuring your application is accessible from multiple hostnames.

## SSL/TLS Certificates

**Note**: Automatic SSL certificate provisioning via ACME/Let's Encrypt is planned but not yet implemented.

### Current State

- Domain verification is functional
- Certificate status tracking is in place
- ACME challenge tables are created

### Future Implementation

The planned implementation will:
1. Use DNS-01 challenges for domain validation
2. Automatically request certificates from Let's Encrypt
3. Store certificates securely (encrypted in database or as K8s secrets)
4. Automatically renew certificates before expiration
5. Configure TLS in Ingress resources

For now, you can:
- Use a wildcard certificate configured via `ingress_tls_secret_name`
- Configure cert-manager separately to handle certificates
- Use external certificate management

## Troubleshooting

### Domain Verification Fails

**Problem**: `rise domain verify` reports verification failed

**Solutions**:
1. Wait for DNS propagation (can take up to 48 hours)
2. Verify CNAME record is correct using `dig` or `nslookup`:
   ```bash
   dig example.com CNAME
   nslookup example.com
   ```
3. Ensure the CNAME points to the correct target
4. Check with your DNS provider for configuration issues

### Domain Not Accessible

**Problem**: Domain is verified but application not accessible

**Solutions**:
1. Verify the project has an active deployment
2. Check Kubernetes Ingress is created: `kubectl get ingress -n rise-<project>`
3. Ensure Ingress controller is running
4. Check application logs for errors

### Multiple Domains

**Problem**: Need to configure multiple subdomains

**Solution**: Add each subdomain separately:
```bash
rise domain add my-app www.example.com
rise domain add my-app api.example.com
rise domain add my-app admin.example.com
```

## Limitations

Current limitations:
- ACME/Let's Encrypt integration not yet implemented
- No automatic certificate renewal
- No support for HTTP-01 challenges (only DNS-01 planned)
- Certificate provisioning requires manual setup or external tools

## Security Considerations

- Domain ownership is verified through DNS lookups
- Only project owners and team members can add domains
- Domain verification prevents unauthorized domain usage
- All domains follow project visibility settings (public/private)

## Related Documentation

- [Kubernetes Deployment](kubernetes.md)
- [CLI Commands](cli.md)
- [Project Management](../README.md#project-management)
