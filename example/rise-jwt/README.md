# Rise JWT Claims Viewer Example

This example demonstrates how to decode and display the `rise_jwt` cookie that Rise sets after OAuth authentication.

## What This Example Shows

- How to read the `rise_jwt` HttpOnly cookie server-side
- How to decode a JWT token (Base64 URL decoding)
- How to extract and display JWT claims
- JWT claim structure including:
  - User information (`sub`, `email`, `name`)
  - Token metadata (`iss`, `aud`, `iat`, `exp`)
  - Team memberships (`groups`)

## How It Works

When you authenticate with Rise:

1. Rise performs OAuth authentication with your identity provider (e.g., Dex, Azure AD)
2. Rise issues its own JWT token containing user information
3. The JWT is stored in an **HttpOnly** `rise_jwt` cookie
4. The browser automatically sends this cookie with requests to your deployed app
5. Your server-side code can read the cookie from request headers and decode it

**Important:** The `rise_jwt` cookie is HttpOnly, which means JavaScript cannot access it via `document.cookie`. This is a security feature that protects against XSS attacks. Only server-side code can read this cookie.

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

## Security Notes

1. **HttpOnly Cookie**: The `rise_jwt` cookie is marked HttpOnly, which means:
   - JavaScript cannot access it via `document.cookie` in production
   - This protects against XSS attacks
   - For this example to work when deployed, you'd need server-side access to the cookie

2. **Client-Side Decoding**: This example decodes the JWT client-side for demonstration.
   In a real application, you'd typically:
   - Validate the JWT signature server-side
   - Verify the expiration time
   - Check the audience (`aud`) matches your app

3. **No Secret Required**: JWT verification only requires the public key (for RS256) or the shared secret (for HS256).
   Rise provides the public keys via the `RISE_JWKS` environment variable for deployed apps.

## Learn More

- [Rise JWT Authentication Documentation](../../docs/authentication-for-apps.md)
- [JWT.io](https://jwt.io/) - JWT debugger and documentation
- [RFC 7519](https://datatracker.ietf.org/doc/html/rfc7519) - JSON Web Token standard
