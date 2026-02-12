// @ts-nocheck
import { Fragment, useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { navigate } from '../lib/navigation';
import { copyToClipboard, formatISO8601, formatRelativeTimeRounded } from '../lib/utils';
import { useToast } from '../components/toast';
import { Button, ConfirmDialog, FormField, Modal, ModalActions, ModalSection, SegmentedRadioGroup } from '../components/ui';
import { ProjectTable } from '../components/project-table';
import { ActiveDeploymentsSummary, DeploymentDetail, DeploymentsList } from './deployments';
import { DomainsList, EnvVarsList, ExtensionDetailPage, ExtensionsList, ServiceAccountsList } from './resources';
import { EmptyState, ErrorState, LoadingState } from '../components/states';
import { useRowKeyboardNavigation, useSortableData } from '../lib/table';


// Projects List Component
export function ProjectsList({ openCreate = false }) {
    const [projects, setProjects] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [formData, setFormData] = useState({ name: '', access_class: 'public', owner: 'self' });
    const [teams, setTeams] = useState([]);
    const [currentUser, setCurrentUser] = useState(null);
    const [accessClasses, setAccessClasses] = useState([]);
    const [saving, setSaving] = useState(false);
    const [actionStatus, setActionStatus] = useState('');
    const { showToast } = useToast();
    const { sortedItems: sortedProjects, sortKey, sortDirection, requestSort } = useSortableData(projects, 'name');
    const { activeIndex, setActiveIndex, onKeyDown } = useRowKeyboardNavigation(
        (idx) => {
            const project = sortedProjects[idx];
            if (project) navigate(`/project/${project.name}`);
        },
        sortedProjects.length
    );

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

    useEffect(() => {
        if (!openCreate) return;
        handleCreateClick();
        window.history.replaceState({}, '', window.location.pathname);
    }, [openCreate]);

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
        setActionStatus(`Creating project ${formData.name}...`);
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
            setActionStatus(`Created project ${formData.name}.`);
            setIsModalOpen(false);
            loadProjects();
        } catch (err) {
            showToast(`Failed to create project: ${err.message}`, 'error');
            setActionStatus(`Failed to create project ${formData.name}.`);
        } finally {
            setSaving(false);
        }
    };

    if (loading) return <LoadingState label="Loading projects..." />;
    if (error) return <ErrorState message={`Error loading projects: ${error}`} onRetry={loadProjects} />;

    return (
        <section>
            <div className="flex justify-end items-center mb-6">
                <Button variant="primary" size="sm" onClick={handleCreateClick}>
                    Create Project
                </Button>
            </div>
            {actionStatus && <p className="mono-inline-status mb-3">{actionStatus}</p>}
            <ProjectTable
                projects={sortedProjects}
                sortKey={sortKey}
                sortDirection={sortDirection}
                requestSort={requestSort}
                onRowClick={(project) => navigate(`/project/${project.name}`)}
                onKeyDown={onKeyDown}
                activeIndex={activeIndex}
                setActiveIndex={setActiveIndex}
                emptyMessage="No projects found."
                emptyActionLabel="Create Project"
                onEmptyAction={handleCreateClick}
            />

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title="Create Project"
            >
                <ModalSection>
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

                    <ModalActions>
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
                    </ModalActions>
                </ModalSection>
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
    const [editingOwner, setEditingOwner] = useState(false);
    const [ownerType, setOwnerType] = useState('user');
    const [ownerUserEmail, setOwnerUserEmail] = useState('');
    const [ownerTeamId, setOwnerTeamId] = useState('');
    const [teams, setTeams] = useState([]);
    const [updatingOwner, setUpdatingOwner] = useState(false);
    const [detailActionStatus, setDetailActionStatus] = useState('');
    const { showToast } = useToast();
    const handleCopy = useCallback(async (value, label) => {
        if (!value || value === '-') return;

        try {
            await copyToClipboard(value);
            showToast(`${label} copied`, 'success');
        } catch (err) {
            showToast(`Failed to copy ${label.toLowerCase()}: ${err.message}`, 'error');
        }
    }, [showToast]);

    const getStatusTone = (status) => {
        const statusTones = {
            Healthy: 'ok',
            Running: 'ok',
            Deploying: 'warn',
            Pending: 'warn',
            Building: 'warn',
            Pushing: 'warn',
            Pushed: 'warn',
            Unhealthy: 'bad',
            Failed: 'bad',
            Stopped: 'muted',
            Cancelled: 'muted',
            Superseded: 'muted',
            Expired: 'muted',
            Terminating: 'muted',
        };
        return statusTones[status] || 'muted';
    };

    const getOwnerInfo = (projectData) => {
        if (!projectData) return null;

        if (projectData.owner_user_email) {
            return { type: 'user', label: projectData.owner_user_email };
        }
        if (projectData.owner_team_name) {
            return { type: 'team', label: projectData.owner_team_name, teamId: projectData.owner_team_id || null };
        }

        const owner = projectData.owner;
        if (!owner) return null;

        if (typeof owner === 'string') {
            if (owner.startsWith('user:')) return { type: 'user', label: owner.slice(5) };
            if (owner.startsWith('team:')) return { type: 'team', label: owner.slice(5), teamId: null };
            return { type: 'user', label: owner };
        }

        if (owner.user_email) return { type: 'user', label: owner.user_email };
        if (owner.team_name) return { type: 'team', label: owner.team_name, teamId: owner.team_id || owner.team || null };
        if (owner.email) return { type: 'user', label: owner.email, userId: owner.id || null };
        if (owner.name) return { type: 'team', label: owner.name, teamId: owner.id || null };

        if (owner.user && typeof owner.user === 'object' && owner.user.email) {
            return { type: 'user', label: owner.user.email, userId: owner.user.id || owner.user.user_id || null };
        }
        if (owner.team && typeof owner.team === 'object' && owner.team.name) {
            return { type: 'team', label: owner.team.name, teamId: owner.team.id || owner.team.team_id || null };
        }

        if (owner.user && typeof owner.user === 'string') {
            return { type: 'user', label: owner.user, userId: owner.user };
        }
        if (owner.team && typeof owner.team === 'string') {
            return { type: 'team', label: owner.team, teamId: owner.team };
        }

        if (projectData.owner_user) {
            return { type: 'user', label: String(projectData.owner_user).replace(/^user:/, ''), userId: projectData.owner_user };
        }
        if (projectData.owner_team) {
            return { type: 'team', label: String(projectData.owner_team).replace(/^team:/, ''), teamId: projectData.owner_team };
        }

        return null;
    };

    const loadProject = useCallback(async () => {
        try {
            const data = await api.getProject(projectName, { expand: 'owner' });
            setProject(data);
        } catch (err) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    }, [projectName]);

    useEffect(() => {
        loadProject();
    }, [loadProject]);

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

    useEffect(() => {
        async function loadTeams() {
            try {
                const data = await api.getTeams();
                setTeams(data || []);
            } catch (err) {
                console.error('Failed to load teams:', err);
            }
        }
        loadTeams();
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
        setDetailActionStatus(`Deleting project ${project.name}...`);
        try {
            await api.deleteProject(project.name);
            showToast(`Project ${project.name} deleted successfully`, 'success');
            setDetailActionStatus(`Deleted project ${project.name}.`);
            setConfirmDialogOpen(false);
            // Redirect to projects list
            navigate('/projects');
        } catch (err) {
            showToast(`Failed to delete project: ${err.message}`, 'error');
            setDetailActionStatus(`Failed to delete project ${project.name}.`);
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
        setDetailActionStatus(`Updating access class for ${project.name}...`);
        try {
            await api.updateProject(project.name, { access_class: newAccessClass });
            const ac = accessClasses.find(a => a.id === newAccessClass);
            showToast(`Project access class updated to ${ac ? ac.display_name : newAccessClass}`, 'success');
            setDetailActionStatus(`Updated access class to ${ac ? ac.display_name : newAccessClass}.`);
            // Reload project to get updated data
            const updatedProject = await api.getProject(projectName, { expand: 'owner' });
            setProject(updatedProject);
            setEditingAccessClass(false);
        } catch (err) {
            showToast(`Failed to update access class: ${err.message}`, 'error');
            setDetailActionStatus(`Failed to update access class for ${project.name}.`);
        } finally {
            setUpdatingAccessClass(false);
        }
    };

    const handleEditOwner = () => {
        const currentOwner = getOwnerInfo(project);
        const initialType = currentOwner?.type || 'user';
        setOwnerType(initialType);
        setOwnerUserEmail(initialType === 'user' ? (currentOwner?.label || '') : '');
        if (initialType === 'team' && teams.length > 0) {
            const matchingTeam = teams.find((t) => t.name === currentOwner?.label || t.id === currentOwner?.teamId);
            setOwnerTeamId(matchingTeam?.id || currentOwner?.teamId || teams[0]?.id || '');
        } else {
            setOwnerTeamId(teams[0]?.id || '');
        }
        setEditingOwner(true);
    };

    const handleCancelEditOwner = () => {
        setEditingOwner(false);
        setOwnerType('user');
        setOwnerUserEmail('');
        setOwnerTeamId('');
    };

    const handleSaveOwner = async () => {
        if (!project) return;

        if (ownerType === 'user' && !ownerUserEmail.trim()) {
            showToast('User email is required', 'error');
            return;
        }
        if (ownerType === 'team' && !ownerTeamId) {
            showToast('Team is required', 'error');
            return;
        }

        setUpdatingOwner(true);
        setDetailActionStatus(`Transferring ownership for ${project.name}...`);

        try {
            let ownerPayload;
            if (ownerType === 'user') {
                const lookup = await api.lookupUsers([ownerUserEmail.trim()]);
                if (!lookup?.users?.length) {
                    showToast(`User not found: ${ownerUserEmail.trim()}`, 'error');
                    setDetailActionStatus(`Failed to transfer ownership for ${project.name}.`);
                    return;
                }
                ownerPayload = { user: lookup.users[0].id };
            } else {
                ownerPayload = { team: ownerTeamId };
            }

            await api.updateProject(project.name, { owner: ownerPayload });
            const updatedProject = await api.getProject(projectName, { expand: 'owner' });
            setProject(updatedProject);
            showToast('Project owner updated', 'success');
            setDetailActionStatus(`Ownership transferred for ${project.name}.`);
            handleCancelEditOwner();
        } catch (err) {
            showToast(`Failed to update owner: ${err.message}`, 'error');
            setDetailActionStatus(`Failed to transfer ownership for ${project.name}.`);
        } finally {
            setUpdatingOwner(false);
        }
    };

    if (loading) return <LoadingState label="Loading project..." />;
    if (error) return <ErrorState message={`Error loading project: ${error}`} onRetry={loadProject} />;
    if (!project) return <EmptyState message="Project not found." />;

    const ownerInfo = getOwnerInfo(project);

    return (
        <section>
            <div className="flex justify-end items-start mb-4">
                <Button
                    variant="danger"
                    size="sm"
                    onClick={handleDeleteClick}
                >
                    Delete Project
                </Button>
            </div>

            {detailActionStatus && <p className="mono-inline-status mb-4">{detailActionStatus}</p>}

            <div className="mono-status-strip mb-6">
                <div className={`mono-status-card mono-status-card-${getStatusTone(project.status)}`}>
                    <span>status</span>
                    <strong>{project.status}</strong>
                </div>
                <div>
                    <span>primary_url</span>
                    <strong className="mono-copyable-value">
                        <span>
                            {project.primary_url ? (
                                <a href={project.primary_url} target="_blank" rel="noopener noreferrer" className="underline uppercase">
                                    {project.primary_url}
                                </a>
                            ) : '-'}
                        </span>
                        {project.primary_url && (
                            <button
                                type="button"
                                className="mono-copy-button"
                                title="Copy primary URL"
                                aria-label="Copy primary URL"
                                onClick={() => handleCopy(project.primary_url, 'Primary URL')}
                            >
                                <span
                                    className="mono-copy-icon svg-mask"
                                    style={{ maskImage: 'url(/assets/copy.svg)', WebkitMaskImage: 'url(/assets/copy.svg)' }}
                                />
                            </button>
                        )}
                    </strong>
                </div>
                <div>
                    <span>access</span>
                    {!editingAccessClass ? (
                        <strong className="mono-copyable-value">
                            <span>{project.access_class}</span>
                            <button
                                type="button"
                                className="mono-copy-button"
                                title="Edit access class"
                                aria-label="Edit access class"
                                onClick={handleEditAccessClass}
                            >
                                <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                                    <path strokeLinecap="round" strokeLinejoin="round" d="M12 20h9" />
                                    <path strokeLinecap="round" strokeLinejoin="round" d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z" />
                                </svg>
                            </button>
                        </strong>
                    ) : (
                        <div className="flex flex-wrap items-center gap-2">
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
                    )}
                </div>
                <div>
                    <span>created</span>
                    <strong className="mono-copyable-value" title={formatISO8601(project.created)}>
                        <span>{formatRelativeTimeRounded(project.created)}</span>
                        <button
                            type="button"
                            className="mono-copy-button"
                            title="Copy created timestamp (ISO8601)"
                            aria-label="Copy created timestamp (ISO8601)"
                            onClick={() => handleCopy(formatISO8601(project.created), 'Created timestamp')}
                        >
                            <span
                                className="mono-copy-icon svg-mask"
                                style={{ maskImage: 'url(/assets/copy.svg)', WebkitMaskImage: 'url(/assets/copy.svg)' }}
                            />
                        </button>
                    </strong>
                </div>
                <div>
                    <span>owner</span>
                    <strong className="mono-copyable-value">
                        <span className="inline-flex items-center gap-2">
                            {ownerInfo?.type === 'user' && (
                                <span
                                    className="w-3 h-3 svg-mask inline-block"
                                    aria-hidden="true"
                                    style={{
                                        maskImage: 'url(/assets/user.svg)',
                                        WebkitMaskImage: 'url(/assets/user.svg)',
                                    }}
                                />
                            )}
                            {ownerInfo?.type === 'team' && (
                                <svg className="w-3 h-3" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                                    <path d="M7 10a3 3 0 1 0-3-3 3 3 0 0 0 3 3Zm6 0a3 3 0 1 0-3-3 3 3 0 0 0 3 3ZM1.5 16.5a5.5 5.5 0 0 1 11 0v.5h-11Zm12 0a5.5 5.5 0 0 1 5-5.48 5.53 5.53 0 0 1 .5.02V17h-5.5Z" />
                                </svg>
                            )}
                            {ownerInfo?.type === 'team' ? (
                                <button
                                    type="button"
                                    className="underline"
                                    onClick={() => navigate(`/team/${ownerInfo.label}`)}
                                >
                                    {ownerInfo.label}
                                </button>
                            ) : (
                                <span>{ownerInfo?.label || '-'}</span>
                            )}
                        </span>
                        <button
                            type="button"
                            className="mono-copy-button"
                            title="Transfer ownership"
                            aria-label="Transfer ownership"
                            onClick={handleEditOwner}
                        >
                            <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                                <path strokeLinecap="round" strokeLinejoin="round" d="M12 20h9" />
                                <path strokeLinecap="round" strokeLinejoin="round" d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z" />
                            </svg>
                        </button>
                    </strong>
                </div>
                <div>
                    <span>custom_domains</span>
                    <strong>
                        {project.custom_domain_urls && project.custom_domain_urls.length > 0
                            ? project.custom_domain_urls.map((url, idx) => (
                                <Fragment key={url}>
                                    {idx > 0 ? ', ' : ''}
                                    <a href={url} target="_blank" rel="noopener noreferrer" className="underline uppercase">
                                        {url}
                                    </a>
                                </Fragment>
                            ))
                            : '-'}
                    </strong>
                </div>
            </div>

            <div className="mono-tabbar mb-6">
                <div className="flex flex-wrap gap-2">
                    <button
                        className={`mono-tab-button ${activeTab === 'overview' ? 'active' : ''}`}
                        onClick={() => changeTab('overview')}
                    >
                        Overview
                    </button>
                    <button
                        className={`mono-tab-button ${activeTab === 'deployments' ? 'active' : ''}`}
                        onClick={() => changeTab('deployments')}
                    >
                        Deployments
                    </button>
                    <button
                        className={`mono-tab-button ${activeTab === 'service-accounts' ? 'active' : ''}`}
                        onClick={() => changeTab('service-accounts')}
                    >
                        Service Accounts
                    </button>
                    <button
                        className={`mono-tab-button ${activeTab === 'env-vars' ? 'active' : ''}`}
                        onClick={() => changeTab('env-vars')}
                    >
                        Environment Variables
                    </button>
                    <button
                        className={`mono-tab-button ${activeTab === 'domains' ? 'active' : ''}`}
                        onClick={() => changeTab('domains')}
                    >
                        Domains
                    </button>
                    <button
                        className={`mono-tab-button ${activeTab === 'extensions' ? 'active' : ''}`}
                        onClick={() => changeTab('extensions')}
                    >
                        Extensions
                    </button>
                </div>
            </div>

            <div className="mono-tab-panel">
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
                message={`Delete project "${project.name}"? Impact: this removes associated deployments, service accounts, and environment variables.`}
                confirmText="Delete Project"
                variant="danger"
                requireConfirmation={true}
                confirmationText={project.name}
                loading={deleting}
            />

            <Modal
                isOpen={editingOwner}
                onClose={handleCancelEditOwner}
                title="Transfer Project Ownership"
                maxWidth="max-w-lg"
            >
                <ModalSection>
                    <SegmentedRadioGroup
                        label="Owner Type"
                        name="owner-type"
                        value={ownerType}
                        onChange={setOwnerType}
                        options={[
                            { value: 'user', label: 'USER' },
                            { value: 'team', label: 'TEAM' },
                        ]}
                    />

                    {ownerType === 'user' ? (
                        <FormField
                            label="Owner User Email"
                            id="project-owner-user-email"
                            value={ownerUserEmail}
                            onChange={(e) => setOwnerUserEmail(e.target.value)}
                            placeholder="owner@example.com"
                            required
                        />
                    ) : (
                        <FormField
                            label="Owner Team"
                            id="project-owner-team"
                            type="select"
                            value={ownerTeamId}
                            onChange={(e) => setOwnerTeamId(e.target.value)}
                            required
                        >
                            <option value="" disabled>Select a team</option>
                            {teams.map((team) => (
                                <option key={team.id} value={team.id}>{team.name}</option>
                            ))}
                        </FormField>
                    )}

                    <ModalActions>
                        <Button variant="secondary" onClick={handleCancelEditOwner} disabled={updatingOwner}>
                            Cancel
                        </Button>
                        <Button variant="primary" onClick={handleSaveOwner} loading={updatingOwner}>
                            Transfer Ownership
                        </Button>
                    </ModalActions>
                </ModalSection>
            </Modal>
        </section>
    );
}
