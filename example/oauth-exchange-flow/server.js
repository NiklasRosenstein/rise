const express = require('express');
const session = require('express-session');
const fetch = require('node-fetch');
const jose = require('jose');

const app = express();
const PORT = process.env.PORT || 8080;

// Helper to get required env var (fails fast if missing)
function requireEnv(name, description) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}${description ? ` (${description})` : ''}`);
  }
  return value;
}

// Configuration - adjust for your setup.
const CONFIG = {
  // RISE_ISSUER: Rise server URL (base URL for all Rise endpoints)
  riseIssuer: process.env.RISE_ISSUER || 'http://localhost:3000',
  projectName: process.env.PROJECT_NAME || 'oauth-demo',
  extensionName: 'oauth-dex',
  sessionSecret: process.env.SESSION_SECRET || 'change-this-in-production',
  // OAuth credentials injected by Rise as {EXT}_CLIENT_ID, {EXT}_CLIENT_SECRET, {EXT}_ISSUER
  // These are REQUIRED - fail fast if not set
  clientId: requireEnv(`OAUTH_DEX_CLIENT_ID`, 'Rise OAuth client ID'),
  clientSecret: requireEnv(`OAUTH_DEX_CLIENT_SECRET`, 'Rise OAuth client secret'),
  // OIDC issuer for id_token validation via JWKS discovery
  oidcIssuer: requireEnv(`OAUTH_DEX_ISSUER`, 'OIDC issuer URL'),
};

// Session middleware for storing OAuth tokens
app.use(session({
  secret: CONFIG.sessionSecret,
  resave: false,
  saveUninitialized: false,
  cookie: {
    httpOnly: true,  // Protect against XSS
    secure: false,    // Set to true in production with HTTPS
    maxAge: 24 * 60 * 60 * 1000  // 24 hours
  }
}));

// Serve static files
app.use(express.static('public'));

// JWKS cache for id_token validation
let cachedJwks = null;
let jwksExpiry = 0;

async function getJwks() {
  if (cachedJwks && Date.now() < jwksExpiry) {
    return cachedJwks;
  }

  // Fetch OIDC discovery document (standard: {issuer}/.well-known/openid-configuration)
  const discoveryUrl = `${CONFIG.oidcIssuer}/.well-known/openid-configuration`;
  const discoveryRes = await fetch(discoveryUrl);
  if (!discoveryRes.ok) {
    throw new Error(`Failed to fetch OIDC discovery from ${discoveryUrl}: ${discoveryRes.status}`);
  }
  const discovery = await discoveryRes.json();

  // Fetch JWKS from jwks_uri
  const jwksRes = await fetch(discovery.jwks_uri);
  if (!jwksRes.ok) {
    throw new Error(`Failed to fetch JWKS: ${jwksRes.status}`);
  }
  cachedJwks = await jwksRes.json();
  jwksExpiry = Date.now() + 3600000; // Cache for 1 hour

  return cachedJwks;
}

async function validateIdToken(idToken) {
  const jwks = await getJwks();
  const JWKS = jose.createLocalJWKSet(jwks);

  // Verify signature and decode
  const { payload } = await jose.jwtVerify(idToken, JWKS);

  return payload;
}

// Home page - check if user is logged in
app.get('/', (req, res) => {
  if (req.session.oauth) {
    // User is logged in - show profile
    res.send(renderProfilePage(req.session.oauth));
  } else {
    // User is not logged in - show login page
    res.send(renderLoginPage());
  }
});

// Initiate OAuth flow
app.get('/login', (req, res) => {
  // Build the OAuth authorization URL (uses RISE_ISSUER for browser redirect)
  const authUrl = new URL(
    `/oidc/${CONFIG.projectName}/${CONFIG.extensionName}/authorize`,
    CONFIG.riseIssuer
  );

  // Set redirect URI to our callback
  const redirectUri = `${req.protocol}://${req.get('host')}/oauth/callback`;
  authUrl.searchParams.set('redirect_uri', redirectUri);

  // Optional: Add state for CSRF protection
  const state = generateState();
  req.session.oauthState = state;
  authUrl.searchParams.set('state', state);

  // Redirect to OAuth flow
  res.redirect(authUrl.toString());
});

