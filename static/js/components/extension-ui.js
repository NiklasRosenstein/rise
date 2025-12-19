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
                <h4 className="text-sm font-semibold text-gray-300">Environment Variables</h4>
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
                    <span className="text-sm text-gray-300">
                        Inject <code className="bg-gray-700 px-1 rounded">PG*</code> variables
                        <span className="text-gray-500 ml-2">(PGHOST, PGPORT, PGDATABASE, PGUSER, PGPASSWORD)</span>
                    </span>
                </label>
                <p className="text-xs text-gray-500">
                    Note: Only one RDS extension should have PG* variables enabled per project, as they will override each other.
                </p>
            </div>

            <div className="bg-gray-800 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-gray-300 mb-2">About This Extension</h4>
                <p className="text-sm text-gray-400">
                    This extension provisions a PostgreSQL database on AWS RDS. The instance size, disk size,
                    and other infrastructure settings are configured at the server level.
                </p>
                <p className="text-sm text-gray-400 mt-2">
                    <strong>Shared mode:</strong> All deployment groups (default, staging, etc.) use the same database.
                    This means staging deployments use the same database as production.
                </p>
                <p className="text-sm text-gray-400 mt-2">
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

    renderConfigureTab(spec, schema, onChange) {
        return <AwsRdsExtensionUI spec={spec} schema={schema} onChange={onChange} />;
    }
};

// OAuth Extension UI Component
function OAuthExtensionUI({ spec, schema, onChange }) {
    const [providerName, setProviderName] = useState(spec?.provider_name || '');
    const [description, setDescription] = useState(spec?.description || '');
    const [clientId, setClientId] = useState(spec?.client_id || '');
    const [clientSecretRef, setClientSecretRef] = useState(spec?.client_secret_ref || '');
    const [authorizationEndpoint, setAuthorizationEndpoint] = useState(spec?.authorization_endpoint || '');
    const [tokenEndpoint, setTokenEndpoint] = useState(spec?.token_endpoint || '');
    const [scopes, setScopes] = useState(spec?.scopes?.join(', ') || '');

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
            scopes: 'session:role:ANALYST, refresh_token'
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
        }
    };

    return (
        <div className="space-y-4">
            {/* Provider Templates */}
            <div className="bg-gray-800 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-gray-300 mb-2">Quick Start Templates</h4>
                <div className="flex gap-2 flex-wrap">
                    <button
                        type="button"
                        onClick={() => applyTemplate('snowflake')}
                        className="px-3 py-1 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-200"
                    >
                        Snowflake
                    </button>
                    <button
                        type="button"
                        onClick={() => applyTemplate('google')}
                        className="px-3 py-1 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-200"
                    >
                        Google
                    </button>
                    <button
                        type="button"
                        onClick={() => applyTemplate('github')}
                        className="px-3 py-1 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-200"
                    >
                        GitHub
                    </button>
                </div>
                <p className="text-xs text-gray-500 mt-2">
                    Click a template to auto-fill common provider endpoints. Don't forget to update account-specific values!
                </p>
            </div>

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

            <div className="bg-gray-800 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-gray-300 mb-2">About This Extension</h4>
                <p className="text-sm text-gray-400">
                    The Generic OAuth 2.0 extension allows your application to authenticate end users via any OAuth 2.0 provider
                    (Snowflake, Google, GitHub, custom SSO, etc.) without managing client secrets locally.
                </p>
                <p className="text-sm text-gray-400 mt-2">
                    <strong>Security:</strong> Client secrets are stored encrypted and never exposed to your application.
                    Tokens are delivered in URL fragments for frontend apps or via secure exchange for backend apps.
                </p>
            </div>

            <div className="bg-blue-900/20 border border-blue-700 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-blue-300 mb-2">⚙️ Setup Required</h4>
                <p className="text-sm text-blue-200 mb-2">
                    Before creating this extension:
                </p>
                <ol className="text-sm text-blue-200 list-decimal list-inside space-y-1">
                    <li>Register an OAuth application with your provider to get client credentials</li>
                    <li>Store the client secret as an encrypted environment variable:
                        <code className="block bg-gray-800 px-2 py-1 rounded mt-1 text-xs">
                            rise env set PROJECT_NAME {clientSecretRef || 'OAUTH_SECRET'} "your_secret" --secret
                        </code>
                    </li>
                    <li>Configure the OAuth callback URL in your provider:
                        <code className="block bg-gray-800 px-2 py-1 rounded mt-1 text-xs">
                            https://api.rise.dev/api/v1/oauth/callback/PROJECT_NAME/EXTENSION_NAME
                        </code>
                    </li>
                </ol>
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
            return (
                <span className="bg-green-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">
                    Configured
                </span>
            );
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

    renderConfigureTab(spec, schema, onChange) {
        return <OAuthExtensionUI spec={spec} schema={schema} onChange={onChange} />;
    }
};

