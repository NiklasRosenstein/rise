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

        onChange(newSpec);
    }, [engine, engineVersion, databaseIsolation, injectDatabaseUrl, injectPgVars, onChange]);

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

// Extension UI Registry
// Maps extension type identifiers to their UI components
const ExtensionUIRegistry = {
    'aws-rds-provisioner': AwsRdsExtensionUI,
    // Add more extension UIs here as needed
};

// Extension Icon Registry
// Maps extension type identifiers to their icon URLs
const ExtensionIconRegistry = {
    'aws-rds-provisioner': '/assets/aws_rds_aurora.jpg',
    // Add more extension icons here as needed
};

// Helper function to check if an extension has custom UI
function hasExtensionUI(extensionName) {
    return extensionName in ExtensionUIRegistry;
}

// Helper function to get the UI component for an extension
function getExtensionUI(extensionName) {
    return ExtensionUIRegistry[extensionName] || null;
}

// Helper function to get the icon URL for an extension
function getExtensionIcon(extensionType) {
    return ExtensionIconRegistry[extensionType] || null;
}
