// React-based Rise Dashboard Application with Tailwind CSS
const { useState, useEffect, useCallback } = React;
// CONFIG is already defined in auth.js which loads before this script

// Utility functions
function formatDate(dateString) {
    const date = new Date(dateString);
    return date.toLocaleString();
}

function formatTimeRemaining(expiresAt) {
    if (!expiresAt) return null;

    const now = new Date();
    const expiryDate = new Date(expiresAt);
    const diffMs = expiryDate - now;
    const diffSec = Math.floor(Math.abs(diffMs) / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHour = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHour / 24);

    const isExpired = diffMs < 0;
    const prefix = isExpired ? 'expired ' : 'in ';
    const suffix = isExpired ? ' ago' : '';

    if (diffDay > 0) {
        return `${prefix}${diffDay} day${diffDay > 1 ? 's' : ''}${suffix}`;
    } else if (diffHour > 0) {
        return `${prefix}${diffHour} hour${diffHour > 1 ? 's' : ''}${suffix}`;
    } else if (diffMin > 0) {
        return `${prefix}${diffMin} minute${diffMin > 1 ? 's' : ''}${suffix}`;
    } else {
        return `${prefix}${diffSec} second${diffSec !== 1 ? 's' : ''}${suffix}`;
    }
}

// Navigation helpers
function useHashLocation() {
    const [hash, setHash] = useState(window.location.hash.slice(1) || 'projects');

    useEffect(() => {
        const handleHashChange = () => {
            setHash(window.location.hash.slice(1) || 'projects');
        };
        window.addEventListener('hashchange', handleHashChange);
        return () => window.removeEventListener('hashchange', handleHashChange);
    }, []);

    return hash;
}

// Header Component
function Header({ user, onLogout, currentView }) {
    const [isProfileOpen, setIsProfileOpen] = useState(false);
    const profileRef = React.useRef(null);
    const { showToast } = useToast();

    // Determine which section is active (projects or teams)
    const isProjectsActive = currentView === 'projects' || currentView === 'project-detail' || currentView === 'deployment-detail';
    const isTeamsActive = currentView === 'teams' || currentView === 'team-detail';

    // Close dropdown when clicking outside
    useEffect(() => {
        function handleClickOutside(event) {
            if (profileRef.current && !profileRef.current.contains(event.target)) {
                setIsProfileOpen(false);
            }
        }

        if (isProfileOpen) {
            document.addEventListener('mousedown', handleClickOutside);
            return () => document.removeEventListener('mousedown', handleClickOutside);
        }
    }, [isProfileOpen]);

    const handleCopyJWT = () => {
        const token = localStorage.getItem('rise_token');
        if (token) {
            navigator.clipboard.writeText(token).then(() => {
                showToast('JWT token copied to clipboard', 'success');
                setIsProfileOpen(false);
            }).catch(() => {
                showToast('Failed to copy JWT token', 'error');
            });
        }
    };

    return (
        <header className="bg-gray-900 border-b border-gray-800">
            <nav className="container mx-auto px-4 py-4">
                <div className="flex items-center justify-between">
                    <a href="#projects" className="flex items-center gap-2 hover:opacity-80 transition-opacity">
                        <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                        </svg>
                        <strong className="text-lg font-bold">Rise Dashboard</strong>
                    </a>
                    <div className="flex items-center gap-6">
                        <a
                            href="#projects"
                            className={`transition-colors ${isProjectsActive ? 'text-indigo-400 font-semibold' : 'text-gray-300 hover:text-white'}`}
                        >
                            Projects
                        </a>
                        <a
                            href="#teams"
                            className={`transition-colors ${isTeamsActive ? 'text-indigo-400 font-semibold' : 'text-gray-300 hover:text-white'}`}
                        >
                            Teams
                        </a>

                        {/* User Profile Dropdown */}
                        <div className="relative" ref={profileRef}>
                            <button
                                onClick={() => setIsProfileOpen(!isProfileOpen)}
                                className="flex items-center gap-2 hover:opacity-80 transition-opacity"
                            >
                                <div className="w-8 h-8 rounded-full bg-indigo-600 flex items-center justify-center border-2 border-indigo-500">
                                    <svg className="w-5 h-5 text-white" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                        <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                        <circle cx="12" cy="7" r="4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                    </svg>
                                </div>
                            </button>

                            {isProfileOpen && (
                                <div className="absolute right-0 mt-2 w-64 bg-gray-800 border border-gray-700 rounded-lg shadow-xl z-50">
                                    <div className="p-4 border-b border-gray-700">
                                        <p className="text-sm text-gray-400 mb-1">Signed in as</p>
                                        <p className="text-white font-medium break-all">{user?.email}</p>
                                    </div>
                                    <div className="p-2">
                                        <button
                                            onClick={handleCopyJWT}
                                            className="w-full flex items-center gap-2 px-3 py-2 text-left text-gray-300 hover:bg-gray-700 rounded transition-colors"
                                        >
                                            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                <rect x="9" y="9" width="13" height="13" rx="2" ry="2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                            </svg>
                                            Copy JWT Token
                                        </button>
                                        <button
                                            onClick={() => { setIsProfileOpen(false); onLogout(); }}
                                            className="w-full flex items-center gap-2 px-3 py-2 text-left text-red-400 hover:bg-gray-700 rounded transition-colors"
                                        >
                                            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                                                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                                <polyline points="16 17 21 12 16 7" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                                <line x1="21" y1="12" x2="9" y2="12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                                            </svg>
                                            Logout
                                        </button>
                                    </div>
                                </div>
                            )}
                        </div>
                    </div>
                </div>
            </nav>
        </header>
    );
}

// Status Badge Component
function StatusBadge({ status }) {
    const statusColors = {
        'Healthy': 'bg-green-600',
        'Running': 'bg-green-600',
        'Deploying': 'bg-yellow-600',
        'Pending': 'bg-yellow-600',
        'Building': 'bg-yellow-600',
        'Pushing': 'bg-yellow-600',
        'Pushed': 'bg-yellow-600',
        'Unhealthy': 'bg-red-600',
        'Failed': 'bg-red-600',
        'Stopped': 'bg-gray-600',
        'Cancelled': 'bg-gray-600',
        'Superseded': 'bg-gray-600',
        'Expired': 'bg-gray-600',
        'Terminating': 'bg-gray-600',
    };

    const color = statusColors[status] || 'bg-gray-600';

    return (
        <span className={`${color} text-white text-xs font-semibold px-3 py-1 rounded-full uppercase`}>
            {status}
        </span>
    );
}

