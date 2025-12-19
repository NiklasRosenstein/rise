# Rise Examples

This directory contains example applications demonstrating various Rise features and deployment patterns.

## Examples

### Hello World Examples

Simple static and dynamic applications to get started with Rise.

- **[hello-world](./hello-world/)** - Static HTML page served with nginx
- **[hello-world-js](./hello-world-js/)** - Node.js Express application
- **[hello-world-py](./hello-world-py/)** - Python Flask application

### OAuth Examples

Demonstrate the OAuth 2.0 extension for end-user authentication.

- **[oauth-fragment-flow](./oauth-fragment-flow/)** - Fragment-based OAuth flow for SPAs
  - Tokens delivered in URL fragment (`#access_token=...`)
  - Best for: React, Vue, Angular, vanilla JavaScript
  - Security: Tokens never sent to server, no logs/referer leaks

- **[oauth-exchange-flow](./oauth-exchange-flow/)** - Exchange token flow for backend apps
  - Temporary token exchanged server-side
  - Best for: Rails, Django, Express, server-rendered apps
  - Security: HttpOnly cookies, XSS-safe, tokens never exposed to browser

## Quick Start

Each example includes its own README with detailed setup instructions.

### General Steps

1. **Start local development environment**:
   ```bash
   docker-compose up -d
   ```

2. **Login to Rise**:
   ```bash
   rise login
   ```

3. **Create a project** (if needed):
   ```bash
   rise project create <project-name>
   ```

4. **Deploy the example**:
   ```bash
   cd example/<example-name>
   rise deployment create <project-name>
   ```

## OAuth Examples Setup

The OAuth examples require additional setup to create the OAuth extension:

1. **Create the OAuth extension**:
   ```bash
   # Store Dex client secret
   rise env set oauth-demo DEX_CLIENT_SECRET "rise-backend-secret" --secret

   # Create OAuth extension
   rise extension create oauth-demo oauth-dex \
     --type oauth \
     --spec '{
       "provider_name": "Dex (Local Dev)",
       "description": "Local Dex OIDC provider for development",
       "client_id": "rise-backend",
       "client_secret_ref": "DEX_CLIENT_SECRET",
       "authorization_endpoint": "http://localhost:5556/dex/auth",
       "token_endpoint": "http://localhost:5556/dex/token",
       "scopes": ["openid", "email", "profile"]
     }'
   ```

2. **Restart Dex** (callback URLs already configured):
   ```bash
   docker-compose restart dex
   ```

3. **Deploy the example**:
   ```bash
   cd example/oauth-fragment-flow  # or oauth-exchange-flow
   rise deployment create oauth-demo
   ```

4. **Test locally** (optional):
   - Fragment flow: Open `public/index.html` in browser with local server
   - Exchange flow: Run `npm install && npm start` and visit `http://localhost:8080`

## Default Dex Credentials

For local development with Dex:

- **Email**: `admin@example.com`
- **Password**: `password`

Or:

- **Email**: `test@example.com`
- **Password**: `password`

## Documentation

For more details on Rise features, see the main documentation:

- [OAuth Extension](../docs/oauth.md)
- [Build Backends](../docs/builds.md)
- [Deployments](../docs/deployments.md)
- [Configuration](../docs/configuration.md)

## Contributing

When adding new examples:

1. Create a new directory under `example/`
2. Include a README with setup instructions
3. Add entry to this README
4. Keep examples minimal and focused on one feature
