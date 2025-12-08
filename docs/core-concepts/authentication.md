# Authentication

Rise uses JWT tokens issued by Dex OAuth2/OIDC provider. The CLI supports two OAuth2 authentication flows.

## Browser Flow (Default, Recommended)

OAuth2 authorization code flow with PKCE (Proof Key for Code Exchange):

```bash
rise login
```

Or explicitly:

```bash
rise login --browser
```

**How it works:**
1. CLI starts a local HTTP server on ports 8765-8767 to receive the OAuth callback
2. Opens browser to Dex authentication page
3. User authenticates with Dex (username/password or other configured methods)
4. Dex redirects to `http://localhost:8765/callback` with authorization code
5. CLI exchanges code with backend at `/auth/code/exchange`
6. Backend validates code with Dex using client credentials and PKCE verifier
7. CLI receives and stores JWT token

**Advantages:**
- Standard OAuth2 flow (RFC 6749 + RFC 7636)
- More secure than password grant (credentials never pass through CLI)
- Fast and user-friendly
- Works reliably with Dex

## Device Flow (Not Compatible with Dex)

OAuth2 device authorization flow:

```bash
rise login --device
```

**⚠️ Warning:** Dex's device flow implementation doesn't follow RFC 8628 properly. It uses a hybrid approach that redirects the browser with an authorization code instead of returning the token via polling, which is incompatible with pure CLI implementations.

**Status:** Not recommended with Dex. Use the browser flow instead.

**Expected behavior with a compliant OAuth2 provider:**
1. CLI requests device code from OAuth provider
2. Displays user code and verification URL
3. Opens browser for user to enter code and authenticate
4. CLI polls token endpoint until authorization completes
5. Receives and stores JWT token

## Token Storage

Tokens are stored in `~/.config/rise/config.json`:

```json
{
  "backend_url": "http://localhost:3000",
  "token": "eyJhbG..."
}
```

**Security Note:** Tokens are currently stored in plain JSON. Future enhancement planned to use OS-native secure storage:
- macOS: Keychain
- Linux: Secret Service API / libsecret
- Windows: Credential Manager

## Backend URL

You can authenticate with a different backend:

```bash
rise login --url https://rise.example.com
```

The URL is saved and used for subsequent commands.

## API Usage

All protected endpoints require `Authorization: Bearer <token>`:

```bash
curl http://localhost:3000/projects \
  -H "Authorization: Bearer YOUR_TOKEN"
```

**Responses:**
- Without token: `401 Unauthorized`
- Invalid/expired token: `401 Unauthorized`
- Valid token: Success response

## Authentication Endpoints

### Public Endpoints (No Authentication Required)

- `POST /auth/code/exchange` - Exchange authorization code for JWT token
  ```json
  {
    "code": "authorization_code_from_dex",
    "code_verifier": "pkce_verifier",
    "redirect_uri": "http://localhost:8765/callback"
  }
  ```

  Response:
  ```json
  {
    "token": "eyJhbG..."
  }
  ```

### Protected Endpoints (Authentication Required)

- `GET /me` - Get current user information
  ```json
  {
    "id": "user-uuid",
    "email": "user@example.com"
  }
  ```

- `POST /users/lookup` - Lookup users by email addresses
  ```json
  {
    "emails": ["user1@example.com", "user2@example.com"]
  }
  ```

## Local Development

The default configuration assumes:
- Backend: `http://localhost:3000`
- Dex: `http://localhost:5556/dex`
- Local callback ports: 8765, 8766, 8767 (tries in order)

## Troubleshooting

### "Failed to start local callback server"

The CLI tries to bind to ports 8765, 8766, and 8767. If all are in use:
1. Close applications using these ports
2. Or use device flow (if using a compatible OAuth2 provider): `rise login --device`

### "Code exchange failed"

Common causes:
1. Backend is not running
2. Dex is not configured properly
3. Network connectivity issues

Check backend and Dex logs for details.

### Token Expired

Tokens have an expiration time (default: 1 hour). Re-authenticate:

```bash
rise login
```
