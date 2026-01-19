const express = require('express');

const app = express();
const PORT = process.env.PORT || 8080;

// Utility: Decode JWT and extract claims (server-side)
function decodeJWT(token) {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) {
      throw new Error('Invalid JWT format (expected 3 parts)');
    }

    // Decode the payload (second part)
    const payload = parts[1];
    // URL-safe base64 decoding
    const decoded = Buffer.from(payload.replace(/-/g, '+').replace(/_/g, '/'), 'base64').toString('utf-8');
    return JSON.parse(decoded);
  } catch (e) {
    throw new Error(`Failed to decode JWT: ${e.message}`);
  }
}

// Utility: Extract rise_jwt cookie from request
function getRiseJwtCookie(req) {
  const cookies = req.headers.cookie;
  if (!cookies) return null;

  const cookieArray = cookies.split(';');
  for (let cookie of cookieArray) {
    const [name, value] = cookie.trim().split('=');
    if (name === 'rise_jwt') {
      return value;
    }
  }
  return null;
}

// Utility: Format timestamp
function formatTimestamp(unixTimestamp) {
  const date = new Date(unixTimestamp * 1000);
  const now = Date.now();
  const diff = date.getTime() - now;
  const diffStr = diff > 0 ?
    `in ${Math.floor(diff / 1000 / 60)} minutes` :
    `${Math.floor(-diff / 1000 / 60)} minutes ago`;

  return `${date.toLocaleString()} <span class="timestamp">(${diffStr})</span>`;
}

// Home route - decode and display JWT claims
app.get('/', (req, res) => {
  const token = getRiseJwtCookie(req);

  if (!token) {
    return res.send(renderNoTokenPage());
  }

  try {
    const claims = decodeJWT(token);
    res.send(renderClaimsPage(claims));
  } catch (error) {
    res.send(renderErrorPage(error.message));
  }
});

// Render pages
function renderNoTokenPage() {
  return `
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Rise JWT Viewer - Not Authenticated</title>
        <style>${getStyles()}</style>
    </head>
    <body>
        <div class="container">
            <h1>🔐 Rise JWT Claims Viewer</h1>
            <p class="subtitle">Decode and inspect the <code>rise_jwt</code> cookie set by Rise authentication</p>

            <div class="status error">
                ✗ No <code>rise_jwt</code> cookie found
            </div>

            <div class="info">
                <h2>Why can't I see my JWT?</h2>
                <p>The <code>rise_jwt</code> cookie is an <strong>HttpOnly cookie</strong>, which means:</p>
                <ul>
                    <li>JavaScript cannot access it (XSS protection)</li>
                    <li>It's automatically sent with requests to the same domain</li>
                    <li>Only server-side code can read it</li>
                </ul>

                <h3>To see your JWT claims:</h3>
                <ol>
                    <li>Make sure you're logged in to Rise (visit the Rise dashboard)</li>
                    <li>Refresh this page - the cookie will be sent automatically</li>
                    <li>This server-side app will decode and display the claims</li>
                </ol>
            </div>

            <div class="footer">
                <strong>Note:</strong> This is a server-side example. The JWT cookie is read from the HTTP request headers,
                decoded server-side, and rendered in the HTML response.
                <br><br>
                Learn more: <a href="https://github.com/NiklasRosenstein/rise" target="_blank">Rise Documentation</a>
            </div>
        </div>
    </body>
    </html>
  `;
}

