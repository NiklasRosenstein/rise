// Deployment-related components for Rise Dashboard
// This file depends on React, utils.js, components/ui.js, and components/toast.js being loaded first

const { useState, useEffect, useCallback, useRef } = React;

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

            // Group deployments by deployment group
            const grouped = deployments.reduce((acc, d) => {
                const group = d.deployment_group || 'default';
                if (!acc[group]) {
                    acc[group] = {
                        active: null,
                        progressing: []
                    };
                }

                // Track the active deployment (is_active === true)
                if (d.is_active) {
                    acc[group].active = d;
                }

                // Track progressing (non-terminal) deployments
                if (!isTerminal(d.status)) {
                    acc[group].progressing.push(d);
                }

                return acc;
            }, {});

            // Filter to only include groups that have an active deployment or progressing deployments
            const filtered = {};
            Object.keys(grouped).forEach(group => {
                const groupData = grouped[group];
                // Always include default group if it has an active deployment
                // Include other groups if they have active OR progressing deployments
                if (groupData.active || (group !== 'default' && groupData.progressing.length > 0)) {
                    filtered[group] = groupData;
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

    // Sort groups: "default" first, then by active deployment's created timestamp
    const sortedGroups = groups.sort((a, b) => {
        if (a === 'default') return -1;
        if (b === 'default') return 1;

        // Both non-default: sort by active deployment's created timestamp (descending)
        const activeA = activeDeployments[a].active;
        const activeB = activeDeployments[b].active;

        // If both have active deployments, sort by created timestamp
        if (activeA && activeB) {
            return new Date(activeB.created) - new Date(activeA.created);
        }

        // Groups with active deployments come first
        if (activeA && !activeB) return -1;
        if (!activeA && activeB) return 1;

        return 0;
    });

    return (
        <>
            <div className="space-y-4">
                {sortedGroups.map(group => {
                    const groupData = activeDeployments[group];
                    const deployment = groupData.active;

                    // Skip if no active deployment (shouldn't happen due to filtering, but be safe)
                    if (!deployment) {
                        return null;
                    }

                    const canStop = !isTerminal(deployment.status);
                    // Count other progressing deployments (exclude the active one)
                    const otherProgressing = groupData.progressing.filter(d => d.deployment_id !== deployment.deployment_id).length;

                    return (
                        <div key={group} className="bg-gray-900 border border-gray-800 rounded-lg p-6">
                            <div className="flex justify-between items-center mb-4">
                                <h5 className="text-lg font-semibold">Group: {group}</h5>
                                <div className="flex items-center gap-3">
                                    <StatusBadge status={deployment.status} />
                                    {canStop && (
                                        <Button
                                            variant="danger"
                                            size="sm"
                                            onClick={() => handleStopClick(deployment)}
                                        >
                                            Stop
                                        </Button>
                                    )}
                                </div>
                            </div>
                        <dl className="grid grid-cols-2 gap-4 text-sm">
                            <div>
                                <dt className="text-gray-400">Deployment ID</dt>
                                <dd className="font-mono text-gray-200">{deployment.deployment_id}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-400">Image</dt>
                                <dd className="font-mono text-gray-200 text-xs">{deployment.image ? deployment.image.split('/').pop() : '-'}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-400">URL</dt>
                                <dd>{deployment.primary_url ? <a href={deployment.primary_url} target="_blank" rel="noopener noreferrer" className="text-indigo-400 hover:text-indigo-300">{deployment.primary_url}</a> : '-'}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-400">Created</dt>
                                <dd className="text-gray-200">{formatDate(deployment.created)}</dd>
                            </div>
                            {deployment.expires_at && (
                                <div>
                                    <dt className="text-gray-400">Expires</dt>
                                    <dd className="text-gray-200">
                                        {formatTimeRemaining(deployment.expires_at)}
                                        <span className="text-gray-500 text-xs ml-2">({formatDate(deployment.expires_at)})</span>
                                    </dd>
                                </div>
                            )}
                        </dl>
                        <div className="mt-4 pt-4 border-t border-gray-800 flex items-center justify-between">
                            <a href={`#deployment/${projectName}/${deployment.deployment_id}`} className="text-indigo-400 hover:text-indigo-300">
                                View Details
                            </a>
                            {otherProgressing > 0 && (
                                <span className="text-sm text-gray-500">
                                    +{otherProgressing} other{otherProgressing === 1 ? '' : 's'} progressing
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
                                            {d.primary_url ? (
                                                <a
                                                    href={d.primary_url}
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
                                                        {d.is_active ? 'Redeploy' : 'Rollback'}
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
                title={deploymentToRollback?.is_active ? 'Redeploy' : 'Rollback to Deployment'}
                message={deploymentToRollback?.is_active
                    ? `Are you sure you want to redeploy ${deploymentToRollback?.deployment_id}? This will create a new deployment with the same image and configuration.`
                    : `Are you sure you want to rollback to deployment ${deploymentToRollback?.deployment_id}? This will create a new deployment with the same image and configuration.`}
                confirmText={deploymentToRollback?.is_active ? 'Redeploy' : 'Rollback'}
                variant="primary"
                loading={rollingBack}
            />
        </div>
    );
}

// Deployment Logs Component with SSE streaming
function DeploymentLogs({ projectName, deploymentId, deploymentStatus }) {
    const [logs, setLogs] = useState([]);
    const [streaming, setStreaming] = useState(false);
    const [error, setError] = useState(null);
    const [autoScroll, setAutoScroll] = useState(true);
    const logsEndRef = useRef(null);
    const eventSourceRef = useRef(null);

    const isLoggable = (status) => {
        // Can view logs for deployments that are running or have run
        return ['Deploying', 'Healthy', 'Unhealthy', 'Stopped', 'Failed', 'Superseded'].includes(status);
    };

    const scrollToBottom = () => {
        if (autoScroll && logsEndRef.current) {
            logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
        }
    };

    useEffect(() => {
        scrollToBottom();
    }, [logs]);

    const startStreaming = useCallback(() => {
        if (eventSourceRef.current) {
            eventSourceRef.current.close();
        }

        setError(null);
        setStreaming(true);

        const token = localStorage.getItem('rise_token');
        const baseUrl = window.API_BASE_URL || '';
        const url = `${baseUrl}/projects/${projectName}/deployments/${deploymentId}/logs?follow=true`;

        // Use fetch for SSE with authorization header
        fetch(url, {
            headers: {
                'Authorization': `Bearer ${token}`,
                'Accept': 'text/event-stream',
            },
        })
        .then(response => {
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }

            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            let buffer = '';

            const processStream = () => {
                reader.read().then(({ done, value }) => {
                    if (done) {
                        setStreaming(false);
                        return;
                    }

                    buffer += decoder.decode(value, { stream: true });
                    const lines = buffer.split('\n');
                    buffer = lines.pop(); // Keep incomplete line in buffer

                    lines.forEach(line => {
                        if (line.startsWith('data: ')) {
                            const logLine = line.substring(6); // Remove 'data: ' prefix
                            if (logLine.trim()) {
                                setLogs(prevLogs => [...prevLogs, logLine]);
                            }
                        }
                    });

                    processStream();
                }).catch(err => {
                    console.error('Stream error:', err);
                    setError(err.message);
                    setStreaming(false);
                });
            };

            processStream();
        })
        .catch(err => {
            console.error('Failed to start log stream:', err);
            setError(err.message);
            setStreaming(false);
        });
    }, [projectName, deploymentId]);

    const stopStreaming = useCallback(() => {
        if (eventSourceRef.current) {
            eventSourceRef.current.close();
            eventSourceRef.current = null;
        }
        setStreaming(false);
    }, []);

    const loadInitialLogs = useCallback(async () => {
        const token = localStorage.getItem('rise_token');
        const baseUrl = window.API_BASE_URL || '';
        const url = `${baseUrl}/projects/${projectName}/deployments/${deploymentId}/logs`;

        try {
            const response = await fetch(url, {
                headers: {
                    'Authorization': `Bearer ${token}`,
                    'Accept': 'text/event-stream',
                },
            });

            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }

            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            let buffer = '';
            const newLogs = [];

            while (true) {
                const { done, value } = await reader.read();
                if (done) break;

                buffer += decoder.decode(value, { stream: true });
                const lines = buffer.split('\n');
                buffer = lines.pop();

                lines.forEach(line => {
                    if (line.startsWith('data: ')) {
                        const logLine = line.substring(6);
                        if (logLine.trim()) {
                            newLogs.push(logLine);
                        }
                    }
                });
            }

            setLogs(newLogs);
        } catch (err) {
            console.error('Failed to load logs:', err);
            setError(err.message);
        }
    }, [projectName, deploymentId]);

    const clearLogs = () => {
        setLogs([]);
    };

    useEffect(() => {
        return () => {
            stopStreaming();
        };
    }, [stopStreaming]);

    if (!isLoggable(deploymentStatus)) {
        return null;
    }

    return (
        <div className="mb-6">
            <div className="flex justify-between items-center mb-3">
                <h3 className="text-xl font-bold">Runtime Logs</h3>
                <div className="flex gap-2">
                    <label className="flex items-center gap-2 text-sm text-gray-400">
                        <input
                            type="checkbox"
                            checked={autoScroll}
                            onChange={(e) => setAutoScroll(e.target.checked)}
                            className="rounded border-gray-600 bg-gray-800 text-indigo-600 focus:ring-indigo-500"
                        />
                        Auto-scroll
                    </label>
                    <Button
                        variant="secondary"
                        size="sm"
                        onClick={clearLogs}
                        disabled={logs.length === 0}
                    >
                        Clear
                    </Button>
                    {!streaming ? (
                        <>
                            <Button
                                variant="secondary"
                                size="sm"
                                onClick={loadInitialLogs}
                            >
                                Load Logs
                            </Button>
                            <Button
                                variant="primary"
                                size="sm"
                                onClick={startStreaming}
                            >
                                Follow Logs
                            </Button>
                        </>
                    ) : (
                        <Button
                            variant="secondary"
                            size="sm"
                            onClick={stopStreaming}
                        >
                            Stop
                        </Button>
                    )}
                </div>
            </div>

            {error && (
                <div className="mb-3 p-3 bg-red-900/20 border border-red-800 rounded text-red-400 text-sm">
                    Error: {error}
                </div>
            )}

            <div className="bg-gray-950 border border-gray-800 rounded-lg overflow-hidden">
                <div
                    className="p-4 overflow-y-auto font-mono text-xs text-gray-300"
                    style={{ height: '400px' }}
                >
                    {logs.length === 0 ? (
                        <div className="text-gray-500 text-center py-8">
                            {streaming ? 'Waiting for logs...' : 'No logs yet. Click "Load Logs" or "Follow Logs" to view.'}
                        </div>
                    ) : (
                        <>
                            {logs.map((log, idx) => (
                                <div key={idx} className="whitespace-pre-wrap break-all">
                                    {log}
                                </div>
                            ))}
                            <div ref={logsEndRef} />
                        </>
                    )}
                </div>
            </div>

            {streaming && (
                <div className="mt-2 flex items-center gap-2 text-sm text-gray-400">
                    <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse"></div>
                    Live streaming logs...
                </div>
            )}
        </div>
    );
}

function DeploymentDetail({ projectName, deploymentId }) {
    const [deployment, setDeployment] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [rollbackDialogOpen, setRollbackDialogOpen] = useState(false);
    const [rolling, setRolling] = useState(false);
    const { showToast } = useToast();

    const isTerminal = (status) => {
        return ['Cancelled', 'Stopped', 'Superseded', 'Failed', 'Expired'].includes(status);
    };

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
    }, [loadDeployment]);

    // Auto-refresh only if deployment is not in a terminal state
    useEffect(() => {
        if (deployment && !isTerminal(deployment.status)) {
            const interval = setInterval(loadDeployment, 5000);
            return () => clearInterval(interval);
        }
    }, [deployment?.status, loadDeployment]);

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
                                {deployment.is_active ? 'Redeploy' : 'Rollback'}
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
                        <dt className="text-gray-400">URLs</dt>
                        <dd className="mt-1 space-y-1">
                            {deployment.primary_url ? (
                                <>
                                    <div>
                                        <a href={deployment.primary_url} target="_blank" rel="noopener noreferrer" className="text-indigo-400 hover:text-indigo-300">{deployment.primary_url}</a>
                                    </div>
                                    {deployment.custom_domain_urls && deployment.custom_domain_urls.map((url, idx) => (
                                        <div key={idx}>
                                            <a href={url} target="_blank" rel="noopener noreferrer" className="text-indigo-400 hover:text-indigo-300">{url}</a>
                                        </div>
                                    ))}
                                </>
                            ) : (
                                <span className="text-gray-500">-</span>
                            )}
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

            <DeploymentLogs projectName={projectName} deploymentId={deploymentId} deploymentStatus={deployment.status} />

            <h3 className="text-xl font-bold mb-4">Environment Variables</h3>
            <EnvVarsList projectName={projectName} deploymentId={deploymentId} />

            <ConfirmDialog
                isOpen={rollbackDialogOpen}
                onClose={() => setRollbackDialogOpen(false)}
                onConfirm={handleRollback}
                title={deployment?.is_active ? 'Redeploy' : 'Rollback to Deployment'}
                message={deployment?.is_active
                    ? `Are you sure you want to redeploy ${deploymentId}? This will create a new deployment with the same image and configuration.`
                    : `Are you sure you want to rollback to deployment ${deploymentId}? This will create a new deployment with the same image and configuration.`}
                confirmText={deployment?.is_active ? 'Redeploy' : 'Rollback'}
                variant="primary"
                loading={rolling}
            />
        </section>
    );
}
