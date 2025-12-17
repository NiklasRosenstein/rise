// Extension UI Components Registry
// This file contains custom UI components for extensions that provide
// a form-based interface instead of just raw JSON editing.

const { useState, useEffect } = React;

// AWS RDS Extension UI Component
function AwsRdsExtensionUI({ spec, onChange }) {
    const [engine, setEngine] = useState(spec?.engine || 'postgres');
    const [engineVersion, setEngineVersion] = useState(spec?.engine_version || '');

    // Update parent when values change
    useEffect(() => {
        // Build the spec object, omitting empty values
        const newSpec = {
            engine,
        };

        // Only include engine_version if it's not empty
        if (engineVersion) {
            newSpec.engine_version = engineVersion;
        }

        onChange(newSpec);
    }, [engine, engineVersion, onChange]);

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
                placeholder="e.g., 16.2 (leave empty for default)"
            />

            <div className="bg-gray-800 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-gray-300 mb-2">About This Extension</h4>
                <p className="text-sm text-gray-400">
                    This extension provisions a PostgreSQL database on AWS RDS. The instance size, disk size,
                    and other infrastructure settings are configured at the server level.
                </p>
                <p className="text-sm text-gray-400 mt-2">
                    The extension automatically creates a separate database for each deployment group
                    (default, staging, etc.) and injects the connection credentials as environment variables.
                </p>
            </div>

            <div className="bg-blue-900/20 border border-blue-700 rounded-lg p-4">
                <h4 className="text-sm font-semibold text-blue-300 mb-2">Environment Variables</h4>
                <p className="text-sm text-blue-200 mb-2">
                    The following environment variables will be automatically injected into your deployments:
                </p>
                <ul className="text-sm text-blue-200 space-y-1 list-disc list-inside">
                    <li><code className="bg-blue-900/30 px-1 rounded">DATABASE_URL</code> - Full connection string</li>
                    <li><code className="bg-blue-900/30 px-1 rounded">DB_HOST</code> - Database hostname</li>
                    <li><code className="bg-blue-900/30 px-1 rounded">DB_PORT</code> - Database port (5432)</li>
                    <li><code className="bg-blue-900/30 px-1 rounded">DB_NAME</code> - Database name</li>
                    <li><code className="bg-blue-900/30 px-1 rounded">DB_USER</code> - Database username</li>
                    <li><code className="bg-blue-900/30 px-1 rounded">DB_PASSWORD</code> - Database password</li>
                </ul>
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
