// Extension UI Components Registry
// This file contains custom UI components for extensions that provide
// a form-based interface instead of just raw JSON editing.

const { useState, useEffect } = React;

// AWS RDS Extension UI Component
function AwsRdsExtensionUI({ spec, schema, onChange }) {
    const [engine, setEngine] = useState(spec?.engine || 'postgres');
    const [engineVersion, setEngineVersion] = useState(spec?.engine_version || '');
    const [databaseIsolation, setDatabaseIsolation] = useState(spec?.database_isolation || 'shared');
    const [injectDatabaseUrl, setInjectDatabaseUrl] = useState(spec?.inject_database_url !== false);
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
            inject_database_url: injectDatabaseUrl,
            inject_pg_vars: injectPgVars,
        };

        // Only include engine_version if it's not empty
        if (engineVersion) {
            newSpec.engine_version = engineVersion;
        }

        onChangeRef.current(newSpec);
    }, [engine, engineVersion, databaseIsolation, injectDatabaseUrl, injectPgVars]);

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
                <label className="flex items-center space-x-3">
                    <input
                        type="checkbox"
                        checked={injectDatabaseUrl}
                        onChange={(e) => setInjectDatabaseUrl(e.target.checked)}
                        className="w-4 h-4 text-indigo-600 bg-gray-700 border-gray-600 rounded focus:ring-indigo-500 focus:ring-2"
                    />
                    <span className="text-sm text-gray-300">
                        Inject <code className="bg-gray-700 px-1 rounded">DATABASE_URL</code>
                        <span className="text-gray-500 ml-2">(full connection string)</span>
                    </span>
                </label>
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
            </div>

            <div className="bg-gray-800 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-gray-300 mb-2">About This Extension</h4>
                <p className="text-sm text-gray-400">
                    This extension provisions a PostgreSQL database on AWS RDS. The instance size, disk size,
                    and other infrastructure settings are configured at the server level.
                </p>
                <p className="text-sm text-gray-400 mt-2">
                    <strong>Shared mode:</strong> All deployment groups (default, staging, etc.) use the same database.
                    This is simpler and suitable for most applications where deployment groups represent different environments.
                </p>
                <p className="text-sm text-gray-400 mt-2">
                    <strong>Isolated mode:</strong> Each deployment group gets its own empty database.
                    This provides true data isolation and is useful for multi-tenant applications or testing with separate datasets.
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

        let badgeColor = 'bg-gray-600';
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

    renderOverviewTab(extension) {
        return <AwsRdsDetailView extension={extension} />;
    },

    renderConfigureTab(spec, schema, onChange) {
        return <AwsRdsExtensionUI spec={spec} schema={schema} onChange={onChange} />;
    }
};

// Extension UI Registry
// Maps extension type identifiers to their UI API implementations
const ExtensionUIRegistry = {
    'aws-rds-provisioner': AwsRdsExtensionAPI,
    // Add more extension UIs here as needed
};

// AWS RDS Custom Detail View Component
function AwsRdsDetailView({ extension }) {
    const status = extension.status || {};
    const spec = extension.spec || {};
    const databases = status.databases || {};

    // Determine instance state badge color
    const getInstanceStateBadge = () => {
        if (!status.state) return null;

        let badgeColor = 'bg-gray-600';
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
                <div className="bg-gray-900 rounded p-4 space-y-2">
                    <label className="flex items-center space-x-2">
                        <input
                            type="checkbox"
                            checked={spec.inject_database_url !== false}
                            disabled
                            className="rounded"
                        />
                        <span className="text-gray-300 text-sm">
                            Inject <code className="bg-gray-800 px-1 rounded">DATABASE_URL</code>
                        </span>
                    </label>
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
    let badgeColor = 'bg-gray-600';
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
        (props) => api.renderOverviewTab(props.extension) :
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