function renderClaimsPage(claims) {
  const groupsHtml = claims.groups && claims.groups.length > 0 ?
    claims.groups.map(g => `<span class="badge">${escapeHtml(g)}</span>`).join('') :
    '<span class="text-muted">No teams</span>';

  return `
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Rise JWT Viewer - Claims</title>
        <style>${getStyles()}</style>
    </head>
    <body>
        <div class="container">
            <h1>🔐 Rise JWT Claims Viewer</h1>
            <p class="subtitle">Decode and inspect the <code>rise_jwt</code> cookie set by Rise authentication</p>

            <div class="status success">
                ✓ Successfully decoded <code>rise_jwt</code> cookie
            </div>

            <div class="claims-section">
                <h2>📋 JWT Claims</h2>
                <div class="claim-row">
                    <div class="claim-key">Subject</div>
                    <div class="claim-value">${escapeHtml(claims.sub || 'N/A')}</div>
                </div>
                <div class="claim-row">
                    <div class="claim-key">Email</div>
                    <div class="claim-value">${escapeHtml(claims.email || 'N/A')}</div>
                </div>
                ${claims.name ? `
                <div class="claim-row">
                    <div class="claim-key">Name</div>
                    <div class="claim-value">${escapeHtml(claims.name)}</div>
                </div>
                ` : ''}
                <div class="claim-row">
                    <div class="claim-key">Issuer</div>
                    <div class="claim-value">${escapeHtml(claims.iss || 'N/A')}</div>
                </div>
                <div class="claim-row">
                    <div class="claim-key">Audience</div>
                    <div class="claim-value">${escapeHtml(claims.aud || 'N/A')}</div>
                </div>
                <div class="claim-row">
                    <div class="claim-key">Issued At</div>
                    <div class="claim-value">${claims.iat ? formatTimestamp(claims.iat) : 'N/A'}</div>
                </div>
                <div class="claim-row">
                    <div class="claim-key">Expires At</div>
                    <div class="claim-value">${claims.exp ? formatTimestamp(claims.exp) : 'N/A'}</div>
                </div>
                ${claims.groups && claims.groups.length >= 0 ? `
                <div class="claim-row">
                    <div class="claim-key">Teams</div>
                    <div class="claim-value array">${groupsHtml}</div>
                </div>
                ` : ''}
            </div>

            <div class="claims-section">
                <h2>🔍 Raw JWT Payload</h2>
                <pre>${escapeHtml(JSON.stringify(claims, null, 2))}</pre>
            </div>

            <div class="footer">
                <strong>About this example:</strong><br>
                This server-side application reads the HttpOnly <code>rise_jwt</code> cookie from the request headers,
                decodes it, and displays the claims. The cookie is automatically sent by the browser when you make requests
                to the same domain where it was set.
                <br><br>
                <strong>Security:</strong> The <code>rise_jwt</code> cookie is HttpOnly, which prevents JavaScript access
                and provides protection against XSS attacks. Only server-side code can read this cookie.
                <br><br>
                Learn more: <a href="https://github.com/NiklasRosenstein/rise" target="_blank">Rise Documentation</a>
            </div>
        </div>
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
        <title>Rise JWT Viewer - Error</title>
        <style>${getStyles()}</style>
    </head>
    <body>
        <div class="container">
            <h1>🔐 Rise JWT Claims Viewer</h1>

            <div class="status error">
                ✗ Error decoding JWT: ${escapeHtml(message)}
            </div>

            <div class="footer">
                If you're seeing this error, the JWT cookie may be corrupted or invalid.
                Try logging out and logging back in to Rise.
            </div>
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
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        min-height: 100vh;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 20px;
    }

    .container {
        background: white;
        border-radius: 12px;
        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
        max-width: 800px;
        width: 100%;
        padding: 40px;
    }

    h1 {
        color: #333;
        margin-bottom: 10px;
        font-size: 28px;
    }

    .subtitle {
        color: #666;
        margin-bottom: 30px;
        font-size: 14px;
    }

    .status {
        padding: 15px;
        border-radius: 8px;
        margin-bottom: 20px;
        font-weight: 500;
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
        background: #e7f3ff;
        padding: 20px;
        border-radius: 8px;
        border-left: 4px solid #667eea;
        margin: 20px 0;
    }

    .info h2, .info h3 {
        color: #333;
        margin: 15px 0 10px 0;
        font-size: 16px;
    }

    .info h2:first-child {
        margin-top: 0;
    }

    .info p, .info ul, .info ol {
        color: #555;
        line-height: 1.6;
        margin: 10px 0;
    }

    .info ul, .info ol {
        padding-left: 25px;
    }

    .info li {
        margin: 5px 0;
    }

    .claims-section {
        background: #f8f9fa;
        padding: 20px;
        border-radius: 8px;
        margin-top: 20px;
    }

    .claims-section h2 {
        font-size: 18px;
        color: #333;
        margin-bottom: 15px;
        display: flex;
        align-items: center;
        gap: 10px;
    }

    .claim-row {
        display: grid;
        grid-template-columns: 150px 1fr;
        gap: 15px;
        padding: 10px 0;
        border-bottom: 1px solid #dee2e6;
    }

    .claim-row:last-child {
        border-bottom: none;
    }

    .claim-key {
        font-weight: 600;
        color: #495057;
        font-size: 13px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
    }

    .claim-value {
        color: #333;
        font-family: 'Monaco', 'Courier New', monospace;
        font-size: 14px;
        word-break: break-all;
    }

    .claim-value.array {
        display: flex;
        flex-wrap: wrap;
        gap: 5px;
    }

    .badge {
        display: inline-block;
        padding: 4px 8px;
        background: #667eea;
        color: white;
        border-radius: 4px;
        font-size: 12px;
        font-weight: 500;
    }

    .text-muted {
        color: #6c757d;
        font-style: italic;
    }

    .timestamp {
        color: #6c757d;
        font-size: 12px;
    }

    pre {
        background: #2d2d2d;
        color: #f8f8f2;
        padding: 15px;
        border-radius: 6px;
        overflow-x: auto;
        font-size: 13px;
        line-height: 1.5;
    }

    .footer {
        margin-top: 30px;
        padding-top: 20px;
        border-top: 1px solid #dee2e6;
        color: #6c757d;
        font-size: 13px;
        line-height: 1.6;
    }

    .footer a {
        color: #667eea;
        text-decoration: none;
    }

    .footer a:hover {
        text-decoration: underline;
    }

    code {
        background: #f8f9fa;
        padding: 2px 6px;
        border-radius: 3px;
        font-family: 'Monaco', 'Courier New', monospace;
        font-size: 13px;
        color: #e83e8c;
    }
  `;
}

function escapeHtml(unsafe) {
  if (!unsafe) return '';
  return unsafe
    .toString()
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

app.listen(PORT, () => {
  console.log(`Rise JWT Viewer listening on port ${PORT}`);
});