// Projects List Component
function ProjectsList() {
    const [projects, setProjects] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [formData, setFormData] = useState({ name: '', visibility: 'Public', owner: 'self' });
    const [teams, setTeams] = useState([]);
    const [currentUser, setCurrentUser] = useState(null);
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

    const handleCreateClick = () => {
        setFormData({ name: '', visibility: 'Public', owner: 'self' });
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

            await api.createProject(formData.name, formData.visibility, owner);
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
    if (error) return <p className="text-red-400">Error loading projects: {error}</p>;

    return (
        <section>
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-2xl font-bold">Projects</h2>
                <Button variant="primary" size="sm" onClick={handleCreateClick}>
                    Create Project
                </Button>
            </div>
            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Name</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Status</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Owner</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Visibility</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">URL</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {projects.length === 0 ? (
                            <tr>
                                <td colSpan="5" className="px-6 py-8 text-center text-gray-400">
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
                                    onClick={() => window.location.hash = `project/${p.name}`}
                                    className="hover:bg-gray-800/50 transition-colors cursor-pointer"
                                >
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-100">{p.name}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm"><StatusBadge status={p.status} /></td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{owner}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{p.visibility}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm">
                                        {p.project_url ? (
                                            <a
                                                href={p.project_url}
                                                target="_blank"
                                                rel="noopener noreferrer"
                                                className="text-indigo-400 hover:text-indigo-300"
                                                onClick={(e) => e.stopPropagation()}
                                            >
                                                {p.project_url}
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
                    <p className="text-sm text-gray-500 -mt-2">
                        Only lowercase letters, numbers, and hyphens allowed
                    </p>

                    <FormField
                        label="Visibility"
                        id="project-visibility"
                        type="select"
                        value={formData.visibility}
                        onChange={(e) => setFormData({ ...formData, visibility: e.target.value })}
                        required
                    >
                        <option value="Public">Public</option>
                        <option value="Private">Private</option>
                    </FormField>

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

// Teams List Component
function TeamsList({ currentUser }) {
    const [teams, setTeams] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [formData, setFormData] = useState({ name: '', members: '', owners: '' });
    const [saving, setSaving] = useState(false);
    const { showToast } = useToast();

    const loadTeams = useCallback(async () => {
        try {
            const data = await api.getTeams();
            setTeams(data);
        } catch (err) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadTeams();
    }, [loadTeams]);

    const handleCreateClick = () => {
        setFormData({ name: '', members: '', owners: currentUser?.email || '' });
        setIsModalOpen(true);
    };

    const handleCreate = async () => {
        if (!formData.name) {
            showToast('Team name is required', 'error');
            return;
        }

        // Parse comma-separated email lists
        const memberEmails = formData.members
            .split(',')
            .map(e => e.trim())
            .filter(e => e.length > 0);

        const ownerEmails = formData.owners
            .split(',')
            .map(e => e.trim())
            .filter(e => e.length > 0);

        if (ownerEmails.length === 0) {
            showToast('At least one owner is required', 'error');
            return;
        }

        setSaving(true);
        try {
            // Look up user IDs for owners and members
            const ownerLookup = await api.lookupUsers(ownerEmails);
            const memberLookup = memberEmails.length > 0 ? await api.lookupUsers(memberEmails) : { users: [] };

            if (!ownerLookup.users || ownerLookup.users.length !== ownerEmails.length) {
                showToast('One or more owner email addresses not found', 'error');
                setSaving(false);
                return;
            }

            if (memberEmails.length > 0 && (!memberLookup.users || memberLookup.users.length !== memberEmails.length)) {
                showToast('One or more member email addresses not found', 'error');
                setSaving(false);
                return;
            }

            const ownerIds = ownerLookup.users.map(u => u.id);
            const memberIds = memberLookup.users.map(u => u.id);

            await api.createTeam(formData.name, memberIds, ownerIds);
            showToast(`Team ${formData.name} created successfully`, 'success');
            setIsModalOpen(false);
            loadTeams();
        } catch (err) {
            showToast(`Failed to create team: ${err.message}`, 'error');
        } finally {
            setSaving(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading teams: {error}</p>;

    return (
        <section>
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-2xl font-bold">Teams</h2>
                <Button variant="primary" size="sm" onClick={handleCreateClick}>
                    Create Team
                </Button>
            </div>
            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Name</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Members</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Owners</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Created</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {teams.length === 0 ? (
                            <tr>
                                <td colSpan="4" className="px-6 py-8 text-center text-gray-400">
                                    No teams found.
                                </td>
                            </tr>
                        ) : (
                            teams.map(t => (
                                <tr
                                    key={t.id}
                                    onClick={() => window.location.hash = `team/${t.name}`}
                                    className="hover:bg-gray-800/50 transition-colors cursor-pointer"
                                >
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-100">
                                        <div className="flex items-center gap-2">
                                            {t.name}
                                            {t.idp_managed && (
                                                <span className="text-xs bg-purple-600 text-white px-2 py-0.5 rounded">IDP</span>
                                            )}
                                        </div>
                                    </td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{t.members.length}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{t.owners.length}</td>
                                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{formatDate(t.created)}</td>
                                </tr>
                            ))
                        )}
                    </tbody>
                </table>
            </div>

            <Modal
                isOpen={isModalOpen}
                onClose={() => setIsModalOpen(false)}
                title="Create Team"
            >
                <div className="space-y-4">
                    <FormField
                        label="Team Name"
                        id="team-name"
                        value={formData.name}
                        onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                        placeholder="engineering"
                        required
                    />

                    <FormField
                        label="Owners (emails, comma-separated)"
                        id="team-owners"
                        type="textarea"
                        value={formData.owners}
                        onChange={(e) => setFormData({ ...formData, owners: e.target.value })}
                        placeholder="alice@example.com, bob@example.com"
                        required
                        rows={3}
                    />
                    <p className="text-sm text-gray-500 -mt-2">
                        Owners can manage the team. At least one owner is required.
                    </p>

                    <FormField
                        label="Members (emails, comma-separated)"
                        id="team-members"
                        type="textarea"
                        value={formData.members}
                        onChange={(e) => setFormData({ ...formData, members: e.target.value })}
                        placeholder="charlie@example.com, dana@example.com"
                        rows={3}
                    />
                    <p className="text-sm text-gray-500 -mt-2">
                        Members can use the team for project ownership.
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

// Active Deployments Summary Component
function ActiveDeploymentsSummary({ projectName }) {
    const [activeDeployments, setActiveDeployments] = useState({});
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [deploymentToStop, setDeploymentToStop] = useState(null);
    const [stopping, setStopping] = useState(false);
    const { showToast } = useToast();

    const isTerminal = (status) => {
        return ['Cancelled', 'Stopped', 'Superseded', 'Failed', 'Expired'].includes(status);
    };

    const loadSummary = useCallback(async () => {
        try {
            const deployments = await api.getProjectDeployments(projectName, { limit: 100 });

            // Group ALL deployments by deployment group
            const allGrouped = deployments.reduce((acc, d) => {
                const group = d.deployment_group || 'default';
                if (!acc[group]) acc[group] = [];
                acc[group].push(d);
                return acc;
            }, {});

            // Sort each group by created date (newest first)
            Object.keys(allGrouped).forEach(group => {
                allGrouped[group].sort((a, b) => new Date(b.created) - new Date(a.created));
            });

            // Filter groups based on visibility rules:
            // - "default" group: always include if it exists (show latest deployment regardless of status)
            // - Other groups: only include if they have at least one non-terminal deployment
            const filtered = {};
            Object.keys(allGrouped).forEach(group => {
                if (group === 'default') {
                    // Always include default group - show latest deployment regardless of status
                    filtered[group] = allGrouped[group];
                } else {
                    // Only include non-default groups if they have non-terminal deployments
                    const nonTerminal = allGrouped[group].filter(d => !isTerminal(d.status));
                    if (nonTerminal.length > 0) {
                        filtered[group] = nonTerminal;
                    }
                }
            });

            setActiveDeployments(filtered);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName]);

    useEffect(() => {
        loadSummary();
        const interval = setInterval(loadSummary, 5000);
        return () => clearInterval(interval);
    }, [loadSummary]);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading active deployments: {error}</p>;

    const handleStopClick = (deployment) => {
        setDeploymentToStop(deployment);
        setConfirmDialogOpen(true);
    };

    const handleStopConfirm = async () => {
        if (!deploymentToStop) return;

        setStopping(true);
        try {
            await api.stopDeployment(projectName, deploymentToStop.deployment_id);
            showToast(`Deployment ${deploymentToStop.deployment_id} stopped successfully`, 'success');
            setConfirmDialogOpen(false);
            setDeploymentToStop(null);
            loadSummary(); // Refresh the list
        } catch (err) {
            showToast(`Failed to stop deployment: ${err.message}`, 'error');
        } finally {
            setStopping(false);
        }
    };

    const groups = Object.keys(activeDeployments);
    if (groups.length === 0) return <p className="text-gray-400">No active deployments.</p>;

    // Sort groups: "default" first, then by latest deployment's created timestamp
    const sortedGroups = groups.sort((a, b) => {
        if (a === 'default') return -1;
        if (b === 'default') return 1;

        // Both non-default: sort by latest deployment's created timestamp (descending)
        const latestA = activeDeployments[a][0];
        const latestB = activeDeployments[b][0];
        return new Date(latestB.created) - new Date(latestA.created);
    });

    return (
        <>
            <div className="space-y-4">
                {sortedGroups.map(group => {
                    const deps = activeDeployments[group];
                    const latest = deps[0];
                    const canStop = !isTerminal(latest.status);
                    return (
                        <div key={group} className="bg-gray-900 border border-gray-800 rounded-lg p-6">
                            <div className="flex justify-between items-center mb-4">
                                <h5 className="text-lg font-semibold">Group: {group}</h5>
                                <div className="flex items-center gap-3">
                                    <StatusBadge status={latest.status} />
                                    {canStop && (
                                        <Button
                                            variant="danger"
                                            size="sm"
                                            onClick={() => handleStopClick(latest)}
                                        >
                                            Stop
                                        </Button>
                                    )}
                                </div>
                            </div>
                        <dl className="grid grid-cols-2 gap-4 text-sm">
                            <div>
                                <dt className="text-gray-400">Deployment ID</dt>
                                <dd className="font-mono text-gray-200">{latest.deployment_id}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-400">Image</dt>
                                <dd className="font-mono text-gray-200 text-xs">{latest.image ? latest.image.split('/').pop() : '-'}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-400">URL</dt>
                                <dd>{latest.deployment_url ? <a href={latest.deployment_url} target="_blank" rel="noopener noreferrer" className="text-indigo-400 hover:text-indigo-300">{latest.deployment_url}</a> : '-'}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-400">Created</dt>
                                <dd className="text-gray-200">{formatDate(latest.created)}</dd>
                            </div>
                            {latest.expires_at && (
                                <div>
                                    <dt className="text-gray-400">Expires</dt>
                                    <dd className="text-gray-200">
                                        {formatTimeRemaining(latest.expires_at)}
                                        <span className="text-gray-500 text-xs ml-2">({formatDate(latest.expires_at)})</span>
                                    </dd>
                                </div>
                            )}
                        </dl>
                        <div className="mt-4 pt-4 border-t border-gray-800 flex items-center justify-between">
                            <a href={`#deployment/${projectName}/${latest.deployment_id}`} className="text-indigo-400 hover:text-indigo-300">
                                View Details
                            </a>
                            {deps.length > 1 && (
                                <span className="text-sm text-gray-500">
                                    (+{deps.length - 1} more active)
                                </span>
                            )}
                        </div>
                    </div>
                );
            })}
            </div>

            <ConfirmDialog
                isOpen={confirmDialogOpen}
                onClose={() => {
                    setConfirmDialogOpen(false);
                    setDeploymentToStop(null);
                }}
                onConfirm={handleStopConfirm}
                title="Stop Deployment"
                message={`Are you sure you want to stop deployment ${deploymentToStop?.deployment_id}? This action will terminate the deployment.`}
                confirmText="Stop Deployment"
                variant="danger"
                loading={stopping}
            />
        </>
    );
}

// Deployments List Component (with pagination)
function DeploymentsList({ projectName }) {
    const [deployments, setDeployments] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [page, setPage] = useState(0);
    const [hasMore, setHasMore] = useState(true);
    const [groupFilter, setGroupFilter] = useState('');
    const [deploymentGroups, setDeploymentGroups] = useState([]);
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [deploymentToStop, setDeploymentToStop] = useState(null);
    const [stopping, setStopping] = useState(false);
    const [rollbackDialogOpen, setRollbackDialogOpen] = useState(false);
    const [deploymentToRollback, setDeploymentToRollback] = useState(null);
    const [rollingBack, setRollingBack] = useState(false);
    const { showToast } = useToast();
    const pageSize = 10;

    // Load deployment groups
    useEffect(() => {
        async function loadGroups() {
            try {
                const groups = await api.getDeploymentGroups(projectName);
                setDeploymentGroups(groups);
            } catch (err) {
                console.error('Failed to load deployment groups:', err);
            }
        }
        loadGroups();
    }, [projectName]);

    const loadDeployments = useCallback(async () => {
        try {
            const params = {
                limit: pageSize,
                offset: page * pageSize,
            };
            if (groupFilter) params.group = groupFilter;

            const data = await api.getProjectDeployments(projectName, params);
            setDeployments(data);
            setHasMore(data.length >= pageSize);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName, page, groupFilter]);

    useEffect(() => {
        loadDeployments();
        const interval = setInterval(loadDeployments, 5000);
        return () => clearInterval(interval);
    }, [loadDeployments]);

    const handleGroupChange = (e) => {
        setGroupFilter(e.target.value);
        setPage(0);
    };

    const isTerminal = (status) => {
        return ['Cancelled', 'Stopped', 'Superseded', 'Failed', 'Expired'].includes(status);
    };

    const isRollbackable = (status) => {
        return ['Healthy', 'Superseded'].includes(status);
    };

    const handleStopClick = (deployment) => {
        setDeploymentToStop(deployment);
        setConfirmDialogOpen(true);
    };

    const handleStopConfirm = async () => {
        if (!deploymentToStop) return;

        setStopping(true);
        try {
            await api.stopDeployment(projectName, deploymentToStop.deployment_id);
            showToast(`Deployment ${deploymentToStop.deployment_id} stopped successfully`, 'success');
            setConfirmDialogOpen(false);
            setDeploymentToStop(null);
            loadDeployments();
        } catch (err) {
            showToast(`Failed to stop deployment: ${err.message}`, 'error');
        } finally {
            setStopping(false);
        }
    };

    const handleRollbackClick = (deployment) => {
        setDeploymentToRollback(deployment);
        setRollbackDialogOpen(true);
    };

    const handleRollbackConfirm = async () => {
        if (!deploymentToRollback) return;

        setRollingBack(true);
        try {
            const response = await api.rollbackDeployment(projectName, deploymentToRollback.deployment_id);
            showToast(`Rollback successful! New deployment: ${response.new_deployment_id}`, 'success');
            setRollbackDialogOpen(false);
            setDeploymentToRollback(null);
            loadDeployments();
        } catch (err) {
            showToast(`Failed to rollback deployment: ${err.message}`, 'error');
        } finally {
            setRollingBack(false);
        }
    };

    if (loading && deployments.length === 0) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading deployments: {error}</p>;

    // Find the most recent deployment in the default group (only non-terminal)
    const mostRecentDefault = deployments.find(d => d.deployment_group === 'default' && !isTerminal(d.status));

    return (
        <div>
            <div className="mb-4 flex items-center gap-2">
                <label htmlFor="deployment-group-filter" className="flex items-center gap-2">
                    <span className="text-sm text-gray-400 whitespace-nowrap">Filter by group:</span>
                    <select
                        id="deployment-group-filter"
                        value={groupFilter}
                        onChange={handleGroupChange}
                        className="bg-gray-900 border border-gray-700 rounded px-3 py-2 text-sm text-gray-100 focus:outline-none focus:border-indigo-500 cursor-pointer"
                    >
                        <option value="">All groups</option>
                        {deploymentGroups.map(group => (
                            <option key={group} value={group}>{group}</option>
                        ))}
                    </select>
                </label>
            </div>

            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">ID</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Status</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Created by</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Image</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Group</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">URL</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Expires</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Created</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Actions</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {deployments.length === 0 ? (
                            <tr>
                                <td colSpan="9" className="px-6 py-8 text-center text-gray-400">
                                    No deployments found.
                                </td>
                            </tr>
                        ) : (
                            deployments.map(d => {
                                    const isHighlighted = mostRecentDefault && d.id === mostRecentDefault.id;
                                    return (
                                    <tr
                                        key={d.id}
                                        onClick={() => window.location.hash = `deployment/${projectName}/${d.deployment_id}`}
                                        className={`transition-colors cursor-pointer ${isHighlighted ? 'bg-indigo-900/20 border-l-4 border-l-indigo-500 hover:bg-indigo-900/30' : 'hover:bg-gray-800/50'}`}
                                    >
                                        <td className="px-6 py-4 whitespace-nowrap text-sm font-mono text-gray-200">{d.deployment_id}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm"><StatusBadge status={d.status} /></td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{d.created_by_email || '-'}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-xs font-mono text-gray-300">{d.image ? d.image.split('/').pop() : '-'}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{d.deployment_group}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm">
                                            {d.deployment_url ? (
                                                <a
                                                    href={d.deployment_url}
                                                    target="_blank"
                                                    rel="noopener noreferrer"
                                                    className="text-indigo-400 hover:text-indigo-300"
                                                    onClick={(e) => e.stopPropagation()}
                                                >
                                                    Link
                                                </a>
                                            ) : '-'}
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">
                                            {d.expires_at ? (
                                                <span>
                                                    {formatTimeRemaining(d.expires_at)}
                                                    <br />
                                                    <span className="text-gray-500 text-xs">({formatDate(d.expires_at)})</span>
                                                </span>
                                            ) : '-'}
                                        </td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{formatDate(d.created)}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm">
                                            <div className="flex gap-2">
                                                {isRollbackable(d.status) && (
                                                    <Button
                                                        variant="primary"
                                                        size="sm"
                                                        onClick={(e) => {
                                                            e.stopPropagation();
                                                            handleRollbackClick(d);
                                                        }}
                                                    >
                                                        Rollback
                                                    </Button>
                                                )}
                                                {!isTerminal(d.status) && (
                                                    <Button
                                                        variant="danger"
                                                        size="sm"
                                                        onClick={(e) => {
                                                            e.stopPropagation();
                                                            handleStopClick(d);
                                                        }}
                                                    >
                                                        Stop
                                                    </Button>
                                                )}
                                            </div>
                                        </td>
                                    </tr>
                                    );
                                })
                        )}
                    </tbody>
                </table>
            </div>

            <div className="mt-4 flex justify-between items-center">
                <button
                    onClick={() => setPage(p => p - 1)}
                    disabled={page === 0}
                    className="bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-600 text-white px-4 py-2 rounded text-sm transition-colors"
                >
                    Previous
                </button>
                <span className="text-sm text-gray-400">
                    Page {page + 1} (showing {deployments.length} deployments)
                </span>
                <button
                    onClick={() => setPage(p => p + 1)}
                    disabled={!hasMore}
                    className="bg-gray-700 hover:bg-gray-600 disabled:bg-gray-800 disabled:text-gray-600 text-white px-4 py-2 rounded text-sm transition-colors"
                >
                    Next
                </button>
            </div>

            <ConfirmDialog
                isOpen={confirmDialogOpen}
                onClose={() => {
                    setConfirmDialogOpen(false);
                    setDeploymentToStop(null);
                }}
                onConfirm={handleStopConfirm}
                title="Stop Deployment"
                message={`Are you sure you want to stop deployment ${deploymentToStop?.deployment_id}? This action will terminate the deployment.`}
                confirmText="Stop Deployment"
                variant="danger"
                loading={stopping}
            />

            <ConfirmDialog
                isOpen={rollbackDialogOpen}
                onClose={() => {
                    setRollbackDialogOpen(false);
                    setDeploymentToRollback(null);
                }}
                onConfirm={handleRollbackConfirm}
                title="Rollback to Deployment"
                message={`Are you sure you want to rollback to deployment ${deploymentToRollback?.deployment_id}? This will create a new deployment with the same image and configuration.`}
                confirmText="Rollback"
                variant="primary"
                loading={rollingBack}
            />
        </div>
    );
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

// Project Detail Component (with tabs)
function ProjectDetail({ projectName, initialTab }) {
    const [project, setProject] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [activeTab, setActiveTab] = useState(initialTab || 'overview');
    const [confirmDialogOpen, setConfirmDialogOpen] = useState(false);
    const [deleting, setDeleting] = useState(false);
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

    // Update activeTab when initialTab changes (e.g., browser back/forward)
    useEffect(() => {
        if (initialTab) {
            setActiveTab(initialTab);
        }
    }, [initialTab]);

    // Helper function to change tab and update URL
    const changeTab = (tab) => {
        setActiveTab(tab);
        window.location.hash = `project/${projectName}/${tab}`;
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
            window.location.hash = 'projects';
        } catch (err) {
            showToast(`Failed to delete project: ${err.message}`, 'error');
        } finally {
            setDeleting(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading project: {error}</p>;
    if (!project) return <p className="text-gray-400">Project not found.</p>;

    return (
        <section>
            <a href="#projects" className="inline-flex items-center gap-2 text-indigo-400 hover:text-indigo-300 mb-6 transition-colors">
                ← Back to Projects
            </a>

            <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 mb-6">
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
                        <dt className="text-gray-400">Status</dt>
                        <dd className="mt-1"><StatusBadge status={project.status} /></dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">Visibility</dt>
                        <dd className="mt-1 text-gray-200">{project.visibility}</dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">URL</dt>
                        <dd className="mt-1">
                            {project.project_url ? <a href={project.project_url} target="_blank" rel="noopener noreferrer" className="text-indigo-400 hover:text-indigo-300">{project.project_url}</a> : '-'}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">Created</dt>
                        <dd className="mt-1 text-gray-200">{formatDate(project.created)}</dd>
                    </div>
                </dl>
            </div>

            <div className="border-b border-gray-800 mb-6">
                <div className="flex gap-8">
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'overview' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => changeTab('overview')}
                    >
                        Overview
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'deployments' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => changeTab('deployments')}
                    >
                        Deployments
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'service-accounts' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => changeTab('service-accounts')}
                    >
                        Service Accounts
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'env-vars' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => changeTab('env-vars')}
                    >
                        Environment Variables
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'domains' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => changeTab('domains')}
                    >
                        Domains
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

// Team Detail Component
function TeamDetail({ teamName, currentUser }) {
    const [team, setTeam] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [newOwnerEmail, setNewOwnerEmail] = useState('');
    const [newMemberEmail, setNewMemberEmail] = useState('');
    const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
    const [deleting, setDeleting] = useState(false);
    const { showToast } = useToast();

    const loadTeam = useCallback(async () => {
        try {
            const data = await api.getTeam(teamName, { expand: 'members,owners' });
            setTeam(data);
        } catch (err) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    }, [teamName]);

    useEffect(() => {
        loadTeam();
    }, [loadTeam]);

    // Check if user can manage this team
    const canManage = currentUser && team && (
        currentUser.is_admin ||
        (team.owners && team.owners.some(o => o.email === currentUser.email))
    );

    // IDP-managed teams can only be managed by admins
    const canEdit = canManage && (!team?.idp_managed || currentUser?.is_admin);

    const handleAddOwner = async () => {
        if (!newOwnerEmail.trim()) {
            showToast('Please enter an email address', 'error');
            return;
        }

        try {
            // Look up user ID by email
            const lookupResult = await api.lookupUsers([newOwnerEmail.trim()]);
            if (!lookupResult.users || lookupResult.users.length === 0) {
                showToast(`User with email ${newOwnerEmail} not found`, 'error');
                return;
            }

            const currentOwnerIds = team.owners?.map(o => o.id) || [];
            const newOwnerId = lookupResult.users[0].id;

            await api.updateTeam(team.id, {
                owners: [...currentOwnerIds, newOwnerId]
            });
            showToast(`Added ${newOwnerEmail} as owner`, 'success');
            setNewOwnerEmail('');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to add owner: ${err.message}`, 'error');
        }
    };

    const handleRemoveOwner = async (ownerId, email) => {
        try {
            const currentOwnerIds = team.owners?.map(o => o.id) || [];
            const updatedOwnerIds = currentOwnerIds.filter(id => id !== ownerId);

            if (updatedOwnerIds.length === 0) {
                showToast('Cannot remove last owner', 'error');
                return;
            }

            await api.updateTeam(team.id, { owners: updatedOwnerIds });
            showToast(`Removed ${email} from owners`, 'success');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to remove owner: ${err.message}`, 'error');
        }
    };

    const handleAddMember = async () => {
        if (!newMemberEmail.trim()) {
            showToast('Please enter an email address', 'error');
            return;
        }

        try {
            // Look up user ID by email
            const lookupResult = await api.lookupUsers([newMemberEmail.trim()]);
            if (!lookupResult.users || lookupResult.users.length === 0) {
                showToast(`User with email ${newMemberEmail} not found`, 'error');
                return;
            }

            const currentMemberIds = team.members?.map(m => m.id) || [];
            const newMemberId = lookupResult.users[0].id;

            await api.updateTeam(team.id, {
                members: [...currentMemberIds, newMemberId]
            });
            showToast(`Added ${newMemberEmail} as member`, 'success');
            setNewMemberEmail('');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to add member: ${err.message}`, 'error');
        }
    };

    const handleRemoveMember = async (memberId, email) => {
        try {
            const currentMemberIds = team.members?.map(m => m.id) || [];
            const updatedMemberIds = currentMemberIds.filter(id => id !== memberId);
            await api.updateTeam(team.id, { members: updatedMemberIds });
            showToast(`Removed ${email} from members`, 'success');
            await loadTeam();
        } catch (err) {
            showToast(`Failed to remove member: ${err.message}`, 'error');
        }
    };

    const handleDeleteTeam = async () => {
        setDeleting(true);
        try {
            await api.deleteTeam(team.id);
            showToast(`Team ${team.name} deleted successfully`, 'success');
            window.location.hash = '#teams';
        } catch (err) {
            showToast(`Failed to delete team: ${err.message}`, 'error');
            setDeleting(false);
        }
    };

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading team: {error}</p>;
    if (!team) return <p className="text-gray-400">Team not found.</p>;

    return (
        <section>
            <a href="#teams" className="inline-flex items-center gap-2 text-indigo-400 hover:text-indigo-300 mb-6 transition-colors">
                ← Back to Teams
            </a>

            <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 mb-6">
                <div className="flex justify-between items-start mb-4">
                    <div>
                        <div className="flex items-center gap-2">
                            <h3 className="text-2xl font-bold">Team {team.name}</h3>
                            {team.idp_managed && (
                                <span className="text-xs bg-purple-600 text-white px-2 py-1 rounded">IDP</span>
                            )}
                        </div>
                    </div>
                    {canEdit && (
                        <Button
                            variant="danger"
                            size="sm"
                            onClick={() => setDeleteDialogOpen(true)}
                        >
                            Delete Team
                        </Button>
                    )}
                </div>
                <dl className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                        <dt className="text-gray-400">Created</dt>
                        <dd className="mt-1 text-gray-200">{formatDate(team.created)}</dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">Updated</dt>
                        <dd className="mt-1 text-gray-200">{formatDate(team.updated)}</dd>
                    </div>
                </dl>
                {team.idp_managed && !currentUser?.is_admin && (
                    <div className="mt-4 p-3 bg-purple-900/20 border border-purple-700 rounded text-sm text-purple-300">
                        This team is managed by your identity provider and can only be modified by administrators.
                    </div>
                )}
            </div>

            <div className="mb-6">
                <div className="flex justify-between items-center mb-4">
                    <h4 className="text-lg font-bold">Owners</h4>
                </div>
                {team.owners && team.owners.length > 0 ? (
                    <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800 mb-4">
                        <table className="w-full">
                            <thead className="bg-gray-800">
                                <tr>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Email</th>
                                    {canEdit && <th className="px-6 py-3 text-right text-xs font-medium text-gray-300 uppercase tracking-wider">Actions</th>}
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-800">
                                {team.owners.map(owner => (
                                    <tr key={owner.id} className="hover:bg-gray-800/50 transition-colors">
                                        <td className="px-6 py-4 text-sm text-gray-200">{owner.email}</td>
                                        {canEdit && (
                                            <td className="px-6 py-4">
                                                <div className="flex justify-end">
                                                    <Button
                                                        variant="danger"
                                                        size="sm"
                                                        onClick={() => handleRemoveOwner(owner.id, owner.email)}
                                                    >
                                                        Remove
                                                    </Button>
                                                </div>
                                            </td>
                                        )}
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                ) : (
                    <p className="text-gray-400 mb-4">No owners</p>
                )}
                {canEdit && (
                    <div className="flex justify-end">
                        <div className="flex gap-2">
                            <input
                                type="email"
                                placeholder="owner@example.com"
                                value={newOwnerEmail}
                                onChange={(e) => setNewOwnerEmail(e.target.value)}
                                onKeyPress={(e) => e.key === 'Enter' && handleAddOwner()}
                                className="w-64 bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-indigo-500"
                            />
                            <Button variant="primary" size="sm" onClick={handleAddOwner}>
                                Add
                            </Button>
                        </div>
                    </div>
                )}
            </div>

            <div>
                <div className="flex justify-between items-center mb-4">
                    <h4 className="text-lg font-bold">Members</h4>
                </div>
                {team.members && team.members.length > 0 ? (
                    <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800 mb-4">
                        <table className="w-full">
                            <thead className="bg-gray-800">
                                <tr>
                                    <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Email</th>
                                    {canEdit && <th className="px-6 py-3 text-right text-xs font-medium text-gray-300 uppercase tracking-wider">Actions</th>}
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-800">
                                {team.members.map(member => (
                                    <tr key={member.id} className="hover:bg-gray-800/50 transition-colors">
                                        <td className="px-6 py-4 text-sm text-gray-200">{member.email}</td>
                                        {canEdit && (
                                            <td className="px-6 py-4">
                                                <div className="flex justify-end">
                                                    <Button
                                                        variant="danger"
                                                        size="sm"
                                                        onClick={() => handleRemoveMember(member.id, member.email)}
                                                    >
                                                        Remove
                                                    </Button>
                                                </div>
                                            </td>
                                        )}
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                ) : (
                    <p className="text-gray-400 mb-4">No members</p>
                )}
                {canEdit && (
                    <div className="flex justify-end">
                        <div className="flex gap-2">
                            <input
                                type="email"
                                placeholder="member@example.com"
                                value={newMemberEmail}
                                onChange={(e) => setNewMemberEmail(e.target.value)}
                                onKeyPress={(e) => e.key === 'Enter' && handleAddMember()}
                                className="w-64 bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-indigo-500"
                            />
                            <Button variant="primary" size="sm" onClick={handleAddMember}>
                                Add
                            </Button>
                        </div>
                    </div>
                )}
            </div>

            {deleteDialogOpen && (
                <Modal
                    isOpen={deleteDialogOpen}
                    onClose={() => setDeleteDialogOpen(false)}
                    title="Delete Team"
                >
                    <div className="mb-6">
                        <p className="text-gray-300 mb-4">
                            Are you sure you want to delete team <strong className="text-white">{team.name}</strong>?
                        </p>
                        <p className="text-sm text-gray-400">
                            This action cannot be undone.
                        </p>
                    </div>
                    <div className="flex justify-end gap-3">
                        <Button
                            variant="secondary"
                            onClick={() => setDeleteDialogOpen(false)}
                            disabled={deleting}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="danger"
                            onClick={handleDeleteTeam}
                            disabled={deleting}
                        >
                            {deleting ? 'Deleting...' : 'Delete Team'}
                        </Button>
                    </div>
                </Modal>
            )}
        </section>
    );
}

// Deployment Detail Component
function DeploymentDetail({ projectName, deploymentId }) {
    const [deployment, setDeployment] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [rollbackDialogOpen, setRollbackDialogOpen] = useState(false);
    const [rolling, setRolling] = useState(false);
    const { showToast } = useToast();

    const loadDeployment = useCallback(async () => {
        try {
            const data = await api.getDeployment(projectName, deploymentId);
            setDeployment(data);
            setLoading(false);
        } catch (err) {
            setError(err.message);
            setLoading(false);
        }
    }, [projectName, deploymentId]);

    const handleRollbackClick = () => {
        setRollbackDialogOpen(true);
    };

    const handleRollback = async () => {
        setRolling(true);
        try {
            const response = await api.rollbackDeployment(projectName, deploymentId);
            showToast(`Rollback successful! New deployment: ${response.new_deployment_id}`, 'success');
            setRollbackDialogOpen(false);
            // Redirect to project page to see the new deployment
            window.location.hash = `project/${projectName}`;
        } catch (err) {
            showToast(`Failed to rollback deployment: ${err.message}`, 'error');
        } finally {
            setRolling(false);
        }
    };

    useEffect(() => {
        loadDeployment();

        // Auto-refresh if deployment is in progress
        const inProgressStatuses = ['Pending', 'Building', 'Pushing', 'Pushed', 'Deploying'];
        if (deployment && inProgressStatuses.includes(deployment.status)) {
            const interval = setInterval(loadDeployment, 3000);
            return () => clearInterval(interval);
        }
    }, [loadDeployment, deployment]);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading deployment: {error}</p>;
    if (!deployment) return <p className="text-gray-400">Deployment not found.</p>;

    return (
        <section>
            <a href={`#project/${projectName}`} className="inline-flex items-center gap-2 text-indigo-400 hover:text-indigo-300 mb-6 transition-colors">
                ← Back
            </a>

            <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 mb-6">
                <div className="flex justify-between items-center mb-4">
                    <h3 className="text-2xl font-bold">Deployment {deployment.deployment_id}</h3>
                    <div className="flex items-center gap-3">
                        <StatusBadge status={deployment.status} />
                        {(deployment.status === 'Healthy' || deployment.status === 'Superseded') && (
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={handleRollbackClick}
                            >
                                Rollback
                            </Button>
                        )}
                    </div>
                </div>
                <dl className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                        <dt className="text-gray-400">Project</dt>
                        <dd className="mt-1 text-gray-200">{deployment.project}</dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">Created by</dt>
                        <dd className="mt-1 text-gray-200">{deployment.created_by_email || '-'}</dd>
                    </div>
                    <div className="col-span-2">
                        <dt className="text-gray-400">Image</dt>
                        <dd className="mt-1 font-mono text-sm text-gray-200">{deployment.image || '-'}</dd>
                    </div>
                    <div className="col-span-2">
                        <dt className="text-gray-400">Image Digest</dt>
                        <dd className="mt-1 font-mono text-xs text-gray-300">{deployment.image_digest || '-'}</dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">Group</dt>
                        <dd className="mt-1 text-gray-200">{deployment.deployment_group}</dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">URL</dt>
                        <dd className="mt-1">
                            {deployment.deployment_url ? <a href={deployment.deployment_url} target="_blank" rel="noopener noreferrer" className="text-indigo-400 hover:text-indigo-300">{deployment.deployment_url}</a> : '-'}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-gray-400">Created</dt>
                        <dd className="mt-1 text-gray-200">{formatDate(deployment.created)}</dd>
                    </div>
                    {deployment.completed_at && (
                        <div>
                            <dt className="text-gray-400">Completed</dt>
                            <dd className="mt-1 text-gray-200">{formatDate(deployment.completed_at)}</dd>
                        </div>
                    )}
                    {deployment.expires_at && (
                        <div>
                            <dt className="text-gray-400">Expires</dt>
                            <dd className="mt-1 text-gray-200">
                                {formatTimeRemaining(deployment.expires_at)}
                                <span className="text-gray-500 text-xs ml-2">({formatDate(deployment.expires_at)})</span>
                            </dd>
                        </div>
                    )}
                    {deployment.error_message && (
                        <div className="col-span-2">
                            <dt className="text-gray-400">Error</dt>
                            <dd className="mt-1 text-red-400">{deployment.error_message}</dd>
                        </div>
                    )}
                </dl>
                {deployment.build_logs && (
                    <details className="mt-4">
                        <summary className="cursor-pointer text-indigo-400 hover:text-indigo-300 font-semibold">Build Logs</summary>
                        <pre className="mt-2 bg-gray-950 border border-gray-800 rounded p-4 overflow-x-auto text-xs">
                            <code className="text-gray-300">{deployment.build_logs}</code>
                        </pre>
                    </details>
                )}
            </div>

            <h3 className="text-xl font-bold mb-4">Environment Variables</h3>
            <EnvVarsList projectName={projectName} deploymentId={deploymentId} />

            <ConfirmDialog
                isOpen={rollbackDialogOpen}
                onClose={() => setRollbackDialogOpen(false)}
                onConfirm={handleRollback}
                title="Rollback to Deployment"
                message={`Are you sure you want to rollback to deployment ${deploymentId}? This will create a new deployment with the same image and configuration.`}
                confirmText="Rollback"
                variant="primary"
                loading={rolling}
            />
        </section>
    );
}

// Toast System
const ToastContext = React.createContext(null);

function ToastProvider({ children }) {
    const [toasts, setToasts] = useState([]);

    const showToast = useCallback((message, type = 'info') => {
        const id = Date.now() + Math.random();
        const toast = { id, message, type };

        setToasts(prev => [...prev, toast]);

        // Auto-dismiss after 4 seconds
        setTimeout(() => {
            setToasts(prev => prev.filter(t => t.id !== id));
        }, 4000);
    }, []);

    const removeToast = useCallback((id) => {
        setToasts(prev => prev.filter(t => t.id !== id));
    }, []);

    return (
        <ToastContext.Provider value={{ showToast }}>
            {children}
            <div className="toast-container">
                {toasts.map(toast => (
                    <Toast key={toast.id} toast={toast} onClose={() => removeToast(toast.id)} />
                ))}
            </div>
        </ToastContext.Provider>
    );
}

function Toast({ toast, onClose }) {
    const typeClasses = {
        success: 'toast-success',
        error: 'toast-error',
        info: 'toast-info',
    };

    return (
        <div className={`toast ${typeClasses[toast.type] || 'toast-info'}`}>
            <div className="toast-content">
                {toast.type === 'success' && (
                    <svg className="toast-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                )}
                {toast.type === 'error' && (
                    <svg className="toast-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                )}
                {toast.type === 'info' && (
                    <svg className="toast-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                )}
                <span className="toast-message">{toast.message}</span>
            </div>
            <button onClick={onClose} className="toast-close">
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
            </button>
        </div>
    );
}

// Hook to use toast from any component
function useToast() {
    const context = React.useContext(ToastContext);
    if (!context) {
        throw new Error('useToast must be used within ToastProvider');
    }
    return context;
}

// Modal Component
function Modal({ isOpen, onClose, title, children, maxWidth = 'max-w-2xl' }) {
    useEffect(() => {
        const handleEscape = (e) => {
            if (e.key === 'Escape' && isOpen) {
                onClose();
            }
        };

        document.addEventListener('keydown', handleEscape);
        return () => document.removeEventListener('keydown', handleEscape);
    }, [isOpen, onClose]);

    useEffect(() => {
        if (isOpen) {
            document.body.style.overflow = 'hidden';
        } else {
            document.body.style.overflow = 'unset';
        }
        return () => {
            document.body.style.overflow = 'unset';
        };
    }, [isOpen]);

    if (!isOpen) return null;

    return (
        <div className="modal-backdrop" onClick={onClose}>
            <div className={`modal-content ${maxWidth}`} onClick={(e) => e.stopPropagation()}>
                <div className="modal-header">
                    <h3 className="modal-title">{title}</h3>
                    <button onClick={onClose} className="modal-close-button">
                        <svg className="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>
                </div>
                <div className="modal-body">
                    {children}
                </div>
            </div>
        </div>
    );
}

// Button Component
function Button({
    children,
    onClick,
    variant = 'primary',
    size = 'md',
    loading = false,
    disabled = false,
    type = 'button',
    className = ''
}) {
    const baseClasses = 'font-semibold rounded transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-900 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2';

    const variantClasses = {
        primary: 'bg-indigo-600 hover:bg-indigo-700 text-white focus:ring-indigo-500',
        secondary: 'bg-gray-600 hover:bg-gray-700 text-white focus:ring-gray-500',
        danger: 'bg-red-600 hover:bg-red-700 text-white focus:ring-red-500',
    };

    const sizeClasses = {
        sm: 'px-3 py-1.5 text-sm',
        md: 'px-4 py-2 text-sm',
        lg: 'px-6 py-3 text-base',
    };

    return (
        <button
            type={type}
            onClick={onClick}
            disabled={disabled || loading}
            className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
        >
            {loading && (
                <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin"></div>
            )}
            {children}
        </button>
    );
}

// FormField Component
function FormField({
    label,
    id,
    type = 'text',
    value,
    onChange,
    error,
    required = false,
    placeholder,
    disabled = false,
    options = [],
    rows = 3,
    children
}) {
    const inputClasses = `w-full bg-gray-800 border ${error ? 'border-red-500' : 'border-gray-700'} rounded px-3 py-2 text-gray-100 placeholder-gray-500 focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed`;

    return (
        <div className="form-field">
            <label htmlFor={id} className="block text-sm font-medium text-gray-300 mb-2">
                {label}
                {required && <span className="text-red-500 ml-1">*</span>}
            </label>

            {type === 'select' ? (
                <select
                    id={id}
                    value={value}
                    onChange={onChange}
                    disabled={disabled}
                    className={inputClasses}
                >
                    {children ? children : options.map(opt => (
                        <option key={opt.value} value={opt.value}>
                            {opt.label}
                        </option>
                    ))}
                </select>
            ) : type === 'textarea' ? (
                <textarea
                    id={id}
                    value={value}
                    onChange={onChange}
                    placeholder={placeholder}
                    disabled={disabled}
                    rows={rows}
                    className={inputClasses}
                />
            ) : type === 'checkbox' ? (
                <div className="flex items-center">
                    <input
                        type="checkbox"
                        id={id}
                        checked={value}
                        onChange={onChange}
                        disabled={disabled}
                        className="w-4 h-4 bg-gray-800 border-gray-700 rounded text-indigo-600 focus:ring-indigo-500 focus:ring-offset-gray-900"
                    />
                    <label htmlFor={id} className="ml-2 text-sm text-gray-300">
                        {placeholder}
                    </label>
                </div>
            ) : (
                <input
                    type={type}
                    id={id}
                    value={value}
                    onChange={onChange}
                    placeholder={placeholder}
                    disabled={disabled}
                    className={inputClasses}
                />
            )}

            {error && (
                <p className="mt-2 text-sm text-red-500">{error}</p>
            )}
        </div>
    );
}

// ConfirmDialog Component
function ConfirmDialog({
    isOpen,
    onClose,
    onConfirm,
    title,
    message,
    confirmText = 'Confirm',
    cancelText = 'Cancel',
    variant = 'danger',
    requireConfirmation = false,
    confirmationText = '',
    loading = false
}) {
    const [inputValue, setInputValue] = useState('');
    const [error, setError] = useState('');

    const handleConfirm = () => {
        if (requireConfirmation && inputValue !== confirmationText) {
            setError(`Please type "${confirmationText}" to confirm`);
            return;
        }
        onConfirm();
    };

    const handleClose = () => {
        setInputValue('');
        setError('');
        onClose();
    };

    useEffect(() => {
        if (!isOpen) {
            setInputValue('');
            setError('');
        }
    }, [isOpen]);

    const isConfirmEnabled = !requireConfirmation || inputValue === confirmationText;

    return (
        <Modal isOpen={isOpen} onClose={handleClose} title={title} maxWidth="max-w-md">
            <div className="space-y-4">
                <p className="text-gray-300">{message}</p>

                {requireConfirmation && (
                    <FormField
                        label={`Type "${confirmationText}" to confirm`}
                        id="confirm-input"
                        value={inputValue}
                        onChange={(e) => setInputValue(e.target.value)}
                        error={error}
                        placeholder={confirmationText}
                    />
                )}

                <div className="flex justify-end gap-3 pt-4">
                    <Button
                        variant="secondary"
                        onClick={handleClose}
                        disabled={loading}
                    >
                        {cancelText}
                    </Button>
                    <Button
                        variant={variant}
                        onClick={handleConfirm}
                        disabled={!isConfirmEnabled}
                        loading={loading}
                    >
                        {confirmText}
                    </Button>
                </div>
            </div>
        </Modal>
    );
}

// Login Page Component
function LoginPage() {
    const [status, setStatus] = useState('');
    const [loading, setLoading] = useState(false);

    // Handle OAuth callback on component mount
    useEffect(() => {
        const params = new URLSearchParams(window.location.search);
        if (params.has('code')) {
            setStatus('Processing authentication...');
            setLoading(true);
            handleOAuthCallback()
                .catch((error) => {
                    setStatus(`Error: ${error.message}`);
                    setLoading(false);
                });
        }
    }, []);

    const handleLogin = async () => {
        setStatus('Initializing authentication...');
        setLoading(true);
        try {
            await login();
        } catch (error) {
            setStatus(`Error: ${error.message}`);
            setLoading(false);
        }
    };

    return (
        <div className="flex items-center justify-center min-h-screen bg-gradient-to-br from-gray-900 via-gray-950 to-black">
            <div className="w-full max-w-md p-8 bg-gray-900 rounded-lg border border-gray-800 shadow-2xl">
                <div className="text-center mb-8">
                    <div className="flex justify-center mb-4">
                        <svg className="w-16 h-16 text-indigo-500" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                        </svg>
                    </div>
                    <h1 className="text-3xl font-bold text-white mb-2">Rise</h1>
                    <p className="text-gray-400">Container Deployment Platform</p>
                </div>

                {loading ? (
                    <div className="flex flex-col items-center gap-4 py-8">
                        <div className="w-12 h-12 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
                        <p className="text-gray-300">{status}</p>
                    </div>
                ) : (
                    <>
                        <button
                            onClick={handleLogin}
                            className="w-full bg-indigo-600 hover:bg-indigo-700 text-white font-semibold py-3 px-4 rounded-lg transition-colors mb-4"
                        >
                            Login with OAuth
                        </button>
                        {status && (
                            <p className="text-center text-sm text-red-400">{status}</p>
                        )}
                    </>
                )}
            </div>
        </div>
    );
}

// Main App Component
function App() {
    const [user, setUser] = useState(null);
    const [authChecked, setAuthChecked] = useState(false);
    const hash = useHashLocation();

    useEffect(() => {
        // Check if we're handling OAuth callback
        const params = new URLSearchParams(window.location.search);
        if (params.has('code')) {
            // Let LoginPage handle the callback
            setAuthChecked(true);
            return;
        }

        if (!isAuthenticated()) {
            setAuthChecked(true);
            return;
        }

        async function loadUser() {
            try {
                const userData = await api.getMe();
                setUser(userData);
            } catch (err) {
                console.error('Failed to load user:', err);
                logout();
            } finally {
                setAuthChecked(true);
            }
        }
        loadUser();
    }, []);

    const handleLogout = () => {
        logout();
    };

    if (!authChecked) {
        return (
            <div className="flex items-center justify-center min-h-screen">
                <div className="w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
            </div>
        );
    }

    if (!isAuthenticated() || !user) {
        return <LoginPage />;
    }

    // Parse hash for routing
    let view = 'projects';
    let params = {};

    if (hash.startsWith('project/')) {
        view = 'project-detail';
        const parts = hash.split('/');
        params.projectName = parts[1];
        params.tab = parts[2] || 'overview'; // Default to overview if no tab specified
    } else if (hash.startsWith('team/')) {
        view = 'team-detail';
        params.teamName = hash.split('/')[1];
    } else if (hash.startsWith('deployment/')) {
        view = 'deployment-detail';
        const parts = hash.split('/');
        params.projectName = parts[1];
        params.deploymentId = parts[2];
    } else if (hash === 'teams') {
        view = 'teams';
    } else {
        view = 'projects';
    }

    return (
        <>
            <Header user={user} onLogout={handleLogout} currentView={view} />
            <main className="container mx-auto px-4 py-8">
                {view === 'projects' && <ProjectsList />}
                {view === 'teams' && <TeamsList currentUser={user} />}
                {view === 'project-detail' && <ProjectDetail projectName={params.projectName} initialTab={params.tab} />}
                {view === 'team-detail' && <TeamDetail teamName={params.teamName} currentUser={user} />}
                {view === 'deployment-detail' && <DeploymentDetail projectName={params.projectName} deploymentId={params.deploymentId} />}
            </main>
        </>
    );
}

// Initialize the React app
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(
    <ToastProvider>
        <App />
    </ToastProvider>
);
 