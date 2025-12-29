// Extension UI Components Registry
// This file contains custom UI components for extensions that provide
// a form-based interface instead of just raw JSON editing.

const { useState, useEffect } = React;

// AWS RDS Extension UI Component
function AwsRdsExtensionUI({ spec, schema, onChange }) {
    const [engine, setEngine] = useState(spec?.engine || 'postgres');
    const [engineVersion, setEngineVersion] = useState(spec?.engine_version || '');
    const [databaseIsolation, setDatabaseIsolation] = useState(spec?.database_isolation || 'shared');
    const [databaseUrlEnvVar, setDatabaseUrlEnvVar] = useState(spec?.database_url_env_var || 'DATABASE_URL');
    const [injectPgVars, setInjectPgVars] = useState(spec?.inject_pg_vars !== false);

    // Extract default engine version from schema
    const defaultEngineVersion = schema?.properties?.engine_version?.default || '';

    // Use a ref to store the latest onChange callback
    const onChangeRef = React.useRef(onChange);
    React.useEffect(() => {
        onChangeRef.current = onChange;
    }, [onChange]);

    // Update parent when values change
    useEffect(() => {
        // Build the spec object, omitting empty values
        const newSpec = {
            engine,
            database_isolation: databaseIsolation,
            inject_pg_vars: injectPgVars,
        };

        // Only include engine_version if it's not empty
        if (engineVersion) {
            newSpec.engine_version = engineVersion;
        }

        // Only include database_url_env_var if it's not empty
        if (databaseUrlEnvVar && databaseUrlEnvVar.trim() !== '') {
            newSpec.database_url_env_var = databaseUrlEnvVar;
        }

        onChangeRef.current(newSpec);
    }, [engine, engineVersion, databaseIsolation, databaseUrlEnvVar, injectPgVars]);

    return (
        <div className="space-y-4">
            <FormField
                label="Database Engine"
                id="rds-engine"
                type="select"
                value={engine}
                onChange={(e) => setEngine(e.target.value)}
                required
            >
                <option value="postgres">PostgreSQL</option>
            </FormField>

            <FormField
                label="Engine Version (Optional)"
                id="rds-engine-version"
                value={engineVersion}
                onChange={(e) => setEngineVersion(e.target.value)}
                placeholder={defaultEngineVersion || "e.g., 16.2"}
            />

            <FormField
                label="Database Isolation"
                id="rds-database-isolation"
                type="select"
                value={databaseIsolation}
                onChange={(e) => setDatabaseIsolation(e.target.value)}
                required
            >
                <option value="shared">Shared (All deployment groups use same database)</option>
                <option value="isolated">Isolated (Each deployment group gets own database)</option>
            </FormField>

            <div className="space-y-3">
                <h4 className="text-sm font-semibold text-gray-700 dark:text-gray-300">Environment Variables</h4>
                <FormField
                    label="Database URL Environment Variable"
                    id="rds-database-url-env-var"
                    value={databaseUrlEnvVar}
                    onChange={(e) => setDatabaseUrlEnvVar(e.target.value)}
                    placeholder="DATABASE_URL"
                    helperText="Environment variable name for the database connection string (e.g., DATABASE_URL, POSTGRES_URL). Leave empty to disable. This allows multiple RDS instances to use different variable names."
                />
                <label className="flex items-center space-x-3">
                    <input
                        type="checkbox"
                        checked={injectPgVars}
                        onChange={(e) => setInjectPgVars(e.target.checked)}
                        className="w-4 h-4 text-indigo-600 bg-gray-700 border-gray-600 rounded focus:ring-indigo-500 focus:ring-2"
                    />
                    <span className="text-sm text-gray-700 dark:text-gray-300">
                        Inject <code className="bg-gray-700 px-1 rounded">PG*</code> variables
                        <span className="text-gray-600 dark:text-gray-500 ml-2">(PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD)</span>
                    </span>
                </label>
                <p className="text-xs text-gray-600 dark:text-gray-500">
                    Note: Only one RDS extension should have PG* variables enabled per project, as they will override each other.
                </p>
            </div>

            <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">About This Extension</h4>
                <p className="text-sm text-gray-600 dark:text-gray-400">
                    This extension provisions a PostgreSQL database on AWS RDS. The instance size, disk size,
                    and other infrastructure settings are configured at the server level.
                </p>
                <p className="text-sm text-gray-600 dark:text-gray-400 mt-2">
                    <strong>Shared mode:</strong> All deployment groups (default, staging, etc.) use the same database.
                    This means staging deployments use the same database as production.
                </p>
                <p className="text-sm text-gray-600 dark:text-gray-400 mt-2">
                    <strong>Isolated mode:</strong> Each deployment group gets its own empty database.
                    This is useful for staging environments to have their own clean database instead of working with the production database.
                </p>
            </div>

            <div className="bg-yellow-900/20 border border-yellow-700 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-yellow-300 mb-2">⏱️ Initial Provisioning</h4>
                <p className="text-sm text-yellow-200">
                    Creating a new RDS instance typically takes <strong>5-15 minutes</strong>.
                    No new deployments can be created until the RDS instance is available.
                    You can monitor the provisioning status in the Extensions tab.
                </p>
            </div>
        </div>
    );
}

// Extension UI API
// Each extension can provide custom implementations for:
// - renderStatusBadge(extension): Custom status badge component
// - renderOverviewTab(extension): Custom overview/detail view component
// - renderConfigureTab(spec, schema, onChange): Custom configuration form component
// - icon: Icon URL for the extension

const AwsRdsExtensionAPI = {
    icon: '/assets/aws_rds_aurora.jpg',

    renderStatusBadge(extension) {
        const status = extension.status || {};
        if (!status.state) return null;

        let badgeColor;
        const state = status.state.toLowerCase();

        switch (state) {
            case 'available':
                badgeColor = 'bg-green-600';
                break;
            case 'creating':
            case 'pending':
                badgeColor = 'bg-yellow-600';
                break;
            case 'failed':
                badgeColor = 'bg-red-600';
                break;
            case 'deleting':
            case 'deleted':
                badgeColor = 'bg-gray-600';
                break;
            default:
                badgeColor = 'bg-gray-600';
        }

        return (
            <span className={`${badgeColor} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase`}>
                {status.state}
            </span>
        );
    },

    renderOverviewTab(extension, projectName) {
        return <AwsRdsDetailView extension={extension} projectName={projectName} />;
    },

    renderConfigureTab(spec, schema, onChange, projectName, instanceName, isEnabled) {
        return <AwsRdsExtensionUI spec={spec} schema={schema} onChange={onChange} />;
    }
};

