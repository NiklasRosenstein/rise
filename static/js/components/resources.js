// Resource management components for Rise Dashboard (Service Accounts, Domains, Environment Variables)
// This file depends on React, utils.js, components/ui.js, and components/toast.js being loaded first

const { useState, useEffect, useCallback } = React;

// Helper function to normalize JSON for comparison (sorts keys recursively)
function normalizeJSON(jsonString) {
    try {
        const obj = JSON.parse(jsonString);
        return JSON.stringify(sortObjectKeys(obj), null, 2);
    } catch (e) {
        return jsonString;
    }
}

// Recursively sort object keys for consistent comparison
function sortObjectKeys(obj) {
    if (obj === null || typeof obj !== 'object' || Array.isArray(obj)) {
        return obj;
    }
    return Object.keys(obj)
        .sort()
        .reduce((sorted, key) => {
            sorted[key] = sortObjectKeys(obj[key]);
            return sorted;
        }, {});
}

// Service Accounts Component
function ServiceAccountsList({ projectName }) {
    const [serviceAccounts, setServiceAccounts] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [editingSA, setEditingSA] = useState(null);
    const [formData, setFormData] = useState({ issuer_url: '', aud: '', claims: {} });
    const [claimsText, setClaimsText] = useState('');
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [saToDelete, setSAToDelete] = useState(null);
    const [deleting, setDeleting] = useState(false);
    const [saving, setSaving] = useState(false);
    const { showToast } = useToast();

    const loadServiceAccounts = useCallback(async () => {
        try {
            const response = await api.getProjectServiceAccounts(projectName);
            setServiceAccounts(response.workload_identities || []);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName]);

    useEffect(() => {
        loadServiceAccounts();
    }, [loadServiceAccounts]);

    const handleAddClick = () => {
        setEditingSA(null);
        // Default aud to Rise backend URL (where the API is hosted)
        const defaultAud = CONFIG.backendUrl;
        setFormData({ issuer_url: '', aud: defaultAud, claims: {} });
        setClaimsText('');
        setIsModalOpen(true);
    };

    const handleEditClick = (sa) => {
        setEditingSA(sa);
        // Extract aud from existing claims
        const aud = sa.claims?.aud || '';
        setFormData({ issuer_url: sa.issuer_url, aud, claims: sa.claims || {} });
        // Convert claims object to JSON string for editing (excluding aud)
        const claimsObj = { ...sa.claims };
        delete claimsObj.aud; // aud is handled separately
        setClaimsText(JSON.stringify(claimsObj, null, 2));
        setIsModalOpen(true);
    };

    const handleDeleteClick = (sa) => {
        setSAToDelete(sa);
        setConfirmDialogOpen(true);
    };

    const handleSave = async () => {
        if (!formData.issuer_url) {
            showToast('Issuer URL is required', 'error');
            return;
        }

        if (!formData.aud) {
            showToast('Audience (aud) is required', 'error');
            return;
        }

        // Parse additional claims from text
        let claims = {};
        try {
            if (claimsText.trim()) {
                claims = JSON.parse(claimsText);
            }
        } catch (err) {
            showToast('Invalid JSON in additional claims', 'error');
            return;
        }

        // Add aud claim from form data
        claims.aud = formData.aud;

        setSaving(true);
        try {
            if (editingSA) {
                await api.updateServiceAccount(projectName, editingSA.id, formData.issuer_url, claims);
                showToast('Service account updated successfully', 'success');
            } else {
                await api.createServiceAccount(projectName, formData.issuer_url, claims);
                showToast('Service account created successfully', 'success');
            }
            setIsModalOpen(false);
            loadServiceAccounts();
        } catch (err) {
            showToast(`Failed to ${editingSA ? 'update' : 'create'} service account: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    const handleDeleteConfirm = async () => {
        if (!saToDelete) return;

        setDeleting(true);
        try {
            await api.deleteServiceAccount(projectName, saToDelete.id);
            showToast(`Service account ${saToDelete.email} deleted successfully`, 'success');
            setConfirmDialogOpen(false);
            setSAToDelete(null);
            loadServiceAccounts();
        } catch (err) {
            showToast(`Failed to delete service account: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading service accounts: {error}</p>;

    return (
        <div>
            <div className="mb-4 flex justify-end">
                <Button variant="primary" size="sm" onClick={handleAddClick}>
                    Create Service Account
                </Button>
            </div>
            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Email</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Issuer URL</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Claims</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Created</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Actions</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {serviceAccounts.length === 0 ? (
                            <tr>
                                <td colSpan="5" className="px-6 py-8 text-center text-gray-400">
                                    No service accounts found.
                                </td>
                            </tr>
                        ) : (
                            serviceAccounts.map(sa => (
                            <tr key={sa.id} className="hover:bg-gray-800/50 transition-colors">
                                <td className="px-6 py-4 text-sm text-gray-200">{sa.email}</td>
                                <td className="px-6 py-4 text-sm text-gray-300 break-all max-w-xs">{sa.issuer_url}</td>
                                <td className="px-6 py-4 text-xs font-mono text-gray-300">
                                    {Object.entries(sa.claims || {})
                                        .map(([key, value]) => `${key}=${value}`)
                                        .join(', ')}
                                </td>
                                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{formatDate(sa.created_at)}</td>
                                <td className="px-6 py-4 text-sm">
                                    <div className="flex gap-2">
                                        <Button
                                            variant="secondary"
                                            size="sm"
                                            onClick={() => handleEditClick(sa)}
                                        >
                                            Edit
                                        </Button>
                                        <Button
                                            variant="danger"
                                            size="sm"
                                            onClick={() => handleDeleteClick(sa)}
                                        >
                                            Delete
                                        </Button>
                                    </div>
                                </td>
                            </tr>
                        ))
                        )}
                    </tbody>
                </table>
            </div>

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title={editingSA ? 'Edit Service Account' : 'Create Service Account'}
            >
                <div className="space-y-4">
                    <FormField
                        label="Issuer URL"
                        id="sa-issuer-url"
                        value={formData.issuer_url}
                        onChange={(e) => setFormData({ ...formData, issuer_url: e.target.value })}
                        placeholder="https://token.actions.githubusercontent.com"
                        required
                    />
                    <FormField
                        label="Audience (aud)"
                        id="sa-aud"
                        value={formData.aud}
                        onChange={(e) => setFormData({ ...formData, aud: e.target.value })}
                        placeholder={CONFIG.backendUrl}
                        required
                    />
                    <FormField
                        label="Additional Claims (JSON)"
                        id="sa-claims"
                        type="textarea"
                        value={claimsText}
                        onChange={(e) => setClaimsText(e.target.value)}
                        placeholder={`{\n  "sub": "repo:myorg/myrepo:*"\n}`}
                        rows={5}
                    />
                    <p className="text-sm text-gray-500">
                        <strong>Note:</strong> Additional claims should be provided as a JSON object. The <code className="bg-gray-800 px-1 rounded">aud</code> claim is configured separately above.
                    </p>

                    <div className="flex justify-end gap-3 pt-4">
                        <Button
                            variant="secondary"
                            onClick={() => setIsModalOpen(false)}
                            disabled={saving}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={handleSave}
                            loading={saving}
                        >
                            {editingSA ? 'Update' : 'Create'}
                        </Button>
                    </div>
                </div>
            </Modal>

            <ConfirmDialog
                isOpen={confirmDialogOpen}
                onClose={() => {
                    setConfirmDialogOpen(false);
                    setSAToDelete(null);
                }}
                onConfirm={handleDeleteConfirm}
                title="Delete Service Account"
                message={`Are you sure you want to delete the service account "${saToDelete?.email}"? This action cannot be undone.`}
                confirmText="Delete Service Account"
                variant="danger"
                loading={deleting}
            />
        </div>
    );
}

// Custom Domains Component
function DomainsList({ projectName }) {
    const [domains, setDomains] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [formData, setFormData] = useState({ domain: '' });
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [domainToDelete, setDomainToDelete] = useState(null);
    const [deleting, setDeleting] = useState(false);
    const [saving, setSaving] = useState(false);
    const { showToast } = useToast();

    const loadDomains = useCallback(async () => {
        try {
            const response = await api.getProjectDomains(projectName);
            setDomains(response.domains || []);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName]);

    useEffect(() => {
        loadDomains();
    }, [loadDomains]);

    const handleAddClick = () => {
        setFormData({ domain: '' });
        setIsModalOpen(true);
    };

    const handleDeleteClick = (domain) => {
        setDomainToDelete(domain);
        setConfirmDialogOpen(true);
    };

    const handleSave = async () => {
        if (!formData.domain) {
            showToast('Domain is required', 'error');
            return;
        }

        setSaving(true);
        try {
            await api.addCustomDomain(projectName, formData.domain);
            showToast(`Custom domain ${formData.domain} added successfully`, 'success');
            setIsModalOpen(false);
            loadDomains();
        } catch (err) {
            showToast(`Failed to add custom domain: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    const handleDeleteConfirm = async () => {
        if (!domainToDelete) return;

        setDeleting(true);
        try {
            await api.deleteCustomDomain(projectName, domainToDelete.domain);
            showToast(`Custom domain ${domainToDelete.domain} deleted successfully`, 'success');
            setConfirmDialogOpen(false);
            setDomainToDelete(null);
            loadDomains();
        } catch (err) {
            showToast(`Failed to delete custom domain: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading custom domains: {error}</p>;

    return (
        <div>
            <div className="mb-4 flex justify-end">
                <Button variant="primary" size="sm" onClick={handleAddClick}>
                    Add Domain
                </Button>
            </div>
            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Domain</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Created</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Actions</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {domains.length === 0 ? (
                            <tr>
                                <td colSpan="3" className="px-6 py-8 text-center text-gray-400">
                                    No custom domains configured.
                                </td>
                            </tr>
                        ) : (
                            domains.map(domain => (
                            <tr key={domain.id} className="hover:bg-gray-800/50 transition-colors">
                                <td className="px-6 py-4 text-sm font-mono text-gray-200">{domain.domain}</td>
                                <td className="px-6 py-4 text-sm text-gray-300">{formatDate(domain.created_at)}</td>
                                <td className="px-6 py-4 text-sm">
                                    <Button
                                        variant="danger"
                                        size="sm"
                                        onClick={() => handleDeleteClick(domain)}
                                    >
                                        Delete
                                    </Button>
                                </td>
                            </tr>
                        ))
                        )}
                    </tbody>
                </table>
            </div>

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title="Add Custom Domain"
            >
                <div className="space-y-4">
                    <FormField
                        label="Domain"
                        id="domain-name"
                        value={formData.domain}
                        onChange={(e) => setFormData({ ...formData, domain: e.target.value })}
                        placeholder="example.com"
                        required
                    />
                    <p className="text-sm text-gray-500">
                        <strong>Note:</strong> Make sure to configure your DNS to point this domain to your Rise deployment before adding it.
                        The domain will be added to the ingress for the default deployment group only.
                    </p>

                    <div className="flex justify-end gap-3 pt-4">
                        <Button
                            variant="secondary"
                            onClick={() => setIsModalOpen(false)}
                            disabled={saving}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={handleSave}
                            loading={saving}
                        >
                            Add Domain
                        </Button>
                    </div>
                </div>
            </Modal>

            <ConfirmDialog
                isOpen={confirmDialogOpen}
                onClose={() => {
                    setConfirmDialogOpen(false);
                    setDomainToDelete(null);
                }}
                onConfirm={handleDeleteConfirm}
                title="Delete Custom Domain"
                message={`Are you sure you want to delete the custom domain "${domainToDelete?.domain}"? This action cannot be undone.`}
                confirmText="Delete Domain"
                variant="danger"
                loading={deleting}
            />
        </div>
    );
}

// Environment Variables Component
function EnvVarsList({ projectName, deploymentId }) {
    const [envVars, setEnvVars] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [editingEnvVar, setEditingEnvVar] = useState(null);
    const [formData, setFormData] = useState({ key: '', value: '', is_secret: false });
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [envVarToDelete, setEnvVarToDelete] = useState(null);
    const [deleting, setDeleting] = useState(false);
    const [saving, setSaving] = useState(false);
    const { showToast } = useToast();

    const loadEnvVars = useCallback(async () => {
        try {
            const response = deploymentId
                ? await api.getDeploymentEnvVars(projectName, deploymentId)
                : await api.getProjectEnvVars(projectName);
            setEnvVars(response.env_vars || []);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName, deploymentId]);

    useEffect(() => {
        loadEnvVars();
    }, [loadEnvVars]);

    const handleAddClick = () => {
        setEditingEnvVar(null);
        setFormData({ key: '', value: '', is_secret: false });
        setIsModalOpen(true);
    };

    const handleEditClick = (envVar) => {
        setEditingEnvVar(envVar);
        setFormData({ key: envVar.key, value: envVar.value, is_secret: envVar.is_secret });
        setIsModalOpen(true);
    };

    const handleDeleteClick = (envVar) => {
        setEnvVarToDelete(envVar);
        setConfirmDialogOpen(true);
    };

    const handleSave = async () => {
        if (!formData.key || !formData.value) {
            showToast('Key and value are required', 'error');
            return;
        }

        setSaving(true);
        try {
            await api.setEnvVar(projectName, formData.key, formData.value, formData.is_secret);
            showToast(`Environment variable ${formData.key} ${editingEnvVar ? 'updated' : 'created'} successfully`, 'success');
            setIsModalOpen(false);
            loadEnvVars();
        } catch (err) {
            showToast(`Failed to save environment variable: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    const handleDeleteConfirm = async () => {
        if (!envVarToDelete) return;

        setDeleting(true);
        try {
            await api.deleteEnvVar(projectName, envVarToDelete.key);
            showToast(`Environment variable ${envVarToDelete.key} deleted successfully`, 'success');
            setConfirmDialogOpen(false);
            setEnvVarToDelete(null);
            loadEnvVars();
        } catch (err) {
            showToast(`Failed to delete environment variable: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading environment variables: {error}</p>;

    return (
        <div>
            {!deploymentId && (
                <div className="mb-4 flex justify-end">
                    <Button variant="primary" size="sm" onClick={handleAddClick}>
                        Add Variable
                    </Button>
                </div>
            )}
            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Key</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Value</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Type</th>
                            {!deploymentId && (
                                <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Actions</th>
                            )}
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {envVars.length === 0 ? (
                            <tr>
                                <td colSpan={deploymentId ? "3" : "4"} className="px-6 py-8 text-center text-gray-400">
                                    No environment variables configured.
                                </td>
                            </tr>
                        ) : (
                            envVars.map(env => (
                            <tr key={env.key} className="hover:bg-gray-800/50 transition-colors">
                                <td className="px-6 py-4 text-sm font-mono text-gray-200">{env.key}</td>
                                <td className="px-6 py-4 text-sm font-mono text-gray-200">{env.value}</td>
                                <td className="px-6 py-4 text-sm">
                                    {env.is_secret ? (
                                        <span className="bg-yellow-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">secret</span>
                                    ) : (
                                        <span className="bg-gray-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">plain</span>
                                    )}
                                </td>
                                {!deploymentId && (
                                    <td className="px-6 py-4 text-sm">
                                        <div className="flex gap-2">
                                            <Button
                                                variant="secondary"
                                                size="sm"
                                                onClick={() => handleEditClick(env)}
                                            >
                                                Edit
                                            </Button>
                                            <Button
                                                variant="danger"
                                                size="sm"
                                                onClick={() => handleDeleteClick(env)}
                                            >
                                                Delete
                                            </Button>
                                        </div>
                                    </td>
                                )}
                            </tr>
                        ))
                        )}
                    </tbody>
                </table>
            </div>
            {deploymentId ? (
                <p className="mt-4 text-sm text-gray-500">
                    <strong>Note:</strong> Environment variables are read-only snapshots taken at deployment time.
                    Secret values are always masked for security.
                </p>
            ) : (
                <p className="mt-4 text-sm text-gray-500">
                    <strong>Note:</strong> Environment variables are snapshots at deployment time.
                    Changes to project variables will only apply to new deployments, not existing ones.
                    Secret values are always masked for security.
                </p>
            )}

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title={editingEnvVar ? 'Edit Environment Variable' : 'Add Environment Variable'}
            >
                <div className="space-y-4">
                    <FormField
                        label="Key"
                        id="env-key"
                        value={formData.key}
                        onChange={(e) => setFormData({ ...formData, key: e.target.value })}
                        placeholder="DATABASE_URL"
                        disabled={editingEnvVar !== null}
                        required
                    />
                    <FormField
                        label="Value"
                        id="env-value"
                        type="textarea"
                        value={formData.value}
                        onChange={(e) => setFormData({ ...formData, value: e.target.value })}
                        placeholder="postgres://..."
                        required
                        rows={3}
                    />
                    <FormField
                        label=""
                        id="env-is-secret"
                        type="checkbox"
                        value={formData.is_secret}
                        onChange={(e) => setFormData({ ...formData, is_secret: e.target.checked })}
                        placeholder="Mark as secret (value will be encrypted)"
                    />

                    <div className="flex justify-end gap-3 pt-4">
                        <Button
                            variant="secondary"
                            onClick={() => setIsModalOpen(false)}
                            disabled={saving}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={handleSave}
                            loading={saving}
                        >
                            {editingEnvVar ? 'Update' : 'Add'}
                        </Button>
                    </div>
                </div>
            </Modal>

            <ConfirmDialog
                isOpen={confirmDialogOpen}
                onClose={() => {
                    setConfirmDialogOpen(false);
                    setEnvVarToDelete(null);
                }}
                onConfirm={handleDeleteConfirm}
                title="Delete Environment Variable"
                message={`Are you sure you want to delete the environment variable "${envVarToDelete?.key}"? This action cannot be undone.`}
                confirmText="Delete Variable"
                variant="danger"
                loading={deleting}
            />
        </div>
    );
}

// Extensions Component
function ExtensionsList({ projectName }) {
    const [availableExtensions, setAvailableExtensions] = useState([]);
    const [enabledExtensions, setEnabledExtensions] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isConfigModalOpen, setIsConfigModalOpen] = useState(false);
    const [selectedExtension, setSelectedExtension] = useState(null);
    const [selectedExtensionData, setSelectedExtensionData] = useState(null);
    const [formData, setFormData] = useState({ spec: '{}' });
    const [deleting, setDeleting] = useState(false);
    const [saving, setSaving] = useState(false);
    const [editMode, setEditMode] = useState(false);
    const [modalTab, setModalTab] = useState('ui');
    const [uiSpec, setUiSpec] = useState({});
    const [deleteConfirmName, setDeleteConfirmName] = useState('');
    const { showToast } = useToast();

    const loadExtensions = useCallback(async () => {
        try {
            // Load both available types and enabled extensions in parallel
            const [typesResponse, enabledResponse] = await Promise.all([
                api.getExtensionTypes(),
                api.getProjectExtensions(projectName)
            ]);

            setAvailableExtensions(typesResponse.extension_types || []);
            setEnabledExtensions(enabledResponse.extensions || []);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName]);

    useEffect(() => {
        loadExtensions();

        // Auto-refresh every 5 seconds
        const interval = setInterval(loadExtensions, 5000);
        return () => clearInterval(interval);
    }, [loadExtensions]);

    // Handle UI spec changes - merge with JSON spec (upsert)
    const handleUiSpecChange = useCallback((newUiSpec) => {
        setUiSpec(newUiSpec);

        // Parse current JSON spec
        let currentSpec = {};
        try {
            currentSpec = JSON.parse(formData.spec);
        } catch (err) {
            // If JSON is invalid, start fresh
            currentSpec = {};
        }

        // Merge UI spec into current spec (upsert - UI values override, but preserve unknown fields)
        const mergedSpec = { ...currentSpec, ...newUiSpec };

        // Update JSON spec
        setFormData({ spec: JSON.stringify(mergedSpec, null, 2) });
    }, [formData.spec]);

    // Handle JSON spec changes - update UI spec
    const handleJsonSpecChange = useCallback((newJsonString) => {
        setFormData({ spec: newJsonString });

        // Try to parse and update UI spec
        try {
            const parsed = JSON.parse(newJsonString);
            setUiSpec(parsed);
        } catch (err) {
            // Invalid JSON, don't update UI spec
        }
    }, []);

    const handleSave = async () => {
        if (!selectedExtension) return;

        // Parse spec JSON
        let spec;
        try {
            spec = JSON.parse(formData.spec);
        } catch (err) {
            showToast('Invalid JSON in spec: ' + err.message, 'error');
            return;
        }

        setSaving(true);
        try {
            if (editMode) {
                await api.updateExtension(projectName, selectedExtension.name, spec);
                showToast(`Extension ${selectedExtension.name} updated successfully`, 'success');
            } else {
                await api.createExtension(projectName, selectedExtension.name, spec);
                showToast(`Extension ${selectedExtension.name} enabled successfully`, 'success');
            }
            setIsConfigModalOpen(false);
            loadExtensions();
        } catch (err) {
            showToast(`Failed to ${editMode ? 'update' : 'enable'} extension: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    const handleDelete = async () => {
        if (!selectedExtensionData || deleteConfirmName !== selectedExtensionData.extension) {
            showToast('Please enter the extension name to confirm deletion', 'error');
            return;
        }

        setDeleting(true);
        try {
            await api.deleteExtension(projectName, selectedExtensionData.extension);
            showToast(`Extension ${selectedExtensionData.extension} deleted successfully`, 'success');
            setIsConfigModalOpen(false);
            loadExtensions();
        } catch (err) {
            showToast(`Failed to delete extension: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    // Helper to check if an extension type is enabled
    const isEnabled = (extensionTypeName) => {
        return enabledExtensions.some(e => e.extension_type === extensionTypeName);
    };

    // Helper to get enabled extension data
    const getEnabledExtension = (extensionTypeName) => {
        return enabledExtensions.find(e => e.extension_type === extensionTypeName);
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading extensions: {error}</p>;

    return (
        <div className="space-y-6">
            {/* Available Extensions - Icon Buttons */}
            <div>
                <h3 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-3">
                    Available Extensions
                </h3>
                <div className="flex flex-wrap gap-3">
                    {availableExtensions.length === 0 ? (
                        <p className="text-gray-400 text-sm">No extensions available.</p>
                    ) : (
                        availableExtensions
                            .sort((a, b) => a.display_name.localeCompare(b.display_name))
                            .map(extType => {
                                const enabled = isEnabled(extType.extension_type);
                                const iconUrl = getExtensionIcon(extType.extension_type);

                                return (
                                    <button
                                        key={extType.extension_type}
                                        onClick={() => {
                                            window.location.hash = `#project/${projectName}/extensions/${extType.extension_type}`;
                                        }}
                                        className="group relative flex flex-col items-center justify-center w-32 h-32 bg-gray-800 hover:bg-gray-700 border border-gray-700 hover:border-indigo-500 rounded-lg transition-all"
                                        title={extType.description}
                                    >
                                        {/* Icon */}
                                        {iconUrl ? (
                                            <img
                                                src={iconUrl}
                                                alt={extType.display_name}
                                                className="w-12 h-12 rounded object-contain mb-2"
                                            />
                                        ) : (
                                            <div className="w-12 h-12 mb-2 flex items-center justify-center bg-gray-700 rounded text-gray-400 text-2xl font-bold">
                                                {extType.display_name.charAt(0).toUpperCase()}
                                            </div>
                                        )}
                                        {/* Name */}
                                        <span className="text-xs text-gray-300 text-center px-2 line-clamp-2">
                                            {extType.display_name}
                                        </span>
                                        {/* Enabled Badge */}
                                        {enabled && (
                                            <div className="absolute top-2 right-2">
                                                <div className="w-3 h-3 bg-green-500 rounded-full border-2 border-gray-800" title="Enabled"></div>
                                            </div>
                                        )}
                                    </button>
                                );
                            })
                    )}
                </div>
            </div>

            {/* Enabled Extensions - Table */}
            <div>
                <h3 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-3">
                    Enabled Extensions
                </h3>
                {enabledExtensions.length === 0 ? (
                    <div className="bg-gray-900 rounded-lg border border-gray-800 px-6 py-8 text-center">
                        <p className="text-gray-400 text-sm">
                            No extensions enabled yet. Click an extension above to get started.
                        </p>
                    </div>
                ) : (
                    <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                        <table className="w-full">
                            <thead className="bg-gray-800">
                                <tr>
                                    <th className="w-12 px-3 py-3"></th>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Extension</th>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Type</th>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Status</th>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Updated</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-800">
                                {enabledExtensions
                                    .sort((a, b) => a.extension.localeCompare(b.extension))
                                    .map(ext => {
                                        const extType = availableExtensions.find(t => t.extension_type === ext.extension_type);
                                        const iconUrl = getExtensionIcon(ext.extension_type);

                                        return (
                                            <tr
                                                key={ext.extension}
                                                className="hover:bg-gray-800/50 transition-colors cursor-pointer"
                                                onClick={() => {
                                                    window.location.hash = `#project/${projectName}/extensions/${ext.extension_type}`;
                                                }}
                                            >
                                                <td className="px-3 py-4">
                                                    {iconUrl ? (
                                                        <img
                                                            src={iconUrl}
                                                            alt={ext.extension}
                                                            className="w-8 h-8 rounded object-contain"
                                                        />
                                                    ) : (
                                                        <div className="w-8 h-8"></div>
                                                    )}
                                                </td>
                                                <td className="px-6 py-4 text-sm">
                                                    <span className="font-mono text-gray-200">{ext.extension}</span>
                                                </td>
                                                <td className="px-6 py-4 text-sm text-gray-300">
                                                    {extType?.description || ext.extension_type}
                                                </td>
                                                <td className="px-6 py-4 text-sm">
                                                    {renderExtensionStatusBadge(ext)}
                                                </td>
                                                <td className="px-6 py-4 text-sm text-gray-400">
                                                    {formatDate(ext.updated)}
                                                </td>
                                            </tr>
                                        );
                                    })}
                            </tbody>
                        </table>
                    </div>
                )}
            </div>

            {/* Configuration Modal */}
            <Modal
                isOpen={isConfigModalOpen}
                onClose={() => setIsConfigModalOpen(false)}
                title={editMode ? `Extension: ${selectedExtension?.name}` : `Enable Extension: ${selectedExtension?.name}`}
                maxWidth="max-w-4xl"
            >
                <div className="space-y-4">
                    {selectedExtension && (
                        <>
                            {/* Tab Navigation */}
                            <div className="border-b border-gray-700">
                                <div className="flex gap-6">
                                    {hasExtensionUI(selectedExtension.extension_type) && (
                                        <button
                                            className={`pb-3 px-2 border-b-2 transition-colors ${modalTab === 'ui' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                            onClick={() => setModalTab('ui')}
                                        >
                                            Configure
                                        </button>
                                    )}
                                    <button
                                        className={`pb-3 px-2 border-b-2 transition-colors ${modalTab === 'config' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                        onClick={() => setModalTab('config')}
                                    >
                                        {hasExtensionUI(selectedExtension.extension_type) ? 'JSON' : 'Configuration'}
                                    </button>
                                    <button
                                        className={`pb-3 px-2 border-b-2 transition-colors ${modalTab === 'schema' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                        onClick={() => setModalTab('schema')}
                                    >
                                        Schema
                                    </button>
                                    <button
                                        className={`pb-3 px-2 border-b-2 transition-colors ${modalTab === 'docs' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                        onClick={() => setModalTab('docs')}
                                    >
                                        Documentation
                                    </button>
                                    {editMode && selectedExtensionData && (
                                        <>
                                            <button
                                                className={`pb-3 px-2 border-b-2 transition-colors ${modalTab === 'status' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                                onClick={() => setModalTab('status')}
                                            >
                                                Status
                                            </button>
                                            <button
                                                className={`pb-3 px-2 border-b-2 transition-colors ${modalTab === 'delete' ? 'border-red-500 text-red-400' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                                onClick={() => setModalTab('delete')}
                                            >
                                                Delete
                                            </button>
                                        </>
                                    )}
                                </div>
                            </div>

                            {/* Tab Content */}
                            {modalTab === 'ui' && hasExtensionUI(selectedExtension.extension_type) && (
                                <div className="space-y-4">
                                    {React.createElement(getExtensionUI(selectedExtension.extension_type), {
                                        spec: uiSpec,
                                        schema: selectedExtension.spec_schema,
                                        onChange: handleUiSpecChange
                                    })}
                                </div>
                            )}

                            {modalTab === 'config' && (
                                <div className="space-y-4">
                                    <FormField
                                        label="Configuration Spec (JSON)"
                                        id="extension-spec"
                                        type="textarea"
                                        value={formData.spec}
                                        onChange={(e) => handleJsonSpecChange(e.target.value)}
                                        placeholder="{}"
                                        required
                                        rows={15}
                                    />
                                    <p className="text-sm text-gray-500">
                                        Enter the extension configuration as a JSON object. See the Schema and Documentation tabs for valid fields and examples.
                                        {hasExtensionUI(selectedExtension.extension_type) && <span> Use the Configure tab for a form-based interface.</span>}
                                    </p>
                                </div>
                            )}

                            {modalTab === 'schema' && (
                                <div className="space-y-4">
                                    <h4 className="text-sm font-semibold text-gray-300">Schema</h4>
                                    <pre className="text-xs font-mono text-gray-300 bg-gray-800 p-4 rounded overflow-x-auto max-h-96">
                                        {JSON.stringify(selectedExtension.spec_schema, null, 2)}
                                    </pre>
                                    <p className="text-sm text-gray-500">
                                        This JSON schema defines the valid structure for the extension configuration.
                                    </p>
                                </div>
                            )}

                            {modalTab === 'docs' && (
                                <div className="space-y-4">
                                    <h4 className="text-sm font-semibold text-gray-300">Documentation</h4>
                                    <div
                                        className="prose prose-sm prose-invert max-w-none bg-gray-800 p-4 rounded max-h-96 overflow-y-auto"
                                        dangerouslySetInnerHTML={{
                                            __html: marked.parse(selectedExtension.documentation)
                                        }}
                                    />
                                </div>
                            )}

                            {modalTab === 'status' && editMode && selectedExtensionData && (
                                <div className="space-y-4">
                                    <div>
                                        <h4 className="text-sm font-semibold text-gray-300 mb-2">Status Summary</h4>
                                        <p className="text-gray-200">{selectedExtensionData.status_summary}</p>
                                    </div>

                                    <div>
                                        <h4 className="text-sm font-semibold text-gray-300 mb-2">Current Spec</h4>
                                        <pre className="text-xs font-mono text-gray-300 bg-gray-800 p-3 rounded overflow-x-auto">
                                            {JSON.stringify(selectedExtensionData.spec, null, 2)}
                                        </pre>
                                    </div>

                                    <div>
                                        <h4 className="text-sm font-semibold text-gray-300 mb-2">Full Status</h4>
                                        <pre className="text-xs font-mono text-gray-300 bg-gray-800 p-3 rounded overflow-x-auto max-h-96">
                                            {JSON.stringify(selectedExtensionData.status, null, 2)}
                                        </pre>
                                    </div>

                                    <div className="text-xs text-gray-500">
                                        <p>Created: {formatDate(selectedExtensionData.created)}</p>
                                        <p>Updated: {formatDate(selectedExtensionData.updated)}</p>
                                    </div>
                                </div>
                            )}

                            {modalTab === 'delete' && editMode && selectedExtensionData && (
                                <div className="space-y-4">
                                    <div className="bg-red-900/20 border border-red-700 rounded-lg p-4">
                                        <h4 className="text-sm font-semibold text-red-300 mb-2">Warning: Permanent Deletion</h4>
                                        <p className="text-sm text-red-200">
                                            Deleting this extension will deprovision all resources created by this extension.
                                            This action cannot be undone.
                                        </p>
                                    </div>

                                    <FormField
                                        label={`Type "${selectedExtensionData.extension}" to confirm deletion`}
                                        id="delete-confirm-name"
                                        value={deleteConfirmName}
                                        onChange={(e) => setDeleteConfirmName(e.target.value)}
                                        placeholder={selectedExtensionData.extension}
                                        required
                                    />

                                    <div className="flex justify-end gap-3 pt-4">
                                        <Button
                                            variant="secondary"
                                            onClick={() => setModalTab(hasExtensionUI(selectedExtension.extension_type) ? 'ui' : 'config')}
                                            disabled={deleting}
                                        >
                                            Cancel
                                        </Button>
                                        <Button
                                            variant="danger"
                                            onClick={handleDelete}
                                            loading={deleting}
                                            disabled={deleteConfirmName !== selectedExtensionData.extension}
                                        >
                                            Delete Extension
                                        </Button>
                                    </div>
                                </div>
                            )}

                            {/* Action buttons - only shown when not on delete tab */}
                            {modalTab !== 'delete' && (
                                <div className="flex justify-end gap-3 pt-4">
                                    <Button
                                        variant="secondary"
                                        onClick={() => setIsConfigModalOpen(false)}
                                        disabled={saving}
                                    >
                                        Cancel
                                    </Button>
                                    <Button
                                        variant="primary"
                                        onClick={handleSave}
                                        loading={saving}
                                    >
                                        {editMode ? 'Update' : 'Enable'}
                                    </Button>
                                </div>
                            )}
                        </>
                    )}
                </div>
            </Modal>

        </div>
    );
}

// Helper function to render status badges with color coding
function renderExtensionStatusBadge(extension) {
    // Check if extension has custom status badge renderer
    const customRenderer = getExtensionStatusBadge(extension.extension_type);
    if (customRenderer) {
        const customBadge = customRenderer(extension);
        if (customBadge) return customBadge;
    }

    // Fallback to generic status badge using status_summary
    let badgeColor = 'bg-gray-600';  // Default
    let statusText = extension.status_summary || 'Unknown';

    // Parse status JSON to determine color and text
    if (extension.status && extension.status.state) {
        const state = extension.status.state.toLowerCase();
        statusText = extension.status.state; // Use the original state value as the text

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
    }

    return (
        <span className={`${badgeColor} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase`}>
            {statusText}
        </span>
    );
}

// Generic Extension Detail View (fallback for extensions without custom UI)
function GenericExtensionDetailView({ extension }) {
    return (
        <div className="space-y-6">
            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Status</h2>
                <div className="bg-gray-900 rounded p-4">
                    <p className="text-gray-300">{extension.status_summary}</p>

                    {extension.status && extension.status.error && (
                        <div className="mt-3 p-3 bg-red-900/20 border border-red-700 rounded">
                            <p className="text-sm text-red-300">
                                <strong>Error:</strong> {extension.status.error}
                            </p>
                        </div>
                    )}
                </div>
            </section>

            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Configuration</h2>
                <pre className="bg-gray-900 rounded p-4 overflow-x-auto">
                    <code className="text-sm text-gray-300">
                        {JSON.stringify(extension.spec, null, 2)}
                    </code>
                </pre>
            </section>

            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Full Status</h2>
                <pre className="bg-gray-900 rounded p-4 overflow-x-auto">
                    <code className="text-sm text-gray-300">
                        {JSON.stringify(extension.status, null, 2)}
                    </code>
                </pre>
            </section>

            <section>
                <h2 className="text-lg font-semibold text-gray-200 mb-3">Metadata</h2>
                <div className="bg-gray-900 rounded p-4 space-y-2">
                    <p className="text-sm text-gray-300">
                        <span className="text-gray-500">Created:</span> {formatDate(extension.created)}
                    </p>
                    <p className="text-sm text-gray-300">
                        <span className="text-gray-500">Updated:</span> {formatDate(extension.updated)}
                    </p>
                </div>
            </section>
        </div>
    );
}

// Extension Detail Page Component
function ExtensionDetailPage({ projectName, extensionName }) {
    const [extensionType, setExtensionType] = useState(null);
    const [enabledExtension, setEnabledExtension] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [activeTab, setActiveTab] = useState('overview');
    const [formData, setFormData] = useState({ spec: '{}' });
    const [uiSpec, setUiSpec] = useState({});
    const [saving, setSaving] = useState(false);
    const [deleting, setDeleting] = useState(false);
    const [deleteConfirmName, setDeleteConfirmName] = useState('');
    const [originalSpec, setOriginalSpec] = useState('{}');
    const { showToast } = useToast();

    const isEnabled = enabledExtension !== null;

    // Check if there are unsaved changes (normalize JSON to ignore key order)
    const hasUnsavedChanges = normalizeJSON(formData.spec) !== normalizeJSON(originalSpec);

    // Memoize the extension UI API
    const extensionAPI = React.useMemo(() => {
        return extensionType ? getExtensionUIAPI(extensionType.extension_type) : null;
    }, [extensionType]);

    useEffect(() => {
        async function fetchData() {
            try {
                // Fetch both extension type info and enabled extension data
                const [typesResponse, enabledResponse] = await Promise.all([
                    api.getExtensionTypes(),
                    api.getProjectExtensions(projectName)
                ]);

                const extType = typesResponse.extension_types.find(t => t.extension_type === extensionName);
                if (!extType) {
                    setError('Extension type not found');
                    setLoading(false);
                    return;
                }
                setExtensionType(extType);

                const enabled = enabledResponse.extensions.find(e => e.extension_type === extensionName);
                setEnabledExtension(enabled || null);

                // Set form data only on initial load
                if (loading) {
                    if (enabled) {
                        const specJson = JSON.stringify(enabled.spec, null, 2);
                        setFormData({ spec: specJson });
                        setOriginalSpec(specJson);
                        setUiSpec(enabled.spec);
                        setActiveTab('overview');
                    } else {
                        const defaultSpec = {};
                        const specJson = JSON.stringify(defaultSpec, null, 2);
                        setFormData({ spec: specJson });
                        setOriginalSpec(specJson);
                        setUiSpec(defaultSpec);
                        setActiveTab(hasExtensionUI(extType.extension_type) ? 'configure' : 'config');
                    }
                }
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        fetchData();
    }, [projectName, extensionName]);

    // Handle UI spec changes
    const handleUiSpecChange = useCallback((newUiSpec) => {
        setUiSpec(newUiSpec);
        setFormData({ spec: JSON.stringify(newUiSpec, null, 2) });
    }, []);

    // Handle JSON spec changes
    const handleJsonSpecChange = useCallback((newJsonString) => {
        setFormData({ spec: newJsonString });
        try {
            const parsed = JSON.parse(newJsonString);
            setUiSpec(parsed);
        } catch (err) {
            // Invalid JSON, don't update UI spec
        }
    }, []);

    const handleSave = async () => {
        let spec;
        try {
            spec = JSON.parse(formData.spec);
        } catch (err) {
            showToast('Invalid JSON in spec: ' + err.message, 'error');
            return;
        }

        const wasEnabled = isEnabled;
        setSaving(true);
        try {
            if (isEnabled) {
                // Update existing extension using its instance name
                await api.updateExtension(projectName, enabledExtension.extension, spec);
                showToast(`Extension ${extensionType.display_name} updated successfully`, 'success');
            } else {
                // Create new extension - use extension_type as default instance name
                await api.createExtension(projectName, extensionName, extensionName, spec);
                showToast(`Extension ${extensionType.display_name} enabled successfully`, 'success');
            }
            // Refresh data
            const enabledResponse = await api.getProjectExtensions(projectName);
            const enabled = enabledResponse.extensions.find(e => e.extension_type === extensionName);
            setEnabledExtension(enabled || null);
            if (enabled) {
                const specJson = JSON.stringify(enabled.spec, null, 2);
                setFormData({ spec: specJson });
                setOriginalSpec(specJson);
                setUiSpec(enabled.spec);
                // Only switch to overview tab when first enabling, not when updating
                if (!wasEnabled) {
                    setActiveTab('overview');
                }
            }
        } catch (err) {
            showToast(`Failed to ${isEnabled ? 'update' : 'enable'} extension: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    const handleDelete = async () => {
        if (!enabledExtension || deleteConfirmName !== enabledExtension.extension) {
            showToast('Please enter the extension name to confirm deletion', 'error');
            return;
        }

        setDeleting(true);
        try {
            await api.deleteExtension(projectName, enabledExtension.extension);
            showToast(`Extension ${enabledExtension.extension} deleted successfully`, 'success');
            window.location.hash = `#project/${projectName}/extensions`;
        } catch (err) {
            showToast(`Failed to delete extension: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    if (loading) {
        return (
            <div className="flex items-center justify-center min-h-[400px]">
                <div className="w-12 h-12 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
            </div>
        );
    }

    if (error || !extensionType) {
        return (
            <div className="max-w-7xl mx-auto">
                <div className="mb-6">
                    <button
                        onClick={() => window.location.hash = `#project/${projectName}/extensions`}
                        className="text-indigo-400 hover:text-indigo-300 flex items-center gap-2"
                    >
                        <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M19 12H5M5 12L12 19M5 12L12 5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                        </svg>
                        Back to Extensions
                    </button>
                </div>
                <div className="bg-red-900/20 border border-red-700 rounded-lg p-6">
                    <h2 className="text-lg font-semibold text-red-300 mb-2">Error</h2>
                    <p className="text-red-200">{error || 'Extension type not found'}</p>
                </div>
            </div>
        );
    }

    const CustomDetailView = getExtensionDetailView(extensionType.extension_type);

    return (
        <div className="max-w-7xl mx-auto">
            <div className="mb-6 flex items-center justify-between">
                <button
                    onClick={() => window.location.hash = `#project/${projectName}/extensions`}
                    className="text-indigo-400 hover:text-indigo-300 flex items-center gap-2"
                >
                    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M19 12H5M5 12L12 19M5 12L12 5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                    </svg>
                    Back to Extensions
                </button>
                <div className="flex items-center gap-2 text-sm text-gray-400">
                    <a href={`#project/${projectName}`} className="hover:text-indigo-400 transition-colors">
                        {projectName}
                    </a>
                    <span>/</span>
                    <span>Extensions</span>
                </div>
            </div>

            <div className="bg-gray-800 rounded-lg shadow-xl p-6">
                <div className="flex items-center justify-between mb-6">
                    <div className="flex items-center space-x-4">
                        {getExtensionIcon(extensionType.extension_type) && (
                            <img
                                src={getExtensionIcon(extensionType.extension_type)}
                                alt={extensionType.display_name}
                                className="w-12 h-12 rounded object-contain"
                            />
                        )}
                        <div>
                            <h1 className="text-2xl font-bold text-white">
                                {extensionType.display_name}
                            </h1>
                            <p className="text-gray-400">{extensionType.description}</p>
                        </div>
                    </div>
                    <div className="flex items-center gap-3">
                        {isEnabled ? (
                            renderExtensionStatusBadge(enabledExtension)
                        ) : (
                            <span className="bg-gray-600 text-white text-xs font-semibold px-3 py-1 rounded-full uppercase">
                                Not Enabled
                            </span>
                        )}
                    </div>
                </div>

                {/* Tab Navigation */}
                <div className="border-b border-gray-700 mb-6">
                    <div className="flex gap-6">
                        {/* Left-aligned: Extension-specific tabs */}
                        {isEnabled && (
                            <button
                                className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'overview' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                onClick={() => setActiveTab('overview')}
                            >
                                Overview
                            </button>
                        )}
                        {hasExtensionUI(extensionType.extension_type) && (
                            <button
                                className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'configure' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                onClick={() => setActiveTab('configure')}
                            >
                                Configure{hasUnsavedChanges && ' *'}
                            </button>
                        )}

                        {/* Spacer to push common tabs to the right */}
                        <div className="flex-1"></div>

                        {/* Right-aligned: Common tabs */}
                        <button
                            className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'config' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                            onClick={() => setActiveTab('config')}
                        >
                            Spec{hasUnsavedChanges && ' *'}
                        </button>
                        {isEnabled && (
                            <button
                                className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'status' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                onClick={() => setActiveTab('status')}
                            >
                                Status
                            </button>
                        )}
                        <button
                            className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'schema' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                            onClick={() => setActiveTab('schema')}
                        >
                            Schema
                        </button>
                        <button
                            className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'docs' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                            onClick={() => setActiveTab('docs')}
                        >
                            Documentation
                        </button>
                        {isEnabled && (
                            <button
                                className={`pb-3 px-2 border-b-2 transition-colors ${activeTab === 'delete' ? 'border-red-500 text-red-400' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                                onClick={() => setActiveTab('delete')}
                            >
                                Delete
                            </button>
                        )}
                    </div>
                </div>

                {/* Tab Content */}
                {activeTab === 'overview' && isEnabled && CustomDetailView && (
                    <CustomDetailView extension={enabledExtension} />
                )}

                {activeTab === 'overview' && isEnabled && !CustomDetailView && (
                    <GenericExtensionDetailView extension={enabledExtension} />
                )}

                {activeTab === 'configure' && extensionAPI?.renderConfigureTab && (
                    <div className="space-y-4">
                        {extensionAPI.renderConfigureTab(uiSpec, extensionType.spec_schema, handleUiSpecChange)}
                        <div className="flex justify-end gap-3 pt-4 border-t border-gray-700">
                            <Button
                                variant="primary"
                                onClick={handleSave}
                                loading={saving}
                            >
                                {isEnabled ? 'Update' : 'Enable'}
                            </Button>
                        </div>
                    </div>
                )}

                {activeTab === 'config' && (
                    <div className="space-y-4">
                        <FormField
                            label="Configuration Spec (JSON)"
                            id="extension-spec"
                            type="textarea"
                            value={formData.spec}
                            onChange={(e) => handleJsonSpecChange(e.target.value)}
                            placeholder="{}"
                            required
                            rows={15}
                        />
                        <p className="text-sm text-gray-500">
                            Enter the extension configuration as a JSON object. See the Schema and Documentation tabs for valid fields and examples.
                            {hasExtensionUI(extensionType.extension_type) && <span> Use the Configure tab for a form-based interface.</span>}
                        </p>
                        <div className="flex justify-end gap-3 pt-4 border-t border-gray-700">
                            <Button
                                variant="primary"
                                onClick={handleSave}
                                loading={saving}
                            >
                                {isEnabled ? 'Update' : 'Enable'}
                            </Button>
                        </div>
                    </div>
                )}

                {activeTab === 'status' && isEnabled && (
                    <div className="space-y-4">
                        <div>
                            <h4 className="text-sm font-semibold text-gray-300 mb-2">Status Summary</h4>
                            <p className="text-gray-200">{enabledExtension.status_summary}</p>
                        </div>

                        <div>
                            <h4 className="text-sm font-semibold text-gray-300 mb-2">Current Spec</h4>
                            <pre className="text-xs font-mono text-gray-300 bg-gray-900 p-3 rounded overflow-x-auto">
                                {JSON.stringify(enabledExtension.spec, null, 2)}
                            </pre>
                        </div>

                        <div>
                            <h4 className="text-sm font-semibold text-gray-300 mb-2">Full Status</h4>
                            <pre className="text-xs font-mono text-gray-300 bg-gray-900 p-3 rounded overflow-x-auto max-h-96">
                                {JSON.stringify(enabledExtension.status, null, 2)}
                            </pre>
                        </div>

                        <div className="text-xs text-gray-500">
                            <p>Created: {formatDate(enabledExtension.created)}</p>
                            <p>Updated: {formatDate(enabledExtension.updated)}</p>
                        </div>
                    </div>
                )}

                {activeTab === 'schema' && (
                    <div className="space-y-4">
                        <h4 className="text-sm font-semibold text-gray-300">Schema</h4>
                        <pre className="text-xs font-mono text-gray-300 bg-gray-900 p-4 rounded overflow-x-auto max-h-96">
                            {JSON.stringify(extensionType.spec_schema, null, 2)}
                        </pre>
                        <p className="text-sm text-gray-500">
                            This JSON schema defines the valid structure for the extension configuration.
                        </p>
                    </div>
                )}

                {activeTab === 'docs' && (
                    <div
                        className="prose prose-invert max-w-none"
                        dangerouslySetInnerHTML={{
                            __html: marked.parse(extensionType.documentation)
                        }}
                    />
                )}

                {activeTab === 'delete' && isEnabled && (
                    <div className="space-y-4">
                        <div className="bg-red-900/20 border border-red-700 rounded-lg p-4">
                            <h4 className="text-sm font-semibold text-red-300 mb-2">Warning: Permanent Deletion</h4>
                            <p className="text-sm text-red-200">
                                Deleting this extension will deprovision all resources created by this extension.
                                This action cannot be undone.
                            </p>
                        </div>

                        <FormField
                            label={`Type "${enabledExtension.extension}" to confirm deletion`}
                            id="delete-confirm-name"
                            value={deleteConfirmName}
                            onChange={(e) => setDeleteConfirmName(e.target.value)}
                            placeholder={enabledExtension.extension}
                            required
                        />

                        <div className="flex justify-end gap-3 pt-4 border-t border-gray-700">
                            <Button
                                variant="secondary"
                                onClick={() => setActiveTab('overview')}
                                disabled={deleting}
                            >
                                Cancel
                            </Button>
                            <Button
                                variant="danger"
                                onClick={handleDelete}
                                loading={deleting}
                                disabled={deleteConfirmName !== enabledExtension.extension}
                            >
                                Delete Extension
                            </Button>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
}