// OAuth callback handler
app.get('/oauth/callback', async (req, res) => {
  try {
    const { code, state } = req.query;

    // Verify state (CSRF protection)
    if (req.session.oauthState && req.session.oauthState !== state) {
      return res.status(400).send(renderErrorPage('State mismatch - possible CSRF attack'));
    }
    delete req.session.oauthState;

    if (!code) {
      return res.status(400).send(renderErrorPage('No authorization code received'));
    }

    // Exchange the authorization code for OAuth tokens (uses RISE_ISSUER for backend call)
    const tokenUrl = new URL(
      `/oidc/${CONFIG.projectName}/${CONFIG.extensionName}/token`,
      CONFIG.riseIssuer
    );

    // RFC 6749: redirect_uri MUST match the authorization request
    const redirectUri = `${req.protocol}://${req.get('host')}/oauth/callback`;

    const response = await fetch(tokenUrl.toString(), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      body: new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        client_id: CONFIG.clientId,
        client_secret: CONFIG.clientSecret,
        redirect_uri: redirectUri,
      }),
    });

    if (!response.ok) {
      const error = await response.json();
      return res.status(response.status).send(
        renderErrorPage(`Token exchange failed: ${error.error} - ${error.error_description || ''}`)
      );
    }

    const tokens = await response.json();

    // Store credentials in session (HttpOnly cookie)
    req.session.oauth = {
      accessToken: tokens.access_token,
      idToken: tokens.id_token,
      tokenType: tokens.token_type,
      expiresIn: tokens.expires_in,
      refreshToken: tokens.refresh_token,
      scope: tokens.scope,
      retrievedAt: new Date().toISOString()
    };

    // Redirect to home page
    res.redirect('/');
  } catch (error) {
    console.error('OAuth callback error:', error);
    res.status(500).send(renderErrorPage(`Server error: ${error.message}`));
  }
});

// Logout
app.get('/logout', (req, res) => {
  req.session.destroy((err) => {
    if (err) {
      console.error('Logout error:', err);
    }
    res.redirect('/');
  });
});

// API endpoint that validates the id_token and returns user claims
app.get('/api/protected', async (req, res) => {
  if (!req.session.oauth?.idToken) {
    return res.status(401).json({ error: 'Not authenticated' });
  }

  try {
    // Validate id_token signature and decode claims
    const claims = await validateIdToken(req.session.oauth.idToken);

    res.json({
      message: 'Token validated successfully',
      sub: claims.sub,
      email: claims.email,
      name: claims.name,
      exp: claims.exp,
      iss: claims.iss,
    });
  } catch (error) {
    console.error('Token validation failed:', error);
    return res.status(401).json({ error: 'Invalid token', details: error.message });
  }
});

// HTML rendering functions
function renderLoginPage() {
  return `
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>OAuth Token Endpoint Flow - Login</title>
        <style>${getStyles()}</style>
    </head>
    <body>
        <div class="container">
            <h1>OAuth Token Endpoint Flow Example</h1>
            <div class="badge">Token Endpoint Flow (Backend Apps)</div>

            <p>
                This example demonstrates the <strong>RFC 6749-compliant token endpoint flow</strong> for server-rendered applications.
                The backend securely exchanges an authorization code for OAuth credentials using client credentials.
            </p>

            <button onclick="window.location.href='/login'">Login with OAuth</button>

            <div class="footer">
                <strong>How it works:</strong><br>
                1. Click login → redirect to Rise OAuth endpoint<br>
                2. Rise redirects to Dex for authentication<br>
                3. After auth, redirect with authorization code (5-min TTL)<br>
                4. Backend exchanges code for tokens via /oauth/token<br>
                5. Store in session (HttpOnly cookie, XSS-safe)
            </div>
        </div>
    </body>
    </html>
  `;
}

function renderProfilePage(oauth) {
  return `
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>OAuth Token Endpoint Flow - Profile</title>
        <style>${getStyles()}</style>
    </head>
    <body>
        <div class="container">
            <h1>OAuth Token Endpoint Flow Example</h1>
            <div class="badge">Token Endpoint Flow (Backend Apps)</div>

            <div class="status success">
                ✓ Successfully authenticated! OAuth tokens stored in session.
            </div>

            <div class="info">
                <h2>Session Information</h2>
                <p><strong>Token Type:</strong> ${oauth.tokenType}</p>
                <p><strong>Expires In:</strong> ${oauth.expiresIn} seconds</p>
                <p><strong>Retrieved At:</strong> ${oauth.retrievedAt}</p>
                <p><strong>Has ID Token:</strong> ${oauth.idToken ? 'Yes' : 'No'}</p>
                <p><strong>Has Refresh Token:</strong> ${oauth.refreshToken ? 'Yes' : 'No'}</p>
                <p><strong>Scopes:</strong> ${oauth.scope || 'N/A'}</p>
            </div>

            <div class="info">
                <h2>Security Benefits</h2>
                <ul>
                    <li>✓ Tokens stored in HttpOnly cookie (XSS-safe)</li>
                    <li>✓ Authorization code was single-use (5-min TTL)</li>
                    <li>✓ Client authenticated with client_secret</li>
                    <li>✓ OAuth tokens never exposed to browser</li>
                    <li>✓ CSRF protection via state parameter</li>
                    <li>✓ id_token validated via JWKS signature verification</li>
                </ul>
            </div>

            <div style="margin-top: 2rem;">
                <button onclick="testProtectedEndpoint()">Test Protected API</button>
                <button onclick="window.location.href='/logout'" class="secondary">Logout</button>
            </div>

            <div id="api-response" class="hidden"></div>

            <div class="footer">
                <strong>Note:</strong> In production, tokens are never shown to the user.
                They're used server-side to call protected APIs.
            </div>
        </div>

        <script>
            async function testProtectedEndpoint() {
                try {
                    const response = await fetch('/api/protected');
                    const data = await response.json();

                    const resultDiv = document.getElementById('api-response');
                    resultDiv.className = response.ok ? 'status success' : 'status error';
                    resultDiv.innerHTML = '<pre>' + JSON.stringify(data, null, 2) + '</pre>';
                    resultDiv.classList.remove('hidden');
                } catch (error) {
                    const resultDiv = document.getElementById('api-response');
                    resultDiv.className = 'status error';
                    resultDiv.textContent = 'Error: ' + error.message;
                    resultDiv.classList.remove('hidden');
                }
            }
        </script>
    </body>
    </html>
  `;
}