// OAuth Extension UI Component
function OAuthExtensionUI({ spec, schema, onChange, projectName, instanceName, isEnabled }) {
    const [providerName, setProviderName] = useState(spec?.provider_name || '');
    const [description, setDescription] = useState(spec?.description || '');
    const [clientId, setClientId] = useState(spec?.client_id || '');
    const [clientSecretRef, setClientSecretRef] = useState(spec?.client_secret_ref || '');
    const [authorizationEndpoint, setAuthorizationEndpoint] = useState(spec?.authorization_endpoint || '');
    const [tokenEndpoint, setTokenEndpoint] = useState(spec?.token_endpoint || '');
    const [scopes, setScopes] = useState(spec?.scopes?.join(', ') || '');
    const { showToast } = useToast();

    // Build the redirect URI for display
    const backendUrl = CONFIG.backendUrl.replace(/\/$/, ''); // Remove trailing slash
    const displayProjectName = projectName || 'YOUR_PROJECT';
    const displayExtensionName = isEnabled ? instanceName : (instanceName || 'YOUR_EXTENSION_NAME');
    const redirectUri = `${backendUrl}/api/v1/oauth/callback/${displayProjectName}/${displayExtensionName}`;

    // Use a ref to store the latest onChange callback
    const onChangeRef = React.useRef(onChange);
    React.useEffect(() => {
        onChangeRef.current = onChange;
    }, [onChange]);

    // Update parent when values change
    useEffect(() => {
        // Parse scopes from comma-separated string
        const scopesArray = scopes
            .split(',')
            .map(s => s.trim())
            .filter(s => s.length > 0);

        // Build the spec object
        const newSpec = {
            provider_name: providerName,
            client_id: clientId,
            client_secret_ref: clientSecretRef,
            authorization_endpoint: authorizationEndpoint,
            token_endpoint: tokenEndpoint,
            scopes: scopesArray,
        };

        // Only include description if it's not empty
        if (description && description.trim() !== '') {
            newSpec.description = description;
        }

        onChangeRef.current(newSpec);
    }, [providerName, description, clientId, clientSecretRef, authorizationEndpoint, tokenEndpoint, scopes]);

    // Common provider templates
    const providerTemplates = {
        snowflake: {
            name: 'Snowflake',
            authEndpoint: 'https://YOUR_ACCOUNT.snowflakecomputing.com/oauth/authorize',
            tokenEndpoint: 'https://YOUR_ACCOUNT.snowflakecomputing.com/oauth/token-request',
            scopes: 'refresh_token'
        },
        google: {
            name: 'Google',
            authEndpoint: 'https://accounts.google.com/o/oauth2/v2/auth',
            tokenEndpoint: 'https://oauth2.googleapis.com/token',
            scopes: 'openid, email, profile'
        },
        github: {
            name: 'GitHub',
            authEndpoint: 'https://github.com/login/oauth/authorize',
            tokenEndpoint: 'https://github.com/login/oauth/access_token',
            scopes: 'read:user, user:email'
        }
    };

    const applyTemplate = (templateKey) => {
        const template = providerTemplates[templateKey];
        if (template) {
            setProviderName(template.name);
            setAuthorizationEndpoint(template.authEndpoint);
            setTokenEndpoint(template.tokenEndpoint);
            setScopes(template.scopes);
            showToast(`Applied ${template.name} template`, 'success');
        }
    };

    return (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Left column - Main configuration form */}
            <div className="lg:col-span-2 space-y-4">
                <FormField
                    label="Provider Name"
                    id="oauth-provider-name"
                    value={providerName}
                    onChange={(e) => setProviderName(e.target.value)}
                    placeholder="e.g., Snowflake Production"
                    required
                    helperText="Display name for this OAuth provider"
                />

                <FormField
                    label="Description (Optional)"
                    id="oauth-description"
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    placeholder="e.g., OAuth authentication for analytics access"
                    helperText="Human-readable description of this OAuth configuration"
                />

                <FormField
                    label="Client ID"
                    id="oauth-client-id"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    placeholder="e.g., ABC123XYZ..."
                    required
                    helperText="OAuth client identifier from your provider"
                />

                <FormField
                    label="Client Secret Environment Variable"
                    id="oauth-client-secret-ref"
                    value={clientSecretRef}
                    onChange={(e) => setClientSecretRef(e.target.value)}
                    placeholder="e.g., OAUTH_SNOWFLAKE_SECRET"
                    required
                    helperText="Name of the environment variable containing the client secret (must be set as secret env var)"
                />

                <FormField
                    label="Authorization Endpoint"
                    id="oauth-authorization-endpoint"
                    value={authorizationEndpoint}
                    onChange={(e) => setAuthorizationEndpoint(e.target.value)}
                    placeholder="https://provider.com/oauth/authorize"
                    required
                    helperText="OAuth provider's authorization URL"
                />

                <FormField
                    label="Token Endpoint"
                    id="oauth-token-endpoint"
                    value={tokenEndpoint}
                    onChange={(e) => setTokenEndpoint(e.target.value)}
                    placeholder="https://provider.com/oauth/token"
                    required
                    helperText="OAuth provider's token URL"
                />

                <FormField
                    label="Scopes"
                    id="oauth-scopes"
                    value={scopes}
                    onChange={(e) => setScopes(e.target.value)}
                    placeholder="openid, email, profile"
                    required
                    helperText="Comma-separated list of OAuth scopes to request"
                />

                <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4">
                    <h4 className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">About This Extension</h4>
                    <p className="text-sm text-gray-600 dark:text-gray-400">
                        The Generic OAuth 2.0 extension allows your application to authenticate end users via any OAuth 2.0 provider
                        (Snowflake, Google, GitHub, custom SSO, etc.) without managing client secrets locally.
                    </p>
                    <p className="text-sm text-gray-600 dark:text-gray-400 mt-2">
                        <strong>Security:</strong> Client secrets are stored encrypted and never exposed to your application.
                        Tokens are delivered in URL fragments for frontend apps or via secure exchange for backend apps.
                    </p>
                </div>
            </div>

            {/* Right column - Redirect URI and Quick Start */}
            <div className="lg:col-span-1 space-y-6">
                {/* Redirect URI Configuration - Must be set in provider FIRST */}
                <section>
                    <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Redirect URI</h2>
                    <div className="bg-blue-900/20 border-2 border-blue-600 rounded-lg p-4">
                        <h3 className="text-sm font-semibold text-blue-300 mb-2">🔗 Required Setup</h3>
                        <p className="text-sm text-blue-200 mb-3">
                            Configure this callback URL in your OAuth provider first:
                        </p>
                        <div className="flex items-center gap-2 mb-2">
                            <code className="flex-1 bg-gray-100 dark:bg-gray-800 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 break-all font-mono">
                                {redirectUri}
                            </code>
                            <button
                                type="button"
                                onClick={() => {
                                    navigator.clipboard.writeText(redirectUri);
                                    showToast('Redirect URI copied to clipboard', 'success');
                                }}
                                className="px-3 py-2 bg-blue-600 hover:bg-blue-700 text-white text-xs rounded transition-colors whitespace-nowrap font-semibold"
                                title="Copy redirect URI"
                            >
                                Copy
                            </button>
                        </div>
                        <p className="text-xs text-blue-300">
                            Also called "Callback URL" or "Authorized redirect URIs".
                            {!isEnabled && instanceName && (
                                <span className="block mt-2 text-blue-200">
                                    <strong>Note:</strong> Uses extension name "{instanceName}"
                                </span>
                            )}
                        </p>
                    </div>
                </section>

                {/* Quick Start Templates */}
                <section>
                    <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Quick Start</h2>
                    <div className="space-y-2">
                        <button
                            type="button"
                            onClick={() => applyTemplate('snowflake')}
                            className="w-full px-3 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-900 dark:text-gray-200 font-semibold transition-colors text-left"
                            title="Apply Snowflake template"
                        >
                            📋 Snowflake Template
                        </button>
                        <button
                            type="button"
                            onClick={() => applyTemplate('google')}
                            className="w-full px-3 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-900 dark:text-gray-200 font-semibold transition-colors text-left"
                            title="Apply Google template"
                        >
                            📋 Google Template
                        </button>
                        <button
                            type="button"
                            onClick={() => applyTemplate('github')}
                            className="w-full px-3 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-900 dark:text-gray-200 font-semibold transition-colors text-left"
                            title="Apply GitHub template"
                        >
                            📋 GitHub Template
                        </button>
                    </div>
                    <p className="text-xs text-gray-600 dark:text-gray-500 mt-2">
                        Click to auto-fill endpoints for common providers
                    </p>
                </section>

                {/* Setup Steps */}
                <section>
                    <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Setup Steps</h2>
                    <div className="bg-yellow-900/20 border border-yellow-700 rounded-lg p-4">
                        <ol className="text-sm text-yellow-200 list-decimal list-inside space-y-2">
                            <li>Configure redirect URI in OAuth provider</li>
                            <li>Get client ID and secret from provider</li>
                            <li>Store secret as encrypted env var:
                                <code className="block bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded mt-1 text-xs">
                                    rise env set {displayProjectName} {clientSecretRef || 'OAUTH_SECRET'} "..." --secret
                                </code>
                            </li>
                            <li>Fill out the form and enable</li>
                        </ol>
                    </div>
                </section>
            </div>
        </div>
    );
}

