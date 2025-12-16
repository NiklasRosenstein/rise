// Resource management components for Rise Dashboard (Service Accounts, Domains, Environment Variables)
// This file depends on React, utils.js, components/ui.js, and components/toast.js being loaded first

const { useState, useEffect, useCallback } = React;

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
