# Authentication

Rise uses JWT tokens issued by Dex OAuth2/OIDC.

## Device Flow (Recommended)

Browser-based OAuth-style flow:

```bash
rise-cli login
```

Opens browser to authenticate, returns token to CLI.

## Password Authentication

For testing or automation:

```bash
rise-cli login --email user@example.com --password secret
```

## Token Storage

Tokens stored in `~/.config/rise/config.json`:

```json
{
  "backend_url": "http://127.0.0.1:3000",
  "token": "eyJhbG..."
}
```

## API Usage

All endpoints require `Authorization: Bearer <token>`:

```bash
curl http://localhost:3000/projects \
  -H "Authorization: Bearer YOUR_TOKEN"
```

Without token: `401 Unauthorized`
