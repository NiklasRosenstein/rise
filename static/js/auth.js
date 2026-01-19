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
async function login() {
    try {
        // Redirect to backend's OAuth start endpoint
        // The backend will:
        // 1. Generate PKCE params and store state server-side
        // 2. Redirect to OIDC provider
        // 3. Handle callback and token exchange
        // 4. Set rise_jwt HttpOnly cookie
        window.location.href = `${CONFIG.backendUrl}/api/v1/auth/signin/start`;
    } catch (error) {
        console.error('Login error:', error);
        throw error;
    }
}

// Logout - calls backend to clear cookie
async function logout() {
    try {
        // Call backend logout endpoint to clear the rise_jwt cookie
        await fetch(`${CONFIG.backendUrl}/api/v1/auth/logout`, {
            method: 'GET',
            credentials: 'include'  // Include cookies
        });
    } catch (error) {
        console.error('Logout error:', error);
    }
    // Redirect regardless of success/failure
    window.location.href = '/';
}
