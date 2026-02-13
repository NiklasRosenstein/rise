# Environment Variables

Rise manages environment variables at the project level. Variables are injected into containers at deployment time.

## Managing Variables

### Setting Variables

```bash
# Plain text variable
rise env set my-app LOG_LEVEL info

# Secret variable (encrypted at rest, masked in listings)
rise env set my-app DATABASE_URL postgres://user:pass@db/mydb --secret

# Protected secret (encrypted, cannot be retrieved via API)
rise env set my-app JWT_SECRET supersecret --secret --protected
```

With a `rise.toml` in your directory, you can omit the project name:

```bash
rise env set LOG_LEVEL info
```

### Listing Variables

```bash
rise env list my-app
```

Secret values are masked. Protected secrets cannot be decrypted.

### Getting a Single Variable

```bash
rise env get my-app LOG_LEVEL
```

### Deleting Variables

```bash
rise env delete my-app LOG_LEVEL
```

Aliases: `rise env unset`, `rise env rm`, `rise env del`

## Importing from a File

Import variables from a `.env`-style file:

```bash
rise env import my-app .env
```

File format:

```
# Comments are supported
LOG_LEVEL=info
DATABASE_URL=secret:postgres://user:pass@db/mydb
API_KEY=secret:s3cret
```

Prefix a value with `secret:` to store it as a secret variable.

## Deployment Snapshots

View the environment variables that were active for a specific deployment:

```bash
rise env show-deployment my-app 20241205-1234
```

This is a read-only view of the variables as they existed when the deployment was created.

## Auto-Injected Variables

Rise automatically injects these variables into every deployment:

| Variable | Description | Example |
|----------|-------------|---------|
| `PORT` | HTTP port the container should listen on | `8080` |
| `RISE_ISSUER` | Rise server URL and JWT issuer | `https://rise.example.com` |
| `RISE_APP_URL` | Canonical URL (primary custom domain or default URL) | `https://myapp.example.com` |
| `RISE_APP_URLS` | JSON array of all URLs for the app | `["https://myapp.app.example.com"]` |

`PORT` defaults to 8080. Override it per-deployment with `--http-port` on `rise deploy`, or set it permanently with `rise env set my-app PORT 3000`.

## Build-Time vs Runtime Variables

| Aspect | Build-Time | Runtime |
|--------|-----------|---------|
| **Purpose** | Configure build process (compiler flags, tool versions) | Configure running application |
| **Set via** | `-e` flag or `[build] env` in `rise.toml` | `rise env set` |
| **Available during** | Image build only | Container runtime |
| **Storage** | Ephemeral (not persisted) | Database (encrypted for secrets) |
| **Examples** | `NODE_ENV`, `BUILD_VERSION` | `DATABASE_URL`, `API_KEY` |

See [Building Images](builds.md#build-time-environment-variables) for build-time variable details.

## Variables in rise.toml

You can define plain-text environment variables in `rise.toml`:

```toml
[project]
name = "my-app"

[project.env]
LOG_LEVEL = "info"
APP_MODE = "production"
```

These are synced to the backend when you run `rise project create` or `rise project update --sync`.
