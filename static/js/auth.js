// OAuth2 PKCE implementation (mirrors CLI implementation)

// Configuration is injected by the backend via index.html.tera template
// Fallback to defaults for local development if not injected
if (!window.CONFIG) {
    window.CONFIG = {
        backendUrl: window.location.origin,
        issuerUrl: 'http://localhost:5556/dex',
        clientId: 'rise-backend',
        redirectUri: window.location.origin + '/',
    };
}

// Generate random string for PKCE
function generateRandomString(length) {
    const charset = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';
    const randomValues = new Uint8Array(length);
    crypto.getRandomValues(randomValues);
    return Array.from(randomValues)
        .map(x => charset[x % charset.length])
        .join('');
}

// Generate PKCE code_verifier and code_challenge
async function generatePKCE() {
    const codeVerifier = generateRandomString(43);

    // Calculate SHA-256 hash
    const encoder = new TextEncoder();
    const data = encoder.encode(codeVerifier);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);

    // Base64URL encode
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    const base64 = btoa(String.fromCharCode(...hashArray));
    const codeChallenge = base64
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=/g, '');

    return { codeVerifier, codeChallenge };
}

// Initiate OAuth2 authorization code flow
async function login() {
    try {
        const { codeVerifier, codeChallenge } = await generatePKCE();

        // Store code_verifier in sessionStorage
        sessionStorage.setItem('pkce_code_verifier', codeVerifier);

        // Build authorization URL
        const authUrl = new URL(CONFIG.authorizeUrl);
        authUrl.searchParams.append('client_id', CONFIG.clientId);
        authUrl.searchParams.append('redirect_uri', CONFIG.redirectUri);
        authUrl.searchParams.append('response_type', 'code');
        authUrl.searchParams.append('scope', 'openid email profile offline_access');
        authUrl.searchParams.append('code_challenge', codeChallenge);
        authUrl.searchParams.append('code_challenge_method', 'S256');

        // Redirect to OIDC provider
        window.location.href = authUrl.toString();
    } catch (error) {
        console.error('Login error:', error);
        throw error;
    }
}

// Handle OAuth callback
async function handleOAuthCallback() {
    const params = new URLSearchParams(window.location.search);
    const code = params.get('code');
    const error = params.get('error');

    if (error) {
        console.error('OAuth error:', params.get('error_description') || error);
        throw new Error('Authentication failed: ' + (params.get('error_description') || error));
    }

    if (!code) {
        throw new Error('No authorization code received');
    }

    const codeVerifier = sessionStorage.getItem('pkce_code_verifier');
    if (!codeVerifier) {
        throw new Error('No code verifier found');
    }

    try {
        // Exchange code for token
        const response = await fetch(`${CONFIG.backendUrl}/api/v1/auth/code/exchange`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                code,
                code_verifier: codeVerifier,
                redirect_uri: CONFIG.redirectUri,
            }),
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`Token exchange failed: ${errorText}`);
        }

        const data = await response.json();

        // Store token in localStorage
        localStorage.setItem('rise_token', data.token);
        sessionStorage.removeItem('pkce_code_verifier');

        // Clear OAuth params and reload to show dashboard
        window.history.replaceState({}, document.title, '/');
        window.location.reload();
    } catch (error) {
        console.error('Code exchange error:', error);
        throw error;
    }
}

// Logout
function logout() {
    localStorage.removeItem('rise_token');
    window.location.href = '/';
}

// Check if user is authenticated
function isAuthenticated() {
    return !!localStorage.getItem('rise_token');
}

// Get stored token
function getToken() {
    return localStorage.getItem('rise_token');
}
