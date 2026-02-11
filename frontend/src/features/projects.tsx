// @ts-nocheck
import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { navigate } from '../lib/navigation';
import { formatDate } from '../lib/utils';
import { useToast } from '../components/toast';
import { Button, ConfirmDialog, FormField, Modal, StatusBadge } from '../components/ui';
import { ActiveDeploymentsSummary, DeploymentDetail, DeploymentsList } from './deployments';
import { DomainsList, EnvVarsList, ExtensionDetailPage, ExtensionsList, ServiceAccountsList } from './resources';


// Access Class Badge Component
function AccessClassBadge({ accessClass }) {
    return (
        <span className="text-sm text-gray-700 dark:text-gray-300">
            {accessClass}
        </span>
    );
}

// Projects List Component
export function ProjectsList() {
    const [projects, setProjects] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [formData, setFormData] = useState({ name: '', access_class: 'public', owner: 'self' });
    const [teams, setTeams] = useState([]);
    const [currentUser, setCurrentUser] = useState(null);
    const [accessClasses, setAccessClasses] = useState([]);
    const [saving, setSaving] = useState(false);
    const { showToast } = useToast();

    const loadProjects = useCallback(async () => {
        try {
            const data = await api.getProjects();
            setProjects(data);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadProjects();
    }, [loadProjects]);

    useEffect(() => {
        async function loadTeams() {
            try {
                const data = await api.getTeams();
                setTeams(data);
            } catch (err) {
                console.error('Failed to load teams:', err);
            }
        }
        loadTeams();
    }, []);

    useEffect(() => {
        async function loadCurrentUser() {
            try {
                const user = await api.getMe();
                setCurrentUser(user);
            } catch (err) {
                console.error('Failed to load current user:', err);
            }
        }
        loadCurrentUser();
    }, []);

    useEffect(() => {
        async function loadAccessClasses() {
            try {
                const data = await api.getAccessClasses();
                setAccessClasses(data.access_classes || []);
            } catch (err) {
                console.error('Failed to load access classes:', err);
            }
        }
        loadAccessClasses();
    }, []);

    const handleCreateClick = () => {
        setFormData({ name: '', access_class: 'public', owner: 'self' });
        setIsModalOpen(true);
    };

    const handleCreate = async () => {
        if (!formData.name) {
            showToast('Project name is required', 'error');
            return;
        }

        // Validate project name (lowercase alphanumeric and hyphens)
        if (!/^[a-z0-9-]+$/.test(formData.name)) {
            showToast('Project name must contain only lowercase letters, numbers, and hyphens', 'error');
            return;
        }

        if (!currentUser) {
            showToast('Unable to determine current user', 'error');
            return;
        }

        setSaving(true);
        try {
            // Format owner correctly for the API
            let owner;
            if (formData.owner === 'self') {
                owner = { user: currentUser.id };
            } else {
                // formData.owner is the team ID
                owner = { team: formData.owner };
            }

            await api.createProject(formData.name, formData.access_class, owner);
            showToast(`Project ${formData.name} created successfully`, 'success');
            setIsModalOpen(false);
            loadProjects();
        } catch (err) {
            showToast(`Failed to create project: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-600 dark:text-red-400">Error loading projects: {error}</p>;

    return (
        <section>
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-2xl font-bold">Projects</h2>
                <Button variant="primary" size="sm" onClick={handleCreateClick}>
                    Create Project
                </Button>
            </div>
            <div className="bg-white dark:bg-gray-900 rounded-lg overflow-hidden border border-gray-200 dark:border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-100 dark:bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Name</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Status</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Owner</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">Access Class</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-700 dark:text-gray-300 uppercase tracking-wider">URL</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-200 dark:divide-gray-800">
                        {projects.length === 0 ? (
                            <tr>
                                <td colSpan="5" className="px-6 py-8 text-center text-gray-600 dark:text-gray-400">
                                    No projects found.
                                </td>
                            </tr>
                        ) : (
                            projects.map(p => {
                            const owner = p.owner_user_email ? `user:${p.owner_user_email}` :
                                         p.owner_team_name ? `team:${p.owner_team_name}` : '-';
                            return (
                                <tr
                                    key={p.id}
                                    onClick={() => navigate(`/project/${p.name}`)}
                                    className="hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors cursor-pointer"
                                >
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100">{p.name}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm"><StatusBadge status={p.status} /></td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{owner}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm"><AccessClassBadge accessClass={p.access_class} /></td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm">
                                        {p.primary_url ? (
                                            <a
                                                href={p.primary_url}
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300"
                                                onClick={(e) => e.stopPropagation()}
                                            >
                                                {p.primary_url}
                                            </a>
                                        ) : '-'}
                                    </td>
                                </tr>
                            );
                        })
                        )}
                    </tbody>
                </table>
            </div>

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title="Create Project"
            >
                <div className="space-y-4">
                    <FormField
                        label="Project Name"
                        id="project-name"
                        value={formData.name}
                        onChange={(e) => setFormData({ ...formData, name: e.target.value.toLowerCase() })}
                        placeholder="my-awesome-app"
                        required
                    />
                    <p className="text-sm text-gray-600 dark:text-gray-500 -mt-2">
                        Only lowercase letters, numbers, and hyphens allowed
                    </p>

                    <FormField
                        label="Access Class"
                        id="project-access-class"
                        type="select"
                        value={formData.access_class}
                        onChange={(e) => setFormData({ ...formData, access_class: e.target.value })}
                        required
                    >
                        {accessClasses.map(ac => (
                            <option key={ac.id} value={ac.id} title={ac.description}>
                                {ac.display_name}
                            </option>
                        ))}
                    </FormField>
                    {accessClasses.find(ac => ac.id === formData.access_class) && (
                        <p className="text-sm text-gray-600 dark:text-gray-500 -mt-2">
                            {accessClasses.find(ac => ac.id === formData.access_class).description}
                        </p>
                    )}

                    <FormField
                        label="Owner"
                        id="project-owner"
                        type="select"
                        value={formData.owner}
                        onChange={(e) => setFormData({ ...formData, owner: e.target.value })}
                        required
                    >
                        <option value="self">Self</option>
                        {teams.map(team => (
                            <option key={team.id} value={team.id}>team:{team.name}</option>
                        ))}
                    </FormField>

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
                            onClick={handleCreate}
                            loading={saving}
                        >
                            Create
                        </Button>
                    </div>
                </div>
            </Modal>
        </section>
    );
}

// Project Detail Component
export function ProjectDetail({ projectName, initialTab }) {
    const [project, setProject] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [activeTab, setActiveTab] = useState(initialTab || 'overview');
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [deleting, setDeleting] = useState(false);
    const [editingAccessClass, setEditingAccessClass] = useState(false);
    const [newAccessClass, setNewAccessClass] = useState(null);
    const [updatingAccessClass, setUpdatingAccessClass] = useState(false);
    const [accessClasses, setAccessClasses] = useState([]);
    const { showToast } = useToast();

    useEffect(() => {
        async function loadProject() {
            try {
                const data = await api.getProject(projectName, { expand: 'owner' });
                setProject(data);
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        loadProject();
    }, [projectName]);

    useEffect(() => {
        async function loadAccessClasses() {
            try {
                const data = await api.getAccessClasses();
                setAccessClasses(data.access_classes || []);
            } catch (err) {
                console.error('Failed to load access classes:', err);
            }
        }
        loadAccessClasses();
    }, []);

    // Update activeTab when initialTab changes (e.g., browser back/forward)
    useEffect(() => {
        if (initialTab) {
            setActiveTab(initialTab);
        }
    }, [initialTab]);

    // Helper function to change tab and update URL
    const changeTab = (tab) => {
        setActiveTab(tab);
        navigate(`/project/${projectName}/${tab}`);
    };

    const handleDeleteClick = () => {
        setConfirmDialogOpen(true);
    };

    const handleDeleteConfirm = async () => {
        if (!project) return;

        setDeleting(true);
        try {
            await api.deleteProject(project.name);
            showToast(`Project ${project.name} deleted successfully`, 'success');
            setConfirmDialogOpen(false);
            // Redirect to projects list
            navigate('/projects');
        } catch (err) {
            showToast(`Failed to delete project: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    const handleEditAccessClass = () => {
        setNewAccessClass(project.access_class);
        setEditingAccessClass(true);
    };

    const handleCancelEditAccessClass = () => {
        setEditingAccessClass(false);
        setNewAccessClass(null);
    };

    const handleSaveAccessClass = async () => {
        if (!project || !newAccessClass || newAccessClass === project.access_class) {
            setEditingAccessClass(false);
            return;
        }

        setUpdatingAccessClass(true);
        try {
            await api.updateProject(project.name, { access_class: newAccessClass });
            const ac = accessClasses.find(a => a.id === newAccessClass);
            showToast(`Project access class updated to ${ac ? ac.display_name : newAccessClass}`, 'success');
            // Reload project to get updated data
            const updatedProject = await api.getProject(projectName, { expand: 'owner' });
            setProject(updatedProject);
            setEditingAccessClass(false);
        } catch (err) {
            showToast(`Failed to update access class: ${err.message}`, 'error');
        } finally {
            setUpdatingAccessClass(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-600 dark:text-red-400">Error loading project: {error}</p>;
    if (!project) return <p className="text-gray-600 dark:text-gray-400">Project not found.</p>;

    return (
        <section>
            <a href="/projects" onClick={(e) => { e.preventDefault(); navigate('/projects'); }} className="inline-flex items-center gap-2 text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 mb-6 transition-colors">
                ← Back to Projects
            </a>

            <div className="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-lg p-6 mb-6">
                <div className="flex justify-between items-start mb-4">
                    <h3 className="text-2xl font-bold">Project {project.name}</h3>
                    <Button
                        variant="danger"
                        size="sm"
                        onClick={handleDeleteClick}
                    >
                        Delete Project
                    </Button>
                </div>
                <dl className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                        <dt className="text-gray-600 dark:text-gray-400">Status</dt>
                        <dd className="mt-1"><StatusBadge status={project.status} /></dd>
                    </div>
                    <div>
                        <dt className="text-gray-600 dark:text-gray-400 mb-1">Access Class</dt>
                        <dd className="mt-1">
                            {!editingAccessClass ? (
                                <div className="flex flex-col gap-1">
                                    <div className="flex items-center gap-2">
                                        <AccessClassBadge accessClass={project.access_class} />
                                        <button
                                            onClick={handleEditAccessClass}
                                            className="text-xs text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 transition-colors"
                                        >
                                            Edit
                                        </button>
                                    </div>
                                    {accessClasses.find(ac => ac.id === project.access_class) && (
                                        <p className="text-xs text-gray-600 dark:text-gray-500">
                                            {accessClasses.find(ac => ac.id === project.access_class).description}
                                        </p>
                                    )}
                                </div>
                            ) : (
                                <div className="flex flex-col gap-2">
                                    <div className="flex items-center gap-2">
                                        <select
                                            value={newAccessClass}
                                            onChange={(e) => setNewAccessClass(e.target.value)}
                                            className="bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-700 text-gray-900 dark:text-gray-100 rounded px-2 py-1 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                            disabled={updatingAccessClass}
                                        >
                                            {accessClasses.map(ac => (
                                                <option key={ac.id} value={ac.id}>
                                                    {ac.display_name}
                                                </option>
                                            ))}
                                        </select>
                                        <Button
                                            variant="primary"
                                            size="sm"
                                            onClick={handleSaveAccessClass}
                                            loading={updatingAccessClass}
                                            className="!py-1 !px-2 !text-xs"
                                        >
                                            Save
                                        </Button>
                                        <Button
                                            variant="secondary"
                                            size="sm"
                                            onClick={handleCancelEditAccessClass}
                                            disabled={updatingAccessClass}
                                            className="!py-1 !px-2 !text-xs"
                                        >
                                            Cancel
                                        </Button>
                                    </div>
                                    {accessClasses.find(ac => ac.id === newAccessClass) && (
                                        <p className="text-xs text-gray-600 dark:text-gray-500">
                                            {accessClasses.find(ac => ac.id === newAccessClass).description}
                                        </p>
                                    )}
                                </div>
                            )}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-gray-600 dark:text-gray-400">URLs</dt>
                        <dd className="mt-1 space-y-1">
                            {project.primary_url ? (
                                <>
                                    <div>
                                        <a href={project.primary_url} target="_blank" rel="noopener noreferrer" className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300">{project.primary_url}</a>
                                    </div>
                                    {project.custom_domain_urls && project.custom_domain_urls.map((url, idx) => (
                                        <div key={idx}>
                                            <a href={url} target="_blank" rel="noopener noreferrer" className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300">{url}</a>
                                        </div>
                                    ))}
                                </>
                            ) : (
                                <span className="text-gray-500 dark:text-gray-500">-</span>
                            )}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-gray-600 dark:text-gray-400">Created</dt>
                        <dd className="mt-1 text-gray-900 dark:text-gray-200">{formatDate(project.created)}</dd>
                    </div>
                </dl>
            </div>

            <div className="border-b border-gray-200 dark:border-gray-800 mb-6">
                <div className="flex gap-8">
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'overview' ? 'border-indigo-500 text-gray-900 dark:text-white' : 'border-transparent text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'}`}
                        onClick={() => changeTab('overview')}
                    >
                        Overview
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'deployments' ? 'border-indigo-500 text-gray-900 dark:text-white' : 'border-transparent text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'}`}
                        onClick={() => changeTab('deployments')}
                    >
                        Deployments
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'service-accounts' ? 'border-indigo-500 text-gray-900 dark:text-white' : 'border-transparent text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'}`}
                        onClick={() => changeTab('service-accounts')}
                    >
                        Service Accounts
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'env-vars' ? 'border-indigo-500 text-gray-900 dark:text-white' : 'border-transparent text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'}`}
                        onClick={() => changeTab('env-vars')}
                    >
                        Environment Variables
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'domains' ? 'border-indigo-500 text-gray-900 dark:text-white' : 'border-transparent text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'}`}
                        onClick={() => changeTab('domains')}
                    >
                        Domains
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'extensions' ? 'border-indigo-500 text-gray-900 dark:text-white' : 'border-transparent text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-300'}`}
                        onClick={() => changeTab('extensions')}
                    >
                        Extensions
                    </button>
                </div>
            </div>

            <div>
                {activeTab === 'overview' && (
                    <div>
                        <h3 className="text-xl font-bold mb-4">Active Deployments</h3>
                        <ActiveDeploymentsSummary projectName={projectName} />
                    </div>
                )}
                {activeTab === 'deployments' && (
                    <div>
                        <DeploymentsList projectName={projectName} />
                    </div>
                )}
                {activeTab === 'service-accounts' && (
                    <div>
                        <ServiceAccountsList projectName={projectName} />
                    </div>
                )}
                {activeTab === 'env-vars' && (
                    <div>
                        <EnvVarsList projectName={projectName} />
                    </div>
                )}
                {activeTab === 'domains' && (
                    <div>
                        <DomainsList projectName={projectName} />
                    </div>
                )}
                {activeTab === 'extensions' && (
                    <div>
                        <ExtensionsList projectName={projectName} />
                    </div>
                )}
            </div>

            <ConfirmDialog
                isOpen={confirmDialogOpen}
                onClose={() => setConfirmDialogOpen(false)}
                onConfirm={handleDeleteConfirm}
                title="Delete Project"
                message={`Are you sure you want to delete project "${project.name}"? This action cannot be undone and will delete all associated deployments, service accounts, and environment variables.`}
                confirmText="Delete Project"
                variant="danger"
                requireConfirmation={true}
                confirmationText={project.name}
                loading={deleting}
            />
        </section>
    );
}