function renderErrorPage(message) {
  return `
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>OAuth Token Endpoint Flow - Error</title>
        <style>${getStyles()}</style>
    </head>
    <body>
        <div class="container">
            <h1>OAuth Token Endpoint Flow Example</h1>
            <div class="status error">
                ✗ Error: ${escapeHtml(message)}
            </div>
            <button onclick="window.location.href='/'">Back to Home</button>
        </div>
    </body>
    </html>
  `;
}

function getStyles() {
  return `
    * {
        margin: 0;
        padding: 0;
        box-sizing: border-box;
    }

    body {
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
        display: flex;
        justify-content: center;
        align-items: center;
        min-height: 100vh;
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        color: #333;
        padding: 2rem;
    }

    .container {
        background: white;
        padding: 2rem 3rem;
        border-radius: 16px;
        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
        max-width: 700px;
        width: 100%;
    }

    h1 {
        font-size: 2rem;
        margin-bottom: 1rem;
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        -webkit-background-clip: text;
        -webkit-text-fill-color: transparent;
        background-clip: text;
    }

    h2 {
        font-size: 1.3rem;
        margin-bottom: 0.75rem;
        color: #667eea;
    }

    p {
        font-size: 1rem;
        color: #666;
        margin-bottom: 1rem;
        line-height: 1.6;
    }

    .badge {
        display: inline-block;
        background: #f0f0f0;
        padding: 0.5rem 1rem;
        border-radius: 8px;
        font-size: 0.85rem;
        color: #667eea;
        font-weight: 600;
        margin-bottom: 1.5rem;
    }

    button {
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        color: white;
        border: none;
        padding: 0.75rem 1.5rem;
        font-size: 1rem;
        border-radius: 8px;
        cursor: pointer;
        font-weight: 600;
        transition: transform 0.2s, box-shadow 0.2s;
        margin-right: 0.5rem;
        margin-bottom: 0.5rem;
    }

    button:hover {
        transform: translateY(-2px);
        box-shadow: 0 4px 12px rgba(102, 126, 234, 0.4);
    }

    button:active {
        transform: translateY(0);
    }

    button.secondary {
        background: #6c757d;
    }

    .status {
        padding: 0.75rem;
        border-radius: 8px;
        margin: 1rem 0;
    }

    .status.success {
        background: #d4edda;
        color: #155724;
        border: 1px solid #c3e6cb;
    }

    .status.error {
        background: #f8d7da;
        color: #721c24;
        border: 1px solid #f5c6cb;
    }

    .info {
        background: #f8f9fa;
        border: 1px solid #e9ecef;
        border-radius: 8px;
        padding: 1rem;
        margin: 1rem 0;
    }

    .info p {
        margin-bottom: 0.5rem;
    }

    .info ul {
        margin-left: 1.5rem;
        margin-top: 0.5rem;
    }

    .info li {
        margin-bottom: 0.25rem;
        color: #666;
    }

    .footer {
        margin-top: 2rem;
        font-size: 0.85rem;
        color: #999;
        text-align: center;
    }

    .hidden {
        display: none;
    }

    pre {
        background: #f8f9fa;
        padding: 1rem;
        border-radius: 4px;
        overflow-x: auto;
        font-size: 0.85rem;
    }

    code {
        background: #f0f0f0;
        padding: 0.2rem 0.4rem;
        border-radius: 4px;
        font-family: 'Monaco', 'Courier New', monospace;
        font-size: 0.9em;
    }
  `;
}

function generateState() {
  return Math.random().toString(36).substring(2, 15) +
         Math.random().toString(36).substring(2, 15);
}

function escapeHtml(text) {
  const map = {
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#039;'
  };
  return text.replace(/[&<>"']/g, m => map[m]);
}

// Start server
app.listen(PORT, () => {
  console.log(`OAuth Token Endpoint Flow Example running on http://localhost:${PORT}`);
  console.log('Configuration:', {
    riseIssuer: CONFIG.riseIssuer,
    projectName: CONFIG.projectName,
    extensionName: CONFIG.extensionName,
    oidcIssuer: CONFIG.oidcIssuer,
  });
});