// OAuth Detail View Component
function OAuthDetailView({ extension, projectName }) {
    const status = extension.status || {};
    const spec = extension.spec || {};

    const scopesArray = spec.scopes || [];
    const extensionName = extension.extension;

    // Get toast function from context
    const { showToast } = useToast();

    // Check if we're returning from OAuth flow
    React.useEffect(() => {
        // Check if we have OAuth callback in the URL fragment
        if (window.location.hash && (window.location.hash.includes('access_token=') || window.location.hash.includes('error='))) {
            const fragment = window.location.hash.substring(1);
            const params = new URLSearchParams(fragment);

            // Restore the original page location from sessionStorage
            const returnPath = sessionStorage.getItem('oauth_return_path');
            sessionStorage.removeItem('oauth_return_path');

            const error = params.get('error');
            const errorDescription = params.get('error_description');
            const accessToken = params.get('access_token');

            if (error) {
                // OAuth flow failed
                const message = errorDescription || `OAuth flow failed: ${error}`;
                showToast(message, 'error');

                if (returnPath) {
                    window.location.hash = returnPath;
                } else {
                    window.location.hash = `project/${projectName}/extension/${extensionName}`;
                }
            } else if (accessToken) {
                // OAuth flow succeeded
                const expiresAt = params.get('expires_at');
                const expiresIn = params.get('expires_in');

                // Calculate expiration time
                let expiresAtDate;
                if (expiresAt) {
                    expiresAtDate = new Date(expiresAt);
                } else if (expiresIn) {
                    expiresAtDate = new Date(Date.now() + parseInt(expiresIn) * 1000);
                }

                // Show success toast
                const message = `OAuth flow successful! Token expires ${expiresAtDate ? expiresAtDate.toLocaleString() : 'soon'}`;
                showToast(message, 'success');

                if (returnPath) {
                    // Navigate back to the extension page
                    window.location.hash = returnPath;
                } else {
                    // Fallback: just clean the URL
                    window.location.hash = `project/${projectName}/extension/${extensionName}`;
                }
            }
        }
    }, [projectName, extensionName, showToast]);

    return (
        <div className="space-y-6">
            {/* Configuration Status */}
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Configuration Status</h2>
                <div className="bg-gray-900 rounded p-4">
                    {status.error ? (
                        <div className="p-3 bg-red-900/20 border border-red-700 rounded">
                            <p className="text-sm text-red-300">
                                <strong>Error:</strong> {status.error}
                            </p>
                        </div>
                    ) : status.configured_at ? (
                        <div className="p-3 bg-green-900/20 border border-green-700 rounded">
                            <p className="text-sm text-green-300">
                                ✓ OAuth provider configured at {formatDate(status.configured_at)}
                            </p>
                        </div>
                    ) : (
                        <div className="p-3 bg-gray-800 rounded">
                            <p className="text-sm text-gray-400">
                                Configuration pending...
                            </p>
                        </div>
                    )}
                </div>
            </section>

            {/* Provider Configuration */}
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">OAuth Provider</h2>
                <div className="bg-gray-900 rounded p-4 space-y-3">
                    <div>
                        <p className="text-sm text-gray-500">Provider Name</p>
                        <p className="text-gray-300 font-semibold">{spec.provider_name || 'N/A'}</p>
                    </div>
                    {spec.description && (
                        <div>
                            <p className="text-sm text-gray-500">Description</p>
                            <p className="text-gray-300">{spec.description}</p>
                        </div>
                    )}
                    <div>
                        <p className="text-sm text-gray-500">Client ID</p>
                        <p className="text-gray-300 font-mono text-sm">{spec.client_id || 'N/A'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Client Secret Reference</p>
                        <p className="text-gray-300 font-mono text-sm">{spec.client_secret_ref || 'N/A'}</p>
                    </div>
                </div>
            </section>

            {/* Endpoints */}
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">OAuth Endpoints</h2>
                <div className="bg-gray-900 rounded p-4 space-y-3">
                    <div>
                        <p className="text-sm text-gray-500">Authorization Endpoint</p>
                        <p className="text-gray-300 font-mono text-xs break-all">{spec.authorization_endpoint || 'N/A'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Token Endpoint</p>
                        <p className="text-gray-300 font-mono text-xs break-all">{spec.token_endpoint || 'N/A'}</p>
                    </div>
                </div>
            </section>

            {/* Scopes */}
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">
                    OAuth Scopes ({scopesArray.length})
                </h2>
                <div className="bg-gray-900 rounded p-4">
                    {scopesArray.length === 0 ? (
                        <p className="text-gray-400 text-sm">No scopes configured</p>
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

            {/* Test OAuth Flow */}
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Test OAuth Flow</h2>
                <div className="bg-gray-900 rounded p-4 space-y-3">
                    <p className="text-sm text-gray-400">
                        Click the button below to test the OAuth flow. You'll be redirected to the OAuth provider for authentication,
                        then returned to this page with a success notification.
                    </p>
                    <button
                        onClick={() => {
                            // Store the current hash location to return to after OAuth
                            const currentHash = window.location.hash.substring(1); // Remove leading #
                            sessionStorage.setItem('oauth_return_path', currentHash);

                            // Use the origin as redirect URI (hash router doesn't work with fragments)
                            const redirectUri = window.location.origin + '/';
                            const authUrl = `/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize?redirect_uri=${encodeURIComponent(redirectUri)}`;
                            window.location.href = authUrl;
                        }}
                        className="px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white rounded-lg font-semibold transition-colors"
                    >
                        Test OAuth Flow
                    </button>
                    <p className="text-xs text-gray-500">
                        After authentication, you'll be redirected back to this page and see a success notification.
                    </p>
                </div>
            </section>

            {/* Integration Guide */}
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Integration Guide</h2>
                <div className="bg-gray-900 rounded p-4 space-y-4">
                    <div>
                        <p className="text-sm text-gray-400 mb-2">
                            <strong>OAuth Authorization URL:</strong>
                        </p>
                        <code className="block bg-gray-800 px-3 py-2 rounded text-xs text-gray-200 break-all">
                            {`https://api.rise.dev/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize`}
                        </code>
                    </div>
                    <div>
                        <p className="text-sm text-gray-400 mb-2">
                            <strong>OAuth Callback URL (configured in provider):</strong>
                        </p>
                        <code className="block bg-gray-800 px-3 py-2 rounded text-xs text-gray-200 break-all">
                            {`https://api.rise.dev/api/v1/oauth/callback/${projectName}/${extensionName}`}
                        </code>
                    </div>

                    <div className="pt-2 border-t border-gray-700">
                        <p className="text-sm font-semibold text-gray-300 mb-3">Fragment Flow (Default - For SPAs)</p>
                        <p className="text-xs text-gray-400 mb-2">
                            Tokens are returned in the URL fragment. Best for single-page applications.
                        </p>
                        <pre className="bg-gray-800 px-3 py-2 rounded text-xs text-gray-200 overflow-x-auto">
{`// Initiate OAuth login (fragment flow is default)
function login() {
  const authUrl = 'https://api.rise.dev/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize';
  window.location.href = authUrl;
}

// Extract tokens from URL fragment after redirect
const fragment = window.location.hash.substring(1);
const params = new URLSearchParams(fragment);
const accessToken = params.get('access_token');
const idToken = params.get('id_token');
const expiresAt = params.get('expires_at');

// Store securely
sessionStorage.setItem('access_token', accessToken);`}
                        </pre>
                    </div>

                    <div className="pt-2 border-t border-gray-700">
                        <p className="text-sm font-semibold text-gray-300 mb-3">Exchange Token Flow (For Backend Apps)</p>
                        <p className="text-xs text-gray-400 mb-2">
                            Your backend receives a temporary exchange token and exchanges it for OAuth tokens.
                        </p>
                        <pre className="bg-gray-800 px-3 py-2 rounded text-xs text-gray-200 overflow-x-auto">
{`// Initiate OAuth login with exchange flow
app.get('/login', (req, res) => {
  const authUrl = 'https://api.rise.dev/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize?flow=exchange';
  res.redirect(authUrl);
});

// Handle OAuth callback
app.get('/oauth/callback', async (req, res) => {
  const exchangeToken = req.query.exchange_token;

  // Exchange for actual OAuth tokens
  const response = await fetch(
    \`https://api.rise.dev/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/exchange?exchange_token=\${exchangeToken}\`
  );

  const tokens = await response.json();

  // Store in HttpOnly session cookie
  req.session.accessToken = tokens.access_token;
  req.session.idToken = tokens.id_token;

  res.redirect('/dashboard');
});`}
                        </pre>
                    </div>

                    <div className="pt-2 border-t border-gray-700">
                        <p className="text-sm font-semibold text-gray-300 mb-2">Local Development</p>
                        <p className="text-xs text-gray-400 mb-2">
                            Override the redirect URI for local testing:
                        </p>
                        <pre className="bg-gray-800 px-3 py-2 rounded text-xs text-gray-200 overflow-x-auto">
{`// Fragment flow
const authUrl = 'https://api.rise.dev/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize?redirect_uri=http://localhost:3000/callback';

// Exchange flow
const authUrl = 'https://api.rise.dev/api/v1/projects/${projectName}/extensions/${extensionName}/oauth/authorize?flow=exchange&redirect_uri=http://localhost:3000/oauth/callback';`}
                        </pre>
                    </div>
                </div>
            </section>

            {/* Documentation Link */}
            <section>
                <div className="bg-blue-900/20 border border-blue-700 rounded-lg p-4">
                    <h4 className="text-sm font-semibold text-blue-300 mb-2">📚 Documentation</h4>
                    <p className="text-sm text-blue-200">
                        For detailed documentation on OAuth flows, security, and advanced configuration,
                        see the <a href="/docs/oauth.md" className="underline">OAuth Extension Documentation</a>.
                    </p>
                </div>
            </section>
        </div>
    );
}

// Extension UI Registry
// Maps extension type identifiers to their UI API implementations
const ExtensionUIRegistry = {
    'aws-rds-provisioner': AwsRdsExtensionAPI,
    'oauth': OAuthExtensionAPI,
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
                <h2 className="text-lg font-semibold text-gray-200 mb-3">RDS Instance</h2>
                <div className="bg-gray-900 rounded p-4 grid grid-cols-2 gap-4">
                    <div>
                        <p className="text-sm text-gray-500">State</p>
                        <div className="mt-1">{getInstanceStateBadge()}</div>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Instance ID</p>
                        <p className="text-gray-300">{status.instance_id || 'N/A'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Instance Size</p>
                        <p className="text-gray-300">{status.instance_size || 'N/A'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Engine</p>
                        <p className="text-gray-300">{spec.engine || 'postgres'} {spec.engine_version || ''}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Endpoint</p>
                        <p className="text-gray-300 font-mono text-xs">{status.endpoint || 'Pending...'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Database Isolation</p>
                        <p className="text-gray-300 capitalize">{spec.database_isolation || 'shared'}</p>
                    </div>
                    <div>
                        <p className="text-sm text-gray-500">Master Username</p>
                        <p className="text-gray-300">{status.master_username || 'N/A'}</p>
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
                <h2 className="text-lg font-semibold text-gray-200 mb-3">
                    Databases ({Object.keys(databases).length})
                </h2>

                {Object.keys(databases).length === 0 ? (
                    <div className="bg-gray-900 rounded p-4 text-gray-400 text-center">
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
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Environment Variables</h2>
                <div className="bg-gray-900 rounded p-4 space-y-3">
                    <div>
                        <span className="text-gray-400 text-sm">Database URL Variable:</span>
                        <code className="ml-2 bg-gray-800 px-2 py-1 rounded text-gray-200 text-sm">
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
                        <span className="text-gray-300 text-sm">
                            Inject <code className="bg-gray-800 px-1 rounded">PG*</code> variables
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
        <div className="bg-gray-900 rounded-lg p-4 border border-gray-700">
            <div className="flex items-center justify-between mb-3">
                <h3 className="text-white font-semibold">{name}</h3>
                <span className={`${badgeColor} text-white text-xs font-semibold px-2 py-1 rounded uppercase`}>
                    {statusText}
                </span>
            </div>

            <div className="space-y-2 text-sm">
                <div>
                    <span className="text-gray-500">User:</span>
                    <span className="text-gray-300 ml-2 font-mono">{status.user}</span>
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