const OAuthExtensionAPI = {
    icon: '/assets/oauth2.jpg',

    renderStatusBadge(extension) {
        const status = extension.status || {};

        if (status.error) {
            return (
                <span className="bg-red-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">
                    Error
                </span>
            );
        }

        if (status.configured_at) {
            if (status.auth_verified) {
                return (
                    <span className="bg-green-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">
                        Configured
                    </span>
                );
            } else {
                return (
                    <span className="bg-yellow-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">
                        Waiting For Auth
                    </span>
                );
            }
        }

        return (
            <span className="bg-gray-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">
                Not Configured
            </span>
        );
    },

    renderOverviewTab(extension, projectName) {
        return <OAuthDetailView extension={extension} projectName={projectName} />;
    },

    renderConfigureTab(spec, schema, onChange, projectName, instanceName, isEnabled) {
        return <OAuthExtensionUI spec={spec} schema={schema} onChange={onChange} projectName={projectName} instanceName={instanceName} isEnabled={isEnabled} />;
    }
};

// Integration Guide Modal Component
function IntegrationGuideModal({ isOpen, onClose, projectName, extensionName }) {
    const [activeTab, setActiveTab] = useState('fragment');

    if (!isOpen) return null;

    const backendUrl = CONFIG.backendUrl.replace(/\/$/, '');
    const authorizeUrl = `${backendUrl}/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize`;
    const callbackUrl = `${backendUrl}/api/v1/oauth/callback/${projectName}/${extensionName}`;
    const exchangeUrl = `${backendUrl}/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/exchange`;

    return (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center p-4 z-50" onClick={onClose}>
            <div className="bg-gray-100 dark:bg-gray-800 rounded-lg max-w-4xl w-full max-h-[90vh] overflow-hidden" onClick={(e) => e.stopPropagation()}>
                {/* Modal Header */}
                <div className="flex items-center justify-between p-6 border-b border-gray-300 dark:border-gray-700">
                    <h2 className="text-xl font-bold text-white">Integration Guide</h2>
                    <button
                        onClick={onClose}
                        className="text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
                        title="Close"
                    >
                        <div className="w-6 h-6 svg-mask" style={{
                            maskImage: 'url(/assets/close-x.svg)',
                            WebkitMaskImage: 'url(/assets/close-x.svg)'
                        }}></div>
                    </button>
                </div>

                {/* Tabs */}
                <div className="flex border-b border-gray-300 dark:border-gray-700 bg-white dark:bg-gray-900 px-6">
                    <button
                        onClick={() => setActiveTab('fragment')}
                        className={`px-4 py-3 text-sm font-medium transition-colors border-b-2 ${
                            activeTab === 'fragment'
                                ? 'border-indigo-500 text-indigo-400'
                                : 'border-transparent text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'
                        }`}
                    >
                        Fragment Flow (SPAs)
                    </button>
                    <button
                        onClick={() => setActiveTab('exchange')}
                        className={`px-4 py-3 text-sm font-medium transition-colors border-b-2 ${
                            activeTab === 'exchange'
                                ? 'border-indigo-500 text-indigo-400'
                                : 'border-transparent text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'
                        }`}
                    >
                        Exchange Flow (Backend)
                    </button>
                    <button
                        onClick={() => setActiveTab('local')}
                        className={`px-4 py-3 text-sm font-medium transition-colors border-b-2 ${
                            activeTab === 'local'
                                ? 'border-indigo-500 text-indigo-400'
                                : 'border-transparent text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'
                        }`}
                    >
                        Local Development
                    </button>
                </div>

                {/* Modal Content */}
                <div className="p-6 overflow-y-auto max-h-[calc(90vh-140px)]">
                    {activeTab === 'fragment' && (
                        <div className="space-y-4">
                            <p className="text-sm text-gray-700 dark:text-gray-300">
                                <strong>Fragment Flow</strong> is the default and recommended approach for single-page applications (SPAs).
                                Tokens are returned in the URL fragment (<code className="bg-white dark:bg-gray-900 px-1 rounded">#access_token=...</code>),
                                which never reaches the server.
                            </p>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Authorization URL:</p>
                                <code className="block bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 break-all">
                                    {authorizeUrl}
                                </code>
                            </div>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Example Code:</p>
                                <pre className="bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 overflow-x-auto">
{`// Initiate OAuth login (fragment flow is default)
function login() {
  const authUrl = '${authorizeUrl}';
  window.location.href = authUrl;
}

// Extract tokens from URL fragment after redirect
function handleCallback() {
  const fragment = window.location.hash.substring(1);
  const params = new URLSearchParams(fragment);

  const accessToken = params.get('access_token');
  const idToken = params.get('id_token');
  const expiresAt = params.get('expires_at');

  if (accessToken) {
    // Store securely (session storage for security)
    sessionStorage.setItem('access_token', accessToken);
    if (idToken) {
      sessionStorage.setItem('id_token', idToken);
    }

    // Clear the fragment from URL
    window.location.hash = '';

    // Redirect to your app
    window.location.href = '/dashboard';
  }
}

// Call on page load
handleCallback();`}
                                </pre>
                            </div>
                        </div>
                    )}

                    {activeTab === 'exchange' && (
                        <div className="space-y-4">
                            <p className="text-sm text-gray-700 dark:text-gray-300">
                                <strong>Exchange Flow</strong> is designed for server-rendered applications. Your backend receives a
                                temporary exchange token as a query parameter, which it exchanges for the actual OAuth tokens via a server-side API call.
                            </p>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Authorization URL:</p>
                                <code className="block bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 break-all">
                                    {authorizeUrl}?flow=exchange
                                </code>
                            </div>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Exchange Endpoint:</p>
                                <code className="block bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 break-all">
                                    {exchangeUrl}?exchange_token=...
                                </code>
                            </div>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Example Code (Node.js/Express):</p>
                                <pre className="bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 overflow-x-auto">
{`// Initiate OAuth login with exchange flow
app.get('/login', (req, res) => {
  const authUrl = '${authorizeUrl}?flow=exchange';
  res.redirect(authUrl);
});

// Handle OAuth callback
app.get('/oauth/callback', async (req, res) => {
  const exchangeToken = req.query.exchange_token;

  if (!exchangeToken) {
    return res.status(400).send('Missing exchange token');
  }

  try {
    // Exchange the temporary token for actual OAuth tokens
    const response = await fetch(
      \`${exchangeUrl}?exchange_token=\${exchangeToken}\`
    );

    if (!response.ok) {
      throw new Error('Token exchange failed');
    }

    const tokens = await response.json();
    // { access_token, id_token, expires_at, ... }

    // Store in HttpOnly session cookie (recommended)
    req.session.accessToken = tokens.access_token;
    req.session.idToken = tokens.id_token;
    req.session.expiresAt = tokens.expires_at;

    // Redirect to app
    res.redirect('/dashboard');
  } catch (error) {
    console.error('OAuth exchange error:', error);
    res.status(500).send('Authentication failed');
  }
});`}
                                </pre>
                            </div>
                        </div>
                    )}

                    {activeTab === 'local' && (
                        <div className="space-y-4">
                            <p className="text-sm text-gray-700 dark:text-gray-300">
                                For local development, you can override the redirect URI to point to your local development server.
                                Rise always handles the OAuth provider callback, so you don't need to register localhost URLs with your OAuth provider.
                            </p>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Fragment Flow (localhost):</p>
                                <pre className="bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 overflow-x-auto">
{`// Override redirect URI for local development
const authUrl = '${authorizeUrl}?redirect_uri=' +
  encodeURIComponent('http://localhost:3000/callback');

window.location.href = authUrl;

// Handle the callback in your local app
function handleCallback() {
  const fragment = window.location.hash.substring(1);
  const params = new URLSearchParams(fragment);
  const accessToken = params.get('access_token');
  // ... use the token
}`}
                                </pre>
                            </div>

                            <div>
                                <p className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">Exchange Flow (localhost):</p>
                                <pre className="bg-white dark:bg-gray-900 px-3 py-2 rounded text-xs text-gray-900 dark:text-gray-200 overflow-x-auto">
{`// Override redirect URI for local development
app.get('/login', (req, res) => {
  const authUrl = '${authorizeUrl}?flow=exchange&redirect_uri=' +
    encodeURIComponent('http://localhost:3000/oauth/callback');
  res.redirect(authUrl);
});

// Your local callback handler
app.get('/oauth/callback', async (req, res) => {
  const exchangeToken = req.query.exchange_token;
  // ... same exchange logic as production
});`}
                                </pre>
                            </div>

                            <div className="bg-blue-900/20 border border-blue-700 rounded p-4">
                                <p className="text-sm font-semibold text-blue-300 mb-2">ℹ️ How it Works</p>
                                <p className="text-xs text-blue-200">
                                    The OAuth provider only redirects to <code className="bg-gray-100 dark:bg-gray-800 px-1 rounded">{callbackUrl}</code> (Rise's callback URL).
                                    Rise then redirects to your app's redirect_uri with the tokens. You don't need to configure localhost URLs in your OAuth provider.
                                </p>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

// OAuth Detail View Component
function OAuthDetailView({ extension, projectName }) {
    const status = extension.status || {};
    const spec = extension.spec || {};
    const scopesArray = spec.scopes || [];
    const extensionName = extension.extension;
    const { showToast } = useToast();
    const [showGuideModal, setShowGuideModal] = useState(false);

    // Build URLs using actual backend URL
    const backendUrl = CONFIG.backendUrl.replace(/\/$/, ''); // Remove trailing slash
    const callbackUrl = `${backendUrl}/api/v1/oauth/callback/${projectName}/${extensionName}`;

    const handleTestOAuth = () => {
        // Store the current hash location to return to after OAuth
        const currentHash = window.location.hash.substring(1);
        sessionStorage.setItem('oauth_return_path', currentHash);

        // Use the origin as redirect URI (hash router doesn't work with fragments)
        const redirectUri = window.location.origin + '/';
        const authUrl = `/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize?redirect_uri=${encodeURIComponent(redirectUri)}`;
        window.location.href = authUrl;
    };

    return (
        <>
            <IntegrationGuideModal
                isOpen={showGuideModal}
                onClose={() => setShowGuideModal(false)}
                projectName={projectName}
                extensionName={extensionName}
            />

            {/* Two-column layout */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Left column - Main content */}
                <div className="lg:col-span-2 space-y-6">
                    {/* Provider Configuration */}
                    <section>
                        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">OAuth Provider</h2>
                        <div className="bg-white dark:bg-gray-900 rounded p-4 space-y-3">
                            <div>
                                <p className="text-sm text-gray-600 dark:text-gray-500">Provider Name</p>
                                <p className="text-gray-700 dark:text-gray-300 font-semibold">{spec.provider_name || 'N/A'}</p>
                            </div>
                            {spec.description && (
                                <div>
                                    <p className="text-sm text-gray-600 dark:text-gray-500">Description</p>
                                    <p className="text-gray-700 dark:text-gray-300">{spec.description}</p>
                                </div>
                            )}
                            <div>
                                <p className="text-sm text-gray-600 dark:text-gray-500">Client ID</p>
                                <p className="text-gray-700 dark:text-gray-300 font-mono text-sm">{spec.client_id || 'N/A'}</p>
                            </div>
                            <div>
                                <p className="text-sm text-gray-600 dark:text-gray-500">Client Secret Reference</p>
                                <p className="text-gray-700 dark:text-gray-300 font-mono text-sm">{spec.client_secret_ref || 'N/A'}</p>
                            </div>
                        </div>
                    </section>

                    {/* Endpoints */}
                    <section>
                        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">OAuth Endpoints</h2>
                        <div className="bg-white dark:bg-gray-900 rounded p-4 space-y-3">
                            <div>
                                <p className="text-sm text-gray-600 dark:text-gray-500">Authorization Endpoint</p>
                                <p className="text-gray-700 dark:text-gray-300 font-mono text-xs break-all">{spec.authorization_endpoint || 'N/A'}</p>
                            </div>
                            <div>
                                <p className="text-sm text-gray-600 dark:text-gray-500">Token Endpoint</p>
                                <p className="text-gray-700 dark:text-gray-300 font-mono text-xs break-all">{spec.token_endpoint || 'N/A'}</p>
                            </div>
                        </div>
                    </section>

                    {/* Scopes */}
                    <section>
                        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">
                            OAuth Scopes ({scopesArray.length})
                        </h2>
                        <div className="bg-white dark:bg-gray-900 rounded p-4">
                            {scopesArray.length === 0 ? (
                                <p className="text-gray-600 dark:text-gray-400 text-sm">No scopes configured</p>
                            ) : (
                                <div className="flex flex-wrap gap-2">
                                    {scopesArray.map((scope, idx) => (
                                        <span
                                            key={idx}
                                            className="bg-indigo-600 text-white text-xs font-semibold px-3 py-1 rounded-full"
                                        >
                                            {scope}
                                        </span>
                                    ))}
                                </div>
                            )}
                        </div>
                    </section>
                </div>

                {/* Right column - Actions */}
                <div className="lg:col-span-1 space-y-6">
                    {/* Configuration Status */}
                    <section>
                        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Status</h2>
                        <div className="bg-white dark:bg-gray-900 rounded p-4">
                            {status.error ? (
                                <div className="p-3 bg-red-900/20 border border-red-700 rounded">
                                    <p className="text-sm text-red-300">
                                        <strong>Error:</strong> {status.error}
                                    </p>
                                </div>
                            ) : status.configured_at ? (
                                status.auth_verified ? (
                                    <div className="p-3 bg-green-900/20 border border-green-700 rounded">
                                        <p className="text-sm text-green-300">
                                            ✓ Configured
                                        </p>
                                        <p className="text-xs text-green-400 mt-1">
                                            {formatDate(status.configured_at)}
                                        </p>
                                    </div>
                                ) : (
                                    <div className="p-3 bg-yellow-900/20 border border-yellow-700 rounded">
                                        <p className="text-sm text-yellow-300">
                                            ⚠ Waiting For Auth
                                        </p>
                                        <p className="text-xs text-yellow-400 mt-1">
                                            Complete OAuth flow to verify configuration
                                        </p>
                                    </div>
                                )
                            ) : (
                                <div className="p-3 bg-gray-100 dark:bg-gray-800 rounded">
                                    <p className="text-sm text-gray-600 dark:text-gray-400">
                                        Configuration pending...
                                    </p>
                                </div>
                            )}
                        </div>
                    </section>

                    {/* Test OAuth Flow */}
                    <section>
                        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Test</h2>
                        <button
                            onClick={handleTestOAuth}
                            className="w-full px-4 py-3 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg font-semibold transition-colors"
                        >
                            🔐 Test OAuth Flow
                        </button>
                        <p className="text-xs text-gray-600 dark:text-gray-500 mt-2">
                            Test the OAuth flow and return to this page with a notification.
                        </p>
                    </section>

                    {/* Integration Guide Button */}
                    <section>
                        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Integration</h2>
                        <button
                            onClick={() => setShowGuideModal(true)}
                            className="w-full px-4 py-3 bg-gray-700 hover:bg-gray-600 text-white rounded-lg font-semibold transition-colors"
                        >
                            📚 Integration Guide
                        </button>
                        <p className="text-xs text-gray-600 dark:text-gray-500 mt-2">
                            View code examples for Fragment Flow, Exchange Flow, and local development.
                        </p>
                    </section>
                </div>
            </div>
        </>
    );
}

// Snowflake OAuth Provisioner Extension UI Component
function SnowflakeOAuthExtensionUI({ spec, schema, onChange }) {
    const [blockedRoles, setBlockedRoles] = useState(spec?.blocked_roles?.join(', ') || '');
    const [scopes, setScopes] = useState(spec?.scopes?.join(', ') || '');
    const [clientSecretEnvVar, setClientSecretEnvVar] = useState(spec?.client_secret_env_var || 'SNOWFLAKE_CLIENT_SECRET');

    // Use a ref to store the latest onChange callback
    const onChangeRef = React.useRef(onChange);
    React.useEffect(() => {
        onChangeRef.current = onChange;
    }, [onChange]);

    // Update parent when values change
    useEffect(() => {
        const newSpec = {
            blocked_roles: blockedRoles.split(',').map(r => r.trim()).filter(r => r),
            scopes: scopes.split(',').map(s => s.trim()).filter(s => s),
            client_secret_env_var: clientSecretEnvVar.trim() || 'SNOWFLAKE_CLIENT_SECRET',
        };

        onChangeRef.current(newSpec);
    }, [blockedRoles, scopes, clientSecretEnvVar]);

    return (
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Left column: Form */}
            <div className="lg:col-span-2 space-y-4">
                <FormField
                    label="Additional Blocked Roles"
                    id="snowflake-blocked-roles"
                    type="textarea"
                    value={blockedRoles}
                    onChange={(e) => setBlockedRoles(e.target.value)}
                    placeholder="SYSADMIN, USERADMIN"
                    helperText="Comma-separated list of Snowflake roles to block. These will be ADDED to the backend-configured defaults (ACCOUNTADMIN, SECURITYADMIN). Users will not be able to select these roles when authenticating."
                />

                <FormField
                    label="Additional OAuth Scopes"
                    id="snowflake-scopes"
                    type="textarea"
                    value={scopes}
                    onChange={(e) => setScopes(e.target.value)}
                    placeholder="session:role:ANALYST, session:role:DEVELOPER"
                    helperText="Comma-separated list of additional OAuth scopes. These will be ADDED to the backend-configured defaults (usually 'refresh_token'). Use 'session:role:ROLENAME' to allow users to select specific roles."
                />

                <FormField
                    label="Client Secret Environment Variable"
                    id="snowflake-client-secret-env-var"
                    value={clientSecretEnvVar}
                    onChange={(e) => setClientSecretEnvVar(e.target.value)}
                    placeholder="SNOWFLAKE_CLIENT_SECRET"
                    helperText="Name of the environment variable where the OAuth client secret will be stored. Defaults to 'SNOWFLAKE_CLIENT_SECRET'."
                />

                <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4">
                    <h4 className="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">How This Works</h4>
                    <ol className="text-sm text-gray-600 dark:text-gray-400 space-y-2 list-decimal list-inside">
                        <li>This extension provisions a Snowflake SECURITY INTEGRATION (OAuth provider)</li>
                        <li>Automatically creates a Generic OAuth extension for end-user authentication</li>
                        <li>OAuth credentials are retrieved from Snowflake and stored encrypted</li>
                        <li>Users authenticate via Snowflake OAuth in your application</li>
                    </ol>
                </div>

                <div className="bg-yellow-900/20 border border-yellow-700 rounded-lg p-4">
                    <h4 className="text-sm font-semibold text-yellow-300 mb-2">⏱️ Initial Provisioning</h4>
                    <p className="text-sm text-yellow-200">
                        Creating the Snowflake integration typically takes <strong>10-30 seconds</strong>.
                        The extension will automatically create the OAuth integration in Snowflake and configure
                        the corresponding OAuth extension for your project.
                    </p>
                </div>
            </div>

            {/* Right column: Guidance */}
            <div className="space-y-4">
                <section>
                    <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Configuration Notes</h2>
                    <div className="bg-blue-900/20 border border-blue-700 rounded-lg p-3">
                        <p className="text-sm text-blue-200 mb-2">
                            <strong>Backend Configuration</strong>
                        </p>
                        <p className="text-xs text-blue-200">
                            Snowflake credentials (account, user, password/key) are configured at the server level.
                            You only need to specify additional blocked roles and scopes here.
                        </p>
                    </div>
                </section>

                <section>
                    <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Common Scopes</h2>
                    <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-3">
                        <ul className="text-xs text-gray-600 dark:text-gray-400 space-y-1">
                            <li><code>refresh_token</code> - Enable refresh tokens (default)</li>
                            <li><code>session:role:ANALYST</code> - Allow ANALYST role</li>
                            <li><code>session:role:DEVELOPER</code> - Allow DEVELOPER role</li>
                            <li><code>session:role:PUBLIC</code> - Allow PUBLIC role</li>
                        </ul>
                    </div>
                </section>

                <section>
                    <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Security</h2>
                    <div className="bg-gray-100 dark:bg-gray-800 rounded-lg p-3">
                        <p className="text-xs text-gray-600 dark:text-gray-400">
                            Blocked roles prevent users from accessing sensitive roles.
                            ACCOUNTADMIN and SECURITYADMIN are always blocked by default.
                        </p>
                    </div>
                </section>
            </div>
        </div>
    );
}

// Snowflake OAuth Provisioner Detail View Component
function SnowflakeOAuthDetailView({ extension, projectName }) {
    const status = extension.status || {};
    const spec = extension.spec || {};

    // Get state badge color
    const getStateBadge = () => {
        if (!status.state) return null;

        let badgeColor;
        const state = status.state;

        switch (state) {
            case 'Available':
                badgeColor = 'bg-green-600';
                break;
            case 'Pending':
            case 'TestingConnection':
            case 'CreatingIntegration':
            case 'RetrievingCredentials':
            case 'CreatingOAuthExtension':
                badgeColor = 'bg-yellow-600';
                break;
            case 'Failed':
                badgeColor = 'bg-red-600';
                break;
            case 'Deleting':
            case 'Deleted':
                badgeColor = 'bg-gray-600';
                break;
            default:
                badgeColor = 'bg-gray-600';
        }

        return (
            <span className={`${badgeColor} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase`}>
                {state}
            </span>
        );
    };

    return (
        <>
            <div className="mb-6">
                <div className="flex items-center space-x-3">
                    <h2 className="text-xl font-semibold text-gray-900 dark:text-gray-100">Snowflake OAuth Provisioner</h2>
                    {getStateBadge()}
                </div>
                <p className="text-sm text-gray-600 dark:text-gray-400 mt-2">
                    Automatically provisions Snowflake SECURITY INTEGRATIONs and configures OAuth extensions
                </p>
            </div>

            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {/* Snowflake Integration Details */}
                <section className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4">
                    <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-4">Snowflake Integration</h3>

                    <div className="space-y-3 text-sm">
                        {status.integration_name && (
                            <div>
                                <span className="text-gray-600 dark:text-gray-500">Integration Name:</span>
                                <span className="text-gray-700 dark:text-gray-300 ml-2 font-mono">{status.integration_name}</span>
                            </div>
                        )}

                        {status.oauth_client_id && (
                            <div>
                                <span className="text-gray-600 dark:text-gray-500">OAuth Client ID:</span>
                                <span className="text-gray-700 dark:text-gray-300 ml-2 font-mono text-xs">{status.oauth_client_id}</span>
                            </div>
                        )}

                        {status.redirect_uri && (
                            <div>
                                <span className="text-gray-600 dark:text-gray-500">Redirect URI:</span>
                                <span className="text-gray-700 dark:text-gray-300 ml-2 font-mono text-xs">{status.redirect_uri}</span>
                            </div>
                        )}

                        {status.created_at && (
                            <div>
                                <span className="text-gray-600 dark:text-gray-500">Created:</span>
                                <span className="text-gray-700 dark:text-gray-300 ml-2">{formatDate(status.created_at)}</span>
                            </div>
                        )}
                    </div>

                    {status.state === 'Available' && (
                        <div className="mt-4 p-3 bg-green-900/20 border border-green-700 rounded">
                            <p className="text-xs text-green-300">
                                ✓ Snowflake integration is active and configured
                            </p>
                        </div>
                    )}

                    {status.error && (
                        <div className="mt-4 p-3 bg-red-900/20 border border-red-700 rounded">
                            <p className="text-xs text-red-300">
                                <strong>Error:</strong> {status.error}
                            </p>
                        </div>
                    )}
                </section>

                {/* OAuth Extension Details */}
                <section className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4">
                    <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-4">OAuth Extension</h3>

                    {status.oauth_extension_name ? (
                        <div className="space-y-3">
                            <div className="text-sm">
                                <span className="text-gray-600 dark:text-gray-500">Extension Name:</span>
                                <span className="text-gray-700 dark:text-gray-300 ml-2 font-mono">{status.oauth_extension_name}</span>
                            </div>

                            {status.state === 'Available' && (
                                <div className="mt-4">
                                    <a
                                        href={`#project/${projectName}/extensions/oauth/${status.oauth_extension_name}`}
                                        className="inline-block bg-indigo-600 hover:bg-indigo-700 text-white text-sm font-medium px-4 py-2 rounded transition"
                                    >
                                        View OAuth Extension →
                                    </a>
                                </div>
                            )}

                            <div className="mt-4 p-3 bg-blue-900/20 border border-blue-700 rounded">
                                <p className="text-xs text-blue-200">
                                    The OAuth extension is automatically created and managed by this provisioner.
                                    Users can authenticate using their Snowflake credentials.
                                </p>
                            </div>
                        </div>
                    ) : (
                        <div className="text-sm text-gray-600 dark:text-gray-400">
                            OAuth extension will be created during provisioning
                        </div>
                    )}
                </section>

                {/* Configuration Summary */}
                <section className="bg-gray-100 dark:bg-gray-800 rounded-lg p-4 lg:col-span-2">
                    <h3 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-4">Configuration</h3>

                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div>
                            <h4 className="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2">Blocked Roles</h4>
                            {spec.blocked_roles && spec.blocked_roles.length > 0 ? (
                                <div className="flex flex-wrap gap-2">
                                    {spec.blocked_roles.map((role, idx) => (
                                        <span key={idx} className="bg-red-900/30 text-red-300 text-xs px-2 py-1 rounded">
                                            {role}
                                        </span>
                                    ))}
                                </div>
                            ) : (
                                <p className="text-xs text-gray-600 dark:text-gray-500">Using backend defaults only</p>
                            )}
                        </div>

                        <div>
                            <h4 className="text-sm font-semibold text-gray-600 dark:text-gray-400 mb-2">OAuth Scopes</h4>
                            {spec.scopes && spec.scopes.length > 0 ? (
                                <div className="flex flex-wrap gap-2">
                                    {spec.scopes.map((scope, idx) => (
                                        <span key={idx} className="bg-blue-900/30 text-blue-300 text-xs px-2 py-1 rounded font-mono">
                                            {scope}
                                        </span>
                                    ))}
                                </div>
                            ) : (
                                <p className="text-xs text-gray-600 dark:text-gray-500">Using backend defaults only</p>
                            )}
                        </div>
                    </div>

                    <div className="mt-4 p-3 bg-yellow-900/20 border border-yellow-700 rounded">
                        <p className="text-xs text-yellow-200">
                            <strong>Note:</strong> Additional roles and scopes are combined with backend defaults
                            (not replaced). ACCOUNTADMIN and SECURITYADMIN are always blocked.
                        </p>
                    </div>
                </section>
            </div>
        </>
    );
}

const SnowflakeOAuthExtensionAPI = {
    icon: '/assets/snowflake.jpg',

    renderStatusBadge(extension) {
        const status = extension.status || {};
        if (!status.state) return null;

        let badgeColor;
        const state = status.state;

        switch (state) {
            case 'Available':
                badgeColor = 'bg-green-600';
                break;
            case 'Pending':
            case 'TestingConnection':
            case 'CreatingIntegration':
            case 'RetrievingCredentials':
            case 'CreatingOAuthExtension':
                badgeColor = 'bg-yellow-600';
                break;
            case 'Failed':
                badgeColor = 'bg-red-600';
                break;
            case 'Deleting':
            case 'Deleted':
                badgeColor = 'bg-gray-600';
                break;
            default:
                badgeColor = 'bg-gray-600';
        }

        return (
            <span className={`${badgeColor} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase`}>
                {status.state}
            </span>
        );
    },

    renderOverviewTab(extension, projectName) {
        return <SnowflakeOAuthDetailView extension={extension} projectName={projectName} />;
    },

    renderConfigureTab(spec, schema, onChange, projectName, instanceName, isEnabled) {
        return <SnowflakeOAuthExtensionUI spec={spec} schema={schema} onChange={onChange} />;
    },
};

// Extension UI Registry
// Maps extension type identifiers to their UI API implementations
const ExtensionUIRegistry = {
    'aws-rds-provisioner': AwsRdsExtensionAPI,
    'oauth': OAuthExtensionAPI,
    'snowflake-oauth-provisioner': SnowflakeOAuthExtensionAPI,
    // Add more extension UIs here as needed
};

// AWS RDS Custom Detail View Component
function AwsRdsDetailView({ extension, projectName }) {
    const status = extension.status || {};
    const spec = extension.spec || {};
    const databases = status.databases || {};

    // Determine instance state badge color
    const getInstanceStateBadge = () => {
        if (!status.state) return null;

        let badgeColor;
        const state = status.state.toLowerCase();

        switch (state) {
            case 'available':
                badgeColor = 'bg-green-600';
                break;
            case 'creating':
            case 'pending':
                badgeColor = 'bg-yellow-600';
                break;
            case 'failed':
                badgeColor = 'bg-red-600';
                break;
            case 'deleting':
            case 'deleted':
                badgeColor = 'bg-gray-600';
                break;
            default:
                badgeColor = 'bg-gray-600';
        }

        return (
            <span className={`${badgeColor} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase inline-block`}>
                {status.state}
            </span>
        );
    };

    return (
        <div className="space-y-6">
            {/* Instance Information */}
            <section>
                <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">RDS Instance</h2>
                <div className="bg-white dark:bg-gray-900 rounded p-4 grid grid-cols-2 gap-4">
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">State</p>
                        <div className="mt-1">{getInstanceStateBadge()}</div>
                    </div>
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">Instance ID</p>
                        <p className="text-gray-700 dark:text-gray-300">{status.instance_id || 'N/A'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">Instance Size</p>
                        <p className="text-gray-700 dark:text-gray-300">{status.instance_size || 'N/A'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">Engine</p>
                        <p className="text-gray-700 dark:text-gray-300">{spec.engine || 'postgres'} {spec.engine_version || ''}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">Endpoint</p>
                        <p className="text-gray-700 dark:text-gray-300 font-mono text-xs">{status.endpoint || 'Pending...'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">Database Isolation</p>
                        <p className="text-gray-700 dark:text-gray-300 capitalize">{spec.database_isolation || 'shared'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-600 dark:text-gray-500">Master Username</p>
                        <p className="text-gray-700 dark:text-gray-300">{status.master_username || 'N/A'}</p>
                    </div>
                </div>

                {status.error && (
                    <div className="mt-3 p-3 bg-red-900/20 border border-red-700 rounded">
                        <p className="text-sm text-red-300">
                            <strong>Error:</strong> {status.error}
                        </p>
                    </div>
                )}
            </section>

            {/* Databases */}
            <section>
                <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">
                    Databases ({Object.keys(databases).length})
                </h2>

                {Object.keys(databases).length === 0 ? (
                    <div className="bg-white dark:bg-gray-900 rounded p-4 text-gray-600 dark:text-gray-400 text-center">
                        No databases provisioned yet
                    </div>
                ) : (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        {Object.entries(databases).map(([dbName, dbStatus]) => (
                            <DatabaseCard
                                key={dbName}
                                name={dbName}
                                status={dbStatus}
                            />
                        ))}
                    </div>
                )}
            </section>

            {/* Configuration */}
            <section>
                <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-200 mb-3">Environment Variables</h2>
                <div className="bg-white dark:bg-gray-900 rounded p-4 space-y-3">
                    <div>
                        <span className="text-gray-600 dark:text-gray-400 text-sm">Database URL Variable:</span>
                        <code className="ml-2 bg-gray-100 dark:bg-gray-800 px-2 py-1 rounded text-gray-900 dark:text-gray-200 text-sm">
                            {spec.database_url_env_var || 'DATABASE_URL'}
                        </code>
                    </div>
                    <label className="flex items-center space-x-2">
                        <input
                            type="checkbox"
                            checked={spec.inject_pg_vars !== false}
                            disabled
                            className="rounded"
                        />
                        <span className="text-gray-700 dark:text-gray-300 text-sm">
                            Inject <code className="bg-gray-100 dark:bg-gray-800 px-1 rounded">PG*</code> variables
                        </span>
                    </label>
                </div>
            </section>
        </div>
    );
}

// Database Card Component
function DatabaseCard({ name, status }) {
    // Determine status badge color
    let badgeColor;
    let statusText = status.status || 'Unknown';
    const state = (status.status || '').toLowerCase();

    switch (state) {
        case 'available':
            badgeColor = 'bg-green-600';
            break;
        case 'pending':
        case 'creatingdatabase':
        case 'creatinguser':
            badgeColor = 'bg-yellow-600';
            statusText = 'Provisioning';
            break;
        case 'terminating':
            badgeColor = 'bg-red-600';
            break;
        default:
            badgeColor = 'bg-gray-600';
    }

    const isScheduledForCleanup = status.cleanup_scheduled_at != null;
    const cleanupDate = isScheduledForCleanup
        ? new Date(status.cleanup_scheduled_at)
        : null;
    const cleanupTime = cleanupDate
        ? new Date(cleanupDate.getTime() + 60 * 60 * 1000) // +1 hour
        : null;

    return (
        <div className="bg-white dark:bg-gray-900 rounded-lg p-4 border border-gray-300 dark:border-gray-700">
            <div className="flex items-center justify-between mb-3">
                <h3 className="text-white font-semibold">{name}</h3>
                <span className={`${badgeColor} text-white text-xs font-semibold px-2 py-1 rounded uppercase`}>
                    {statusText}
                </span>
            </div>

            <div className="space-y-2 text-sm">
                <div>
                    <span className="text-gray-600 dark:text-gray-500">User:</span>
                    <span className="text-gray-700 dark:text-gray-300 ml-2 font-mono">{status.user}</span>
                </div>

                {isScheduledForCleanup && cleanupTime && (
                    <div className="mt-3 p-2 bg-yellow-900/20 border border-yellow-700 rounded">
                        <p className="text-xs text-yellow-300">
                            <strong>⏱️ Cleanup Scheduled</strong>
                        </p>
                        <p className="text-xs text-yellow-200 mt-1">
                            Will be deleted at {formatDate(cleanupTime.toISOString())}
                        </p>
                    </div>
                )}

                {status.status === 'Available' && !isScheduledForCleanup && (
                    <div className="mt-3 p-2 bg-green-900/20 border border-green-700 rounded">
                        <p className="text-xs text-green-300">
                            ✓ Database is active and ready
                        </p>
                    </div>
                )}
            </div>
        </div>
    );
}

// Helper functions to access extension UI API

// Check if an extension has a custom UI API registered
function hasExtensionUI(extensionType) {
    return extensionType in ExtensionUIRegistry;
}

// Get the extension UI API object
function getExtensionUIAPI(extensionType) {
    return ExtensionUIRegistry[extensionType] || null;
}

// Get the configure tab component (for backward compatibility)
function getExtensionUI(extensionType) {
    const api = getExtensionUIAPI(extensionType);
    return api?.renderConfigureTab ?
        (props) => api.renderConfigureTab(props.spec, props.schema, props.onChange) :
        null;
}

// Check if extension has custom overview tab
function hasExtensionDetailView(extensionType) {
    const api = getExtensionUIAPI(extensionType);
    return api?.renderOverviewTab != null;
}

// Get the overview tab component (for backward compatibility)
function getExtensionDetailView(extensionType) {
    const api = getExtensionUIAPI(extensionType);
    return api?.renderOverviewTab ?
        (props) => api.renderOverviewTab(props.extension, props.projectName) :
        null;
}

// Get custom status badge renderer
function getExtensionStatusBadge(extensionType) {
    const api = getExtensionUIAPI(extensionType);
    return api?.renderStatusBadge || null;
}

// Get the icon URL for an extension
function getExtensionIcon(extensionType) {
    const api = getExtensionUIAPI(extensionType);
    return api?.icon || null;
}
