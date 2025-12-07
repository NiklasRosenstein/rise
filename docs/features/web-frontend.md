# Web Frontend

The Rise backend includes an embedded web frontend for viewing and managing projects, teams, and deployments through a browser.

## Overview

The web UI provides a read-only dashboard for monitoring your Rise infrastructure. All static assets (HTML, CSS, JavaScript) are embedded directly into the backend binary using `rust-embed`, so no external files or build steps are needed.

## Accessing the Frontend

1. **Start the backend server**:
   ```bash
   mise backend:run
   # Or manually: cargo run --bin rise -- backend server
   ```

2. **Open your browser** to http://localhost:3000

3. **Login** by clicking "Login with OAuth" to authenticate via Dex

Default development credentials:
- **Email**: `admin@example.com` or `test@example.com`
- **Password**: `password`

## Features

### OAuth2 PKCE Authentication

The web frontend uses the same secure OAuth2 PKCE flow as the CLI:

- **Browser-based**: No passwords sent to Rise backend
- **PKCE**: Proof Key for Code Exchange prevents authorization code interception
- **Token storage**: Access and refresh tokens stored in browser localStorage
- **Auto-refresh**: Tokens automatically refreshed when expired

### Projects Dashboard

View all projects you have access to:

- Project name and owner
- Current status (`running`, `stopped`, `pending`)
- Visibility (`public` or `private`)
- Deployment URL (when available)
- Quick access to deployment details

### Teams Dashboard

Manage and view teams:

- Team name
- Member count
- Owner list
- Access control information

### Deployment Tracking

Real-time monitoring of deployments:

- **Auto-refresh**: Deployment status updates every 3-5 seconds for active deployments
- **Status indicators**: Visual indicators for `running`, `pending`, `failed`, `stopped` states
- **Deployment history**: View all deployments for a project
- **Deployment groups**: Filter by deployment group (`default`, `mr/123`, etc.)

### Deployment Logs

View build and deployment logs inline:

- Build logs from container image builds
- Deployment events and errors
- Real-time log streaming for active deployments

### Responsive Design

The UI works on both desktop and mobile browsers:

- Clean, minimal design using Pico CSS
- No JavaScript framework overhead
- Fast page loads
- Accessible on mobile devices

## Technology Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Backend** | Axum + rust-embed | Serves embedded static assets |
| **Frontend** | Vanilla HTML/CSS/JavaScript | No build step required |
| **CSS Framework** | Pico CSS | Classless, minimal styling |
| **Authentication** | OAuth2 PKCE | Web Crypto API for SHA-256 |
| **Real-time Updates** | JavaScript polling | 3-5 second intervals for active deployments |

### Why Vanilla JavaScript?

Rise uses vanilla JavaScript without a framework to:

- **Eliminate build complexity**: No npm, webpack, or build pipeline
- **Reduce binary size**: Smaller embedded assets
- **Simplify development**: Edit HTML/CSS/JS and reload
- **Improve performance**: No framework overhead

All static files are served from the binary at compile time, making deployment as simple as copying a single executable.

## Embedded Assets

Static assets are embedded using `rust-embed`:

```rust
#[derive(RustEmbed)]
#[folder = "rise-backend/static/"]
struct Asset;
```

This means:
- **Single binary deployment**: No external files needed
- **Immutable assets**: Assets can't be modified after build
- **Fast serving**: Assets served from memory
- **Simple deployment**: Just copy the binary

## Development Workflow

### Editing the Frontend

1. **Edit static files** in `rise-backend/static/`:
   - `index.html` - Main HTML
   - `styles.css` - Custom CSS
   - `app.js` - JavaScript logic

2. **Rebuild the backend**:
   ```bash
   cargo build --bin rise
   ```

3. **Restart the server**:
   ```bash
   mise backend:reload
   ```

4. **Refresh your browser** to see changes

### Hot Reload

For faster development, you can use a simple HTTP server to serve static files without rebuilding:

```bash
cd rise-backend/static
python3 -m http.server 8080
```

Then modify the Axum routes temporarily to proxy to `localhost:8080` during development.

## Future Enhancements

Planned features for the web frontend:

- **Write operations**: Create projects, trigger deployments from the UI
- **User management**: Manage team memberships via UI
- **Service account management**: Create and revoke service accounts
- **Deployment controls**: Stop, rollback, and delete deployments
- **Advanced filtering**: Search and filter projects/deployments
- **Dark mode**: Toggle between light and dark themes

## Security Considerations

The web frontend follows security best practices:

- **No passwords in URL**: OAuth2 flow keeps credentials secure
- **Token storage**: Tokens stored in localStorage (future: consider httpOnly cookies)
- **HTTPS recommended**: Use HTTPS in production to protect tokens
- **CSRF protection**: Future enhancement for write operations
- **Content Security Policy**: Future enhancement to prevent XSS

## Next Steps

- **Setup authentication**: See [Authentication](../core-concepts/authentication.md)
- **Deploy your first app**: See [Getting Started](../getting-started/README.md)
- **Learn about projects**: See [Projects & Teams](../core-concepts/projects-teams.md)
