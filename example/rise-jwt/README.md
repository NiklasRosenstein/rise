# Rise JWT Claims Viewer Example

This example demonstrates how to decode and display the `rise_jwt` cookie that Rise sets after OAuth authentication.

## What This Example Shows

- How to read the `rise_jwt` HttpOnly cookie server-side
- **How to validate JWT signatures using OpenID Connect Discovery**
- How to verify JWT issuer and expiration
- How to decode a JWT token (Base64 URL decoding)
- How to extract and display JWT claims
- JWT claim structure including:
  - User information (`sub`, `email`, `name`)
  - Token metadata (`iss`, `aud`, `iat`, `exp`)
  - Team memberships (`groups`)

## How It Works

When you authenticate with Rise:

1. Rise performs OAuth authentication with your identity provider (e.g., Dex, Azure AD)
2. Rise issues its own JWT token containing user information (signed with RS256)
3. The JWT is stored in an **HttpOnly** `rise_jwt` cookie
4. The browser automatically sends this cookie with requests to your deployed app
5. Your server-side code reads the cookie, validates the signature using JWKS from the OpenID discovery endpoint, and extracts claims

**Important:**
- The `rise_jwt` cookie is HttpOnly (JavaScript cannot access it - XSS protection)
- JWTs are signed with RS256 (asymmetric cryptography)
- You should **always validate** the JWT signature before trusting the claims
- This example fetches JWKS from `${RISE_ISSUER}/.well-known/openid-configuration` (standard OpenID Connect Discovery)
- The JWKS is cached for 1 hour to avoid excessive requests
- This example includes proper validation using the `jsonwebtoken`, `jwk-to-pem`, and `node-fetch` libraries

## Deploying This Example

### Prerequisites

- Rise CLI installed and configured
- Access to a Rise instance (e.g., `http://localhost:3000`)
- Logged in with `rise login`

### Deploy

```bash
# Create a project for this example
rise project create rise-jwt-demo --access-class public

# Deploy the example
rise deployment create rise-jwt-demo example/rise-jwt

# Access the deployed app
# The URL will be shown after deployment completes
```

After deployment, navigate to your app's URL. If you're already logged in to Rise, you'll see your decoded JWT claims.

## JWT Claims Reference

### Standard Claims

- **`sub`** (Subject): Unique user identifier from the identity provider
- **`email`**: User's email address
- **`name`**: User's display name (optional)
- **`iss`** (Issuer): Rise backend URL
- **`aud`** (Audience): Rise public URL (for UI login) or project URL (for ingress auth)
- **`iat`** (Issued At): Unix timestamp when token was issued
- **`exp`** (Expires): Unix timestamp when token expires (default: 24 hours)

### Rise-Specific Claims

- **`groups`**: Array of Rise team names the user belongs to

## Environment Variables

When deployed on Rise, your application automatically receives:

- **`RISE_JWKS`**: JSON Web Key Set containing RS256 public keys for JWT signature validation
  ```json
  {
    "keys": [
      {
        "kty": "RSA",
        "kid": "unique-key-id",
        "n": "...",
        "e": "AQAB"
      }
    ]
  }
  ```

- **`RISE_ISSUER`**: The Rise backend URL (used to validate the `iss` claim)
  - Example: `http://localhost:3000` or `https://rise.example.com`

- **`RISE_APP_URLS`**: JSON array of all URLs your app is accessible at
  - Example: `["http://myapp.rise.local:8080", "https://custom.example.com"]`
  - Includes the primary ingress URL and all custom domains
  - Useful for CORS configuration, redirect validation, etc.

For local testing without Rise deployment, the example will fall back to insecure decode-only mode with a warning.

## Security Notes

1. **JWT Signature Validation**: This example properly validates JWTs using RS256:
   - Signature is verified using public keys from `RISE_JWKS`
   - Issuer (`iss`) is validated against `RISE_ISSUER`
   - Expiration (`exp`) is automatically checked by `jsonwebtoken`
   - **Never trust JWT claims without signature validation!**

2. **HttpOnly Cookie**: The `rise_jwt` cookie is marked HttpOnly:
   - JavaScript cannot access it via `document.cookie` (XSS protection)
   - Only server-side code can read this cookie
   - Cookie is automatically sent by the browser with same-domain requests

3. **RS256 (Asymmetric Cryptography)**:
   - JWTs are signed with a private key (held by Rise backend)
   - Verification uses public keys (distributed via `RISE_JWKS`)
   - No shared secrets needed in your application
   - Public keys can be safely exposed to your application

4. **Local Testing**: When running locally without Rise deployment:
   - `RISE_JWKS` will not be set
   - Example falls back to decode-only mode (INSECURE)
   - A warning is displayed and logged
   - **Never deploy without proper JWT validation!**

## Learn More

- [Rise JWT Authentication Documentation](../../docs/authentication-for-apps.md)
- [JWT.io](https://jwt.io/) - JWT debugger and documentation
- [RFC 7519](https://datatracker.ietf.org/doc/html/rfc7519) - JSON Web Token standard
