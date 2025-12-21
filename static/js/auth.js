// OAuth2 authentication - uses server-side PKCE flow

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

// Initiate OAuth2 authorization code flow (server-side PKCE)
// The backend handles PKCE generation, state management, and token exchange
// to avoid sessionStorage issues on first login
async function login() {
    try {
        // Redirect to backend's OAuth start endpoint
        // The backend will:
        // 1. Generate PKCE params and store state server-side
        // 2. Redirect to OIDC provider
        // 3. Handle callback and token exchange
        // 4. Return an HTML page that stores token in localStorage
        window.location.href = `${CONFIG.backendUrl}/api/v1/auth/signin/start`;
    } catch (error) {
        console.error('Login error:', error);
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
