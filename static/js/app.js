// React-based Rise Dashboard Application with Tailwind CSS
const { useState, useEffect, useCallback } = React;

// Utility functions
function formatDate(dateString) {
    const date = new Date(dateString);
    return date.toLocaleString();
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
function Header({ user, onLogout }) {
    return (
        <header className="bg-gray-900 border-b border-gray-800">
            <nav className="container mx-auto px-4 py-4">
                <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                        <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                            <path d="M12 2L2 7L12 12L22 7L12 2Z" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 17L12 22L22 17" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                            <path d="M2 12L12 17L22 12" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
                        </svg>
                        <strong className="text-lg font-bold">Rise Dashboard</strong>
                    </div>
                    <div className="flex items-center gap-6">
                        <a href="#projects" className="text-gray-300 hover:text-white transition-colors">Projects</a>
                        <a href="#teams" className="text-gray-300 hover:text-white transition-colors">Teams</a>
                        <span className="text-gray-400">{user?.email}</span>
                        <a href="#" onClick={(e) => { e.preventDefault(); onLogout(); }} className="text-red-400 hover:text-red-300 transition-colors">
                            Logout
                        </a>
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

    useEffect(() => {
        async function loadProjects() {
            try {
                const data = await api.getProjects();
                setProjects(data);
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        loadProjects();
    }, []);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading projects: {error}</p>;
    if (projects.length === 0) return <p className="text-gray-400">No projects found.</p>;

    return (
        <section>
            <h2 className="text-2xl font-bold mb-6">Projects</h2>
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
                        {projects.map(p => {
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
                        })}
                    </tbody>
                </table>
            </div>
        </section>
    );
}

// Teams List Component
function TeamsList() {
    const [teams, setTeams] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    useEffect(() => {
        async function loadTeams() {
            try {
                const data = await api.getTeams();
                setTeams(data);
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        loadTeams();
    }, []);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading teams: {error}</p>;
    if (teams.length === 0) return <p className="text-gray-400">No teams found.</p>;

    return (
        <section>
            <h2 className="text-2xl font-bold mb-6">Teams</h2>
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
                        {teams.map(t => (
                            <tr
                                key={t.id}
                                onClick={() => window.location.hash = `team/${t.name}`}
                                className="hover:bg-gray-800/50 transition-colors cursor-pointer"
                            >
                                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-100">{t.name}</td>
                                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{t.members.length}</td>
                                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{t.owners.length}</td>
                                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{formatDate(t.created)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </section>
    );
}

// Active Deployments Summary Component
function ActiveDeploymentsSummary({ projectName }) {
    const [activeDeployments, setActiveDeployments] = useState({});
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    const loadSummary = useCallback(async () => {
        try {
            const deployments = await api.getProjectDeployments(projectName, { limit: 100 });
            const activeStatuses = ['Pending', 'Building', 'Pushing', 'Pushed', 'Deploying', 'Running', 'Healthy'];
            const active = deployments.filter(d => activeStatuses.includes(d.status));

            // Group by deployment group
            const grouped = active.reduce((acc, d) => {
                const group = d.deployment_group || 'default';
                if (!acc[group]) acc[group] = [];
                acc[group].push(d);
                return acc;
            }, {});

            // Sort each group by created date
            Object.keys(grouped).forEach(group => {
                grouped[group].sort((a, b) => new Date(b.created) - new Date(a.created));
            });

            setActiveDeployments(grouped);
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
        <div className="space-y-4">
            {sortedGroups.map(group => {
                const deps = activeDeployments[group];
                const latest = deps[0];
                return (
                    <div key={group} className="bg-gray-900 border border-gray-800 rounded-lg p-6">
                        <div className="flex justify-between items-center mb-4">
                            <h5 className="text-lg font-semibold">Group: {group}</h5>
                            <StatusBadge status={latest.status} />
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
                                    <dd className="text-gray-200">{formatDate(latest.expires_at)}</dd>
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

    if (loading && deployments.length === 0) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading deployments: {error}</p>;

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

            {deployments.length === 0 ? (
                <p className="text-gray-400">No deployments found.</p>
            ) : (
                <>
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
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-gray-800">
                                {deployments.map(d => (
                                    <tr
                                        key={d.id}
                                        onClick={() => window.location.hash = `deployment/${projectName}/${d.deployment_id}`}
                                        className="hover:bg-gray-800/50 transition-colors cursor-pointer"
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
                                        <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{d.expires_at ? formatDate(d.expires_at) : '-'}</td>
                                        <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{formatDate(d.created)}</td>
                                    </tr>
                                ))}
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
                </>
            )}
        </div>
    );
}

// Service Accounts Component
function ServiceAccountsList({ projectName }) {
    const [serviceAccounts, setServiceAccounts] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    useEffect(() => {
        async function loadServiceAccounts() {
            try {
                const response = await api.getProjectServiceAccounts(projectName);
                setServiceAccounts(response.workload_identities || []);
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        loadServiceAccounts();
    }, [projectName]);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading service accounts: {error}</p>;
    if (serviceAccounts.length === 0) return <p className="text-gray-400">No service accounts found.</p>;

    return (
        <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
            <table className="w-full">
                <thead className="bg-gray-800">
                    <tr>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Email</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Issuer URL</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Claims</th>
                        <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Created</th>
                    </tr>
                </thead>
                <tbody className="divide-y divide-gray-800">
                    {serviceAccounts.map(sa => (
                        <tr key={sa.id} className="hover:bg-gray-800/50 transition-colors">
                            <td className="px-6 py-4 text-sm text-gray-200">{sa.email}</td>
                            <td className="px-6 py-4 text-sm text-gray-300 break-all max-w-xs">{sa.issuer_url}</td>
                            <td className="px-6 py-4 text-xs font-mono text-gray-300">
                                {Object.entries(sa.claims || {})
                                    .map(([key, value]) => `${key}=${value}`)
                                    .join(', ')}
                            </td>
                            <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-300">{formatDate(sa.created_at)}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}

// Environment Variables Component
function EnvVarsList({ projectName, deploymentId }) {
    const [envVars, setEnvVars] = useState([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    useEffect(() => {
        async function loadEnvVars() {
            try {
                const response = deploymentId
                    ? await api.getDeploymentEnvVars(projectName, deploymentId)
                    : await api.getProjectEnvVars(projectName);
                setEnvVars(response.env_vars || []);
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        loadEnvVars();
    }, [projectName, deploymentId]);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading environment variables: {error}</p>;
    if (envVars.length === 0) return <p className="text-gray-400">No environment variables configured.</p>;

    return (
        <div>
            <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                <table className="w-full">
                    <thead className="bg-gray-800">
                        <tr>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Key</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Value</th>
                            <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Type</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-800">
                        {envVars.map(env => (
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
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
            {deploymentId && (
                <p className="mt-4 text-sm text-gray-500">
                    <strong>Note:</strong> Environment variables are read-only snapshots taken at deployment time.
                    Secret values are always masked for security.
                </p>
            )}
        </div>
    );
}

// Project Detail Component (with tabs)
function ProjectDetail({ projectName }) {
    const [project, setProject] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [activeTab, setActiveTab] = useState('overview');

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

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading project: {error}</p>;
    if (!project) return <p className="text-gray-400">Project not found.</p>;

    return (
        <section>
            <a href="#projects" className="inline-flex items-center gap-2 text-indigo-400 hover:text-indigo-300 mb-6 transition-colors">
                ← Back to Projects
            </a>

            <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 mb-6">
                <h3 className="text-2xl font-bold mb-4">Project {project.name}</h3>
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
                        onClick={() => setActiveTab('overview')}
                    >
                        Overview
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'deployments' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => setActiveTab('deployments')}
                    >
                        Deployments
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'service-accounts' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => setActiveTab('service-accounts')}
                    >
                        Service Accounts
                    </button>
                    <button
                        className={`pb-4 px-2 border-b-2 transition-colors cursor-pointer ${activeTab === 'env-vars' ? 'border-indigo-500 text-white' : 'border-transparent text-gray-400 hover:text-gray-300'}`}
                        onClick={() => setActiveTab('env-vars')}
                    >
                        Environment Variables
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
            </div>
        </section>
    );
}

// Team Detail Component
function TeamDetail({ teamName }) {
    const [team, setTeam] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    useEffect(() => {
        async function loadTeam() {
            try {
                const data = await api.getTeam(teamName, { expand: 'members,owners' });
                setTeam(data);
            } catch (err) {
                setError(err.message);
            } finally {
                setLoading(false);
            }
        }
        loadTeam();
    }, [teamName]);

    if (loading) return <div className="text-center py-8"><div className="inline-block w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div></div>;
    if (error) return <p className="text-red-400">Error loading team: {error}</p>;
    if (!team) return <p className="text-gray-400">Team not found.</p>;

    return (
        <section>
            <a href="#teams" className="inline-flex items-center gap-2 text-indigo-400 hover:text-indigo-300 mb-6 transition-colors">
                ← Back to Teams
            </a>

            <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 mb-6">
                <h3 className="text-2xl font-bold mb-4">Team {team.name}</h3>
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
            </div>

            <h4 className="text-lg font-bold mb-4">Owners</h4>
            {team.owners && team.owners.length > 0 ? (
                <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800 mb-6">
                    <table className="w-full">
                        <thead className="bg-gray-800">
                            <tr>
                                <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Email</th>
                            </tr>
                        </thead>
                        <tbody className="divide-y divide-gray-800">
                            {team.owners.map(owner => (
                                <tr key={owner.id} className="hover:bg-gray-800/50 transition-colors">
                                    <td className="px-6 py-4 text-sm text-gray-200">{owner.email}</td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            ) : (
                <p className="text-gray-400 mb-6">No owners</p>
            )}

            <h4 className="text-lg font-bold mb-4">Members</h4>
            {team.members && team.members.length > 0 ? (
                <div className="bg-gray-900 rounded-lg overflow-hidden border border-gray-800">
                    <table className="w-full">
                        <thead className="bg-gray-800">
                            <tr>
                                <th className="px-6 py-3 text-left text-xs font-medium text-gray-300 uppercase tracking-wider">Email</th>
                            </tr>
                        </thead>
                        <tbody className="divide-y divide-gray-800">
                            {team.members.map(member => (
                                <tr key={member.id} className="hover:bg-gray-800/50 transition-colors">
                                    <td className="px-6 py-4 text-sm text-gray-200">{member.email}</td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            ) : (
                <p className="text-gray-400">No members</p>
            )}
        </section>
    );
}

// Deployment Detail Component
function DeploymentDetail({ projectName, deploymentId }) {
    const [deployment, setDeployment] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

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
                    <StatusBadge status={deployment.status} />
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
        </section>
    );
}

// Main App Component
function App() {
    const [user, setUser] = useState(null);
    const hash = useHashLocation();

    useEffect(() => {
        if (!isAuthenticated()) {
            window.location.href = '/';
            return;
        }

        async function loadUser() {
            try {
                const userData = await api.getMe();
                setUser(userData);
            } catch (err) {
                console.error('Failed to load user:', err);
                logout();
            }
        }
        loadUser();
    }, []);

    const handleLogout = () => {
        logout();
    };

    if (!user) {
        return (
            <div className="flex items-center justify-center min-h-screen">
                <div className="w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
            </div>
        );
    }

    // Parse hash for routing
    let view = 'projects';
    let params = {};

    if (hash.startsWith('project/')) {
        view = 'project-detail';
        params.projectName = hash.split('/')[1];
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
            <Header user={user} onLogout={handleLogout} />
            <main className="container mx-auto px-4 py-8">
                {view === 'projects' && <ProjectsList />}
                {view === 'teams' && <TeamsList />}
                {view === 'project-detail' && <ProjectDetail projectName={params.projectName} />}
                {view === 'team-detail' && <TeamDetail teamName={params.teamName} />}
                {view === 'deployment-detail' && <DeploymentDetail projectName={params.projectName} deploymentId={params.deploymentId} />}
            </main>
        </>
    );
}

// Initialize the React app
const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(<App />);
