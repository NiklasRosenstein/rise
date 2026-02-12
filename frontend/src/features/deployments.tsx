// @ts-nocheck
import { Fragment, useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../lib/api';
import { navigate } from '../lib/navigation';
import { copyToClipboard, formatDate, formatISO8601, formatRelativeTimeRounded, formatTimeRemaining } from '../lib/utils';
import { useToast } from '../components/toast';
import { Button, ConfirmDialog, Modal, ModalActions, ModalSection, StatusBadge } from '../components/ui';
import { MonoSortButton, MonoTable, MonoTableBody, MonoTableEmptyRow, MonoTableFrame, MonoTableHead, MonoTableRow, MonoTd, MonoTh } from '../components/table';
import { EnvVarsList } from './resources';
import { EmptyState, ErrorState, LoadingState } from '../components/states';
import { useRowKeyboardNavigation, useSortableData } from '../lib/table';

const STATUS_TONES = {
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

function getStatusTone(status) {
    return STATUS_TONES[status] || 'muted';
}


export function ActiveDeploymentsSummary({ projectName }) {
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

    if (loading) return <LoadingState label="Loading active deployments..." />;
    if (error) return <ErrorState message={`Error loading active deployments: ${error}`} onRetry={loadSummary} />;

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
    if (groups.length === 0) return <EmptyState message="No active deployments." />;

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
            <div className="mono-active-deployments-grid grid gap-4 md:grid-cols-2">
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
                        <div
                            key={group}
                            className={`mono-active-deployment-card mono-status-card mono-status-card-${getStatusTone(deployment.status)} border border-gray-200 dark:border-gray-800 p-6`}
                            onClick={() => navigate(`/deployment/${projectName}/${deployment.deployment_id}`)}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter' || e.key === ' ') {
                                    e.preventDefault();
                                    navigate(`/deployment/${projectName}/${deployment.deployment_id}`);
                                }
                            }}
                            role="link"
                            tabIndex={0}
                            aria-label={`View deployment ${deployment.deployment_id}`}
                        >
                            <div className="flex justify-between items-center mb-4">
                                <h5 className="text-lg font-semibold">{group}</h5>
                                <div className="flex items-center gap-3">
                                    <StatusBadge status={deployment.status} />
                                    {canStop && (
                                        <Button
                                            variant="danger"
                                            size="sm"
                                            onClick={(e) => {
                                                e.stopPropagation();
                                                handleStopClick(deployment);
                                            }}
                                        >
                                            Stop
                                        </Button>
                                    )}
                                </div>
                            </div>
                        <dl className="grid grid-cols-2 gap-4 text-sm">
                            <div>
                                <dt className="text-gray-600 dark:text-gray-400">Deployment ID</dt>
                                <dd className="font-mono text-gray-900 dark:text-gray-200">{deployment.deployment_id}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-600 dark:text-gray-400">Image</dt>
                                <dd className="font-mono text-gray-900 dark:text-gray-200 text-xs">{deployment.image ? deployment.image.split('/').pop() : '-'}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-600 dark:text-gray-400">URL</dt>
                                <dd>{deployment.primary_url ? <a href={deployment.primary_url} target="_blank" rel="noopener noreferrer" className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300">{deployment.primary_url}</a> : '-'}</dd>
                            </div>
                            <div>
                                <dt className="text-gray-600 dark:text-gray-400">Created</dt>
                                <dd className="text-gray-900 dark:text-gray-200" title={formatISO8601(deployment.created)}>
                                    {formatRelativeTimeRounded(deployment.created)}
                                </dd>
                            </div>
                            {deployment.expires_at && (
                                <div>
                                    <dt className="text-gray-600 dark:text-gray-400">Expires</dt>
                                    <dd className="text-gray-900 dark:text-gray-200">
                                        {formatTimeRemaining(deployment.expires_at)}
                                        <span className="text-gray-600 dark:text-gray-500 text-xs ml-2">({formatDate(deployment.expires_at)})</span>
                                    </dd>
                                </div>
                            )}
                        </dl>
                        <div className="mt-4 pt-4 border-t border-gray-200 dark:border-gray-800 flex items-center justify-end">
                            {otherProgressing > 0 && (
                                <span className="text-sm text-gray-600 dark:text-gray-500">
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
                message={`Are you sure you want to stop deployment ${deploymentToStop?.deployment_id}? Impact: traffic for group "${deploymentToStop?.deployment_group || 'default'}" may terminate.`}
                confirmText="Stop Deployment"
                variant="danger"
                loading={stopping}
            />
        </>
    );
}

// Deployments List Component (with pagination)
export function DeploymentsList({ projectName }) {
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
    const [actionStatus, setActionStatus] = useState('');
    const { showToast } = useToast();
    const pageSize = 10;
    const { sortedItems: sortedDeployments, sortKey, sortDirection, requestSort } = useSortableData(deployments, 'created', 'desc');
    const { activeIndex, setActiveIndex, onKeyDown } = useRowKeyboardNavigation(
        (idx) => {
            const deployment = sortedDeployments[idx];
            if (deployment) navigate(`/deployment/${projectName}/${deployment.deployment_id}`);
        },
        sortedDeployments.length
    );

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
        setActionStatus(`Stopping deployment ${deploymentToStop.deployment_id}...`);
        try {
            await api.stopDeployment(projectName, deploymentToStop.deployment_id);
            showToast(`Deployment ${deploymentToStop.deployment_id} stopped successfully`, 'success');
            setActionStatus(`Stopped deployment ${deploymentToStop.deployment_id}.`);
            setConfirmDialogOpen(false);
            setDeploymentToStop(null);
            loadDeployments();
        } catch (err) {
            showToast(`Failed to stop deployment: ${err.message}`, 'error');
            setActionStatus(`Failed to stop deployment ${deploymentToStop.deployment_id}.`);
        } finally {
            setStopping(false);
        }
    };

    const handleRollbackClick = (deployment) => {
        setDeploymentToRollback(deployment);
        setRollbackDialogOpen(true);
    };

    const [useSourceEnvVars, setUseSourceEnvVars] = useState(false);

    const handleRollbackConfirm = async () => {
        if (!deploymentToRollback) return;

        setRollingBack(true);
        setActionStatus(`${deploymentToRollback.is_active ? 'Redeploying' : 'Rolling back'} deployment ${deploymentToRollback.deployment_id}...`);
        try {
            const response = await api.createDeploymentFrom(projectName, deploymentToRollback.deployment_id, useSourceEnvVars);
            showToast(`${deploymentToRollback.is_active ? 'Redeploy' : 'Rollback'} successful! New deployment: ${response.deployment_id}`, 'success');
            setActionStatus(`${deploymentToRollback.is_active ? 'Redeployed' : 'Rolled back'} to new deployment ${response.deployment_id}.`);
            setRollbackDialogOpen(false);
            setDeploymentToRollback(null);
            setUseSourceEnvVars(false); // Reset checkbox
            loadDeployments();
        } catch (err) {
            showToast(`Failed to ${deploymentToRollback.is_active ? 'redeploy' : 'rollback'} deployment: ${err.message}`, 'error');
            setActionStatus(`Failed to ${deploymentToRollback.is_active ? 'redeploy' : 'rollback'} deployment ${deploymentToRollback.deployment_id}.`);
        } finally {
            setRollingBack(false);
        }
    };

    if (loading && deployments.length === 0) return <LoadingState label="Loading deployments..." />;
    if (error) return <ErrorState message={`Error loading deployments: ${error}`} onRetry={loadDeployments} />;

    // Find the most recent deployment in the default group (only non-terminal)
    const mostRecentDefault = sortedDeployments.find(d => d.deployment_group === 'default' && !isTerminal(d.status));

    return (
        <div>
            <div className="mb-4 flex items-center gap-2">
                <label htmlFor="deployment-group-filter" className="flex items-center gap-2">
                    <span className="text-sm text-gray-600 dark:text-gray-400 whitespace-nowrap">Filter by group:</span>
                    <select
                        id="deployment-group-filter"
                        value={groupFilter}
                        onChange={handleGroupChange}
                        className="bg-white dark:bg-gray-900 border border-gray-300 dark:border-gray-700 rounded px-3 py-2 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:border-indigo-500 cursor-pointer"
                    >
                        <option value="">All groups</option>
                        {deploymentGroups.map(group => (
                            <option key={group} value={group}>{group}</option>
                        ))}
                    </select>
                </label>
            </div>
            {actionStatus && <p className="mono-inline-status mb-3">{actionStatus}</p>}

            <MonoTableFrame>
                <MonoTable className="mono-sticky-table mono-table--sticky" onKeyDown={onKeyDown}>
                    <MonoTableHead>
                        <tr>
                            <MonoTh stickyCol className="px-6 py-3 text-left">
                                <MonoSortButton label="ID" active={sortKey === 'deployment_id'} direction={sortDirection} onClick={() => requestSort('deployment_id')} />
                            </MonoTh>
                            <MonoTh className="px-6 py-3 text-left">
                                <MonoSortButton label="Status" active={sortKey === 'status'} direction={sortDirection} onClick={() => requestSort('status')} />
                            </MonoTh>
                            <MonoTh className="px-6 py-3 text-left">Created by</MonoTh>
                            <MonoTh className="px-6 py-3 text-left">Image</MonoTh>
                            <MonoTh className="px-6 py-3 text-left">Group</MonoTh>
                            <MonoTh className="px-6 py-3 text-left">URL</MonoTh>
                            <MonoTh className="px-6 py-3 text-left">Expires</MonoTh>
                            <MonoTh className="px-6 py-3 text-left">
                                <MonoSortButton label="Created" active={sortKey === 'created'} direction={sortDirection} onClick={() => requestSort('created')} />
                            </MonoTh>
                            <MonoTh className="px-6 py-3 text-left">Actions</MonoTh>
                        </tr>
                    </MonoTableHead>
                    <MonoTableBody>
                        {sortedDeployments.length === 0 ? (
                            <MonoTableEmptyRow colSpan={9}>
                                <EmptyState message="No deployments found." />
                            </MonoTableEmptyRow>
                        ) : (
                            sortedDeployments.map((d, idx) => {
                                    const isHighlighted = mostRecentDefault && d.id === mostRecentDefault.id;
                                    return (
                                    <MonoTableRow
                                        key={d.id}
                                        onClick={() => navigate(`/deployment/${projectName}/${d.deployment_id}`)}
                                        onFocus={() => setActiveIndex(idx)}
                                        tabIndex={0}
                                        aria-label={`Deployment ${d.deployment_id}`}
                                        interactive
                                        active={activeIndex === idx}
                                        highlight={Boolean(isHighlighted)}
                                        className="transition-colors"
                                    >
                                        <MonoTd stickyCol mono className="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-200">{d.deployment_id}</MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm"><StatusBadge status={d.status} /></MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{d.created_by_email || '-'}</MonoTd>
                                        <MonoTd mono className="px-6 py-4 whitespace-nowrap text-xs text-gray-700 dark:text-gray-300">{d.image ? d.image.split('/').pop() : '-'}</MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">{d.deployment_group}</MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm">
                                            {d.primary_url ? (
                                                <a
                                                    href={d.primary_url}
                                                    target="_blank"
                                                    rel="noopener noreferrer"
                                                    className="text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300"
                                                    onClick={(e) => e.stopPropagation()}
                                                >
                                                    Link
                                                </a>
                                            ) : '-'}
                                        </MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300">
                                            {d.expires_at ? (
                                                <span>
                                                    {formatTimeRemaining(d.expires_at)}
                                                    <br />
                                                    <span className="text-gray-600 dark:text-gray-500 text-xs">({formatDate(d.expires_at)})</span>
                                                </span>
                                            ) : '-'}
                                        </MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm text-gray-700 dark:text-gray-300" title={formatISO8601(d.created)}>
                                            {formatRelativeTimeRounded(d.created)}
                                        </MonoTd>
                                        <MonoTd className="px-6 py-4 whitespace-nowrap text-sm">
                                            <div className="mono-table-action-slot">
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
                                        </MonoTd>
                                    </MonoTableRow>
                                    );
                                })
                        )}
                    </MonoTableBody>
                </MonoTable>
            </MonoTableFrame>

            <div className="mt-4 flex justify-between items-center">
                <button
                    onClick={() => setPage(p => p - 1)}
                    disabled={page === 0}
                    className="bg-gray-700 hover:bg-gray-600 disabled:bg-gray-100 dark:bg-gray-800 disabled:text-gray-600 text-white px-4 py-2 rounded text-sm transition-colors"
                >
                    Previous
                </button>
                <span className="text-sm text-gray-600 dark:text-gray-400">
                    Page {page + 1} (showing {deployments.length} deployments)
                </span>
                <button
                    onClick={() => setPage(p => p + 1)}
                    disabled={!hasMore}
                    className="bg-gray-700 hover:bg-gray-600 disabled:bg-gray-100 dark:bg-gray-800 disabled:text-gray-600 text-white px-4 py-2 rounded text-sm transition-colors"
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
                message={`Are you sure you want to stop deployment ${deploymentToStop?.deployment_id}? Impact: traffic for group "${deploymentToStop?.deployment_group || 'default'}" may terminate.`}
                confirmText="Stop Deployment"
                variant="danger"
                loading={stopping}
            />

            <Modal
                isOpen={rollbackDialogOpen}
                onClose={() => {
                    setRollbackDialogOpen(false);
                    setDeploymentToRollback(null);
                    setUseSourceEnvVars(false);
                }}
                title={deploymentToRollback?.is_active ? 'Redeploy' : 'Rollback to Deployment'}
            >
                <ModalSection>
                    <p className="text-gray-700 dark:text-gray-300">
                        {deploymentToRollback?.is_active
                            ? `Are you sure you want to redeploy ${deploymentToRollback?.deployment_id}? This will create a new deployment with the same image.`
                            : `Are you sure you want to rollback to deployment ${deploymentToRollback?.deployment_id}? This will create a new deployment with the same image.`}
                    </p>
                    
                    <div className="bg-gray-50 dark:bg-gray-800 p-4 rounded-lg">
                        <label className="flex items-start gap-3 cursor-pointer">
                            <input
                                type="checkbox"
                                checked={useSourceEnvVars}
                                onChange={(e) => setUseSourceEnvVars(e.target.checked)}
                                className="mt-1 w-4 h-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500"
                            />
                            <div className="flex-1">
                                <div className="text-sm font-medium text-gray-900 dark:text-gray-100">
                                    Use source deployment's environment variables
                                </div>
                                <div className="text-xs text-gray-600 dark:text-gray-400 mt-1">
                                    {useSourceEnvVars 
                                        ? "Will copy environment variables from the source deployment" 
                                        : "Will use the current project's environment variables (default)"}
                                </div>
                            </div>
                        </label>
                    </div>

                    <ModalActions>
                        <Button
                            variant="secondary"
                            onClick={() => {
                                setRollbackDialogOpen(false);
                                setDeploymentToRollback(null);
                                setUseSourceEnvVars(false);
                            }}
                            disabled={rollingBack}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={handleRollbackConfirm}
                            loading={rollingBack}
                            disabled={rollingBack}
                        >
                            {deploymentToRollback?.is_active ? 'Redeploy' : 'Rollback'}
                        </Button>
                    </ModalActions>
                </ModalSection>
            </Modal>
        </div>
    );
}

// Deployment Logs Component with SSE streaming
function DeploymentLogs({ projectName, deploymentId, deploymentStatus }) {
    const [logs, setLogs] = useState([]);
    const [streaming, setStreaming] = useState(false);
    const [error, setError] = useState(null);
    const [autoScroll, setAutoScroll] = useState(true);
    const [tailLines, setTailLines] = useState(1000);
    const [tailInputValue, setTailInputValue] = useState('1000');
    const logsEndRef = useRef(null);
    const abortControllerRef = useRef(null);

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
        // Stop any existing stream first
        if (abortControllerRef.current) {
            abortControllerRef.current.abort();
            abortControllerRef.current = null;
        }

        // Clear existing logs when starting a new stream
        setLogs([]);
        setError(null);
        setStreaming(true);

        const baseUrl = window.API_BASE_URL || '';
        const url = `${baseUrl}/api/v1/projects/${projectName}/deployments/${deploymentId}/logs?follow=true&tail=${tailLines}`;

        // Create new AbortController for this stream
        const abortController = new AbortController();
        abortControllerRef.current = abortController;

        // Use fetch for SSE with cookies
        fetch(url, {
            headers: {
                'Accept': 'text/event-stream',
            },
            credentials: 'include',  // Include cookies (rise_jwt)
            signal: abortController.signal,
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
                    // Ignore abort errors
                    if (err.name === 'AbortError') {
                        return;
                    }
                    console.error('Stream error:', err);
                    setError(err.message);
                    setStreaming(false);
                });
            };

            processStream();
        })
        .catch(err => {
            // Ignore abort errors
            if (err.name === 'AbortError') {
                return;
            }
            console.error('Failed to start log stream:', err);
            setError(err.message);
            setStreaming(false);
        });
    }, [projectName, deploymentId, tailLines]);

    const stopStreaming = useCallback(() => {
        if (abortControllerRef.current) {
            abortControllerRef.current.abort();
            abortControllerRef.current = null;
        }
        setStreaming(false);
    }, []);

    const loadInitialLogs = useCallback(async () => {
        const baseUrl = window.API_BASE_URL || '';
        const url = `${baseUrl}/api/v1/projects/${projectName}/deployments/${deploymentId}/logs?tail=${tailLines}`;

        try {
            const response = await fetch(url, {
                headers: {
                    'Accept': 'text/event-stream',
                },
                credentials: 'include',  // Include cookies (rise_jwt)
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
    }, [projectName, deploymentId, tailLines]);

    const clearLogs = () => {
        setLogs([]);
    };

    const handleTailLinesChange = (e) => {
        setTailInputValue(e.target.value);
    };

    const handleTailLinesBlur = () => {
        const newTail = parseInt(tailInputValue, 10);
        if (!isNaN(newTail) && newTail > 0) {
            setTailLines(newTail);
        } else {
            // Reset to current value if invalid
            setTailInputValue(tailLines.toString());
        }
    };

    const handleTailLinesKeyPress = (e) => {
        if (e.key === 'Enter') {
            e.target.blur(); // Trigger blur which will handle the update
        }
    };

    // Effect to restart streaming when tailLines changes and we're currently streaming
    useEffect(() => {
        if (streaming) {
            console.log('Tail lines changed to', tailLines, ', restarting stream...');
            startStreaming();
        }
    }, [tailLines]); // Only depend on tailLines, not streaming or startStreaming to avoid loops

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
                <div className="flex gap-2 items-center">
                    <label className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400">
                        <span>Tail lines:</span>
                        <input
                            type="number"
                            value={tailInputValue}
                            onChange={handleTailLinesChange}
                            onBlur={handleTailLinesBlur}
                            onKeyPress={handleTailLinesKeyPress}
                            min="1"
                            className="w-20 bg-gray-100 dark:bg-gray-800 border border-gray-600 rounded px-2 py-1 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:border-indigo-500"
                        />
                    </label>
                    <label className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400">
                        <input
                            type="checkbox"
                            checked={autoScroll}
                            onChange={(e) => setAutoScroll(e.target.checked)}
                            className="rounded border-gray-600 bg-gray-100 dark:bg-gray-800 text-indigo-600 focus:ring-indigo-500"
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
                <div className="mb-3 p-3 bg-red-900/20 border border-red-800 rounded text-red-600 dark:text-red-400 text-sm">
                    Error: {error}
                </div>
            )}

            <div className="bg-gray-950 border border-gray-200 dark:border-gray-800 rounded-lg overflow-hidden">
                <div
                    className="p-4 overflow-y-auto font-mono text-xs text-gray-700 dark:text-gray-300"
                    style={{ height: '400px' }}
                >
                    {logs.length === 0 ? (
                        <div className="text-gray-600 dark:text-gray-500 text-center py-8">
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
                <div className="mt-2 flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400">
                    <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse"></div>
                    Live streaming logs...
                </div>
            )}
        </div>
    );
}

function getPhaseForEvent(event: string) {
    const e = event.toLowerCase();
    if (e.includes('build')) return 'build';
    if (e.includes('push') || e.includes('image')) return 'push';
    if (e.includes('rollout') || e.includes('deploy')) return 'rollout';
    if (e.includes('health') || e.includes('ready') || e.includes('active')) return 'health';
    return 'other';
}

function formatDurationDelta(fromTs?: string | null, toTs?: string | null) {
    if (!fromTs || !toTs) return '--';
    const from = new Date(fromTs).getTime();
    const to = new Date(toTs).getTime();
    if (Number.isNaN(from) || Number.isNaN(to) || to < from) return '--';
    const seconds = Math.floor((to - from) / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const rem = seconds % 60;
    if (minutes < 60) return `${minutes}m ${rem}s`;
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return `${hours}h ${mins}m`;
}

function buildDeploymentTimeline(deployment: any) {
    const events: Array<{ label: string; ts: string | null; phase: string }> = [
        { label: 'Deployment requested', ts: deployment.created || null, phase: 'build' },
        { label: 'Image prepared', ts: deployment.created || null, phase: 'push' },
        { label: 'Rollout started', ts: deployment.created || null, phase: 'rollout' },
    ];

    if (deployment.completed_at) {
        events.push({
            label: deployment.status === 'Failed' ? 'Deployment failed' : 'Deployment completed',
            ts: deployment.completed_at,
            phase: deployment.status === 'Failed' ? 'rollout' : 'health',
        });
    }

    const healthLastCheck = deployment.controller_metadata?.health?.last_check || null;
    if (healthLastCheck) {
        events.push({
            label: deployment.controller_metadata?.health?.healthy ? 'Health check healthy' : 'Health check degraded',
            ts: healthLastCheck,
            phase: 'health',
        });
    }

    const statusEventTime = deployment.completed_at || deployment.created || null;
    events.push({
        label: `Current status: ${deployment.status}`,
        ts: statusEventTime,
        phase: getPhaseForEvent(deployment.status || ''),
    });

    const sorted = events
        .filter((e) => e.ts)
        .sort((a, b) => new Date(a.ts || '').getTime() - new Date(b.ts || '').getTime());

    return sorted.map((event, index) => {
        const prev = index > 0 ? sorted[index - 1] : null;
        return {
            ...event,
            delta: prev ? formatDurationDelta(prev.ts, event.ts) : '--',
        };
    });
}

export function DeploymentDetail({ projectName, deploymentId }) {
    const [deployment, setDeployment] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);
    const [rollbackDialogOpen, setRollbackDialogOpen] = useState(false);
    const [rolling, setRolling] = useState(false);
    const [useSourceEnvVars, setUseSourceEnvVars] = useState(false);
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
        setDetailActionStatus(`${deployment.is_active ? 'Redeploying' : 'Rolling back'} deployment ${deploymentId}...`);
        try {
            const response = await api.createDeploymentFrom(projectName, deploymentId, useSourceEnvVars);
            showToast(`${deployment.is_active ? 'Redeploy' : 'Rollback'} successful! New deployment: ${response.deployment_id}`, 'success');
            setDetailActionStatus(`${deployment.is_active ? 'Redeployed' : 'Rolled back'} to deployment ${response.deployment_id}.`);
            setRollbackDialogOpen(false);
            setUseSourceEnvVars(false); // Reset checkbox
            // Redirect to project page to see the new deployment
            navigate(`/project/${projectName}`);
        } catch (err) {
            showToast(`Failed to ${deployment.is_active ? 'redeploy' : 'rollback'} deployment: ${err.message}`, 'error');
            setDetailActionStatus(`Failed to ${deployment.is_active ? 'redeploy' : 'rollback'} deployment ${deploymentId}.`);
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

    if (loading) return <LoadingState label="Loading deployment..." />;
    if (error) return <ErrorState message={`Error loading deployment: ${error}`} onRetry={loadDeployment} />;
    if (!deployment) return <EmptyState message="Deployment not found." />;

    const timeline = buildDeploymentTimeline(deployment);
    const phases = ['build', 'push', 'rollout', 'health', 'other'];
    const groupedTimeline = phases
        .map((phase) => ({ phase, events: timeline.filter((e) => e.phase === phase) }))
        .filter((group) => group.events.length > 0);

    return (
        <section>
            <div className="flex justify-end items-center mb-4">
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

            {detailActionStatus && <p className="mono-inline-status mb-4">{detailActionStatus}</p>}

            <div className="mono-status-strip mb-6">
                <div className={`mono-status-card mono-status-card-${getStatusTone(deployment.status)}`}>
                    <span>status</span>
                    <strong>{deployment.status}</strong>
                </div>
                <div>
                    <span>deployment</span>
                    <strong className="mono-copyable-value">
                        <span>{deployment.deployment_id}</span>
                        <button
                            type="button"
                            className="mono-copy-button"
                            title="Copy deployment ID"
                            aria-label="Copy deployment ID"
                            onClick={() => handleCopy(deployment.deployment_id, 'Deployment ID')}
                        >
                            <span
                                className="mono-copy-icon svg-mask"
                                style={{ maskImage: 'url(/assets/copy.svg)', WebkitMaskImage: 'url(/assets/copy.svg)' }}
                            />
                        </button>
                    </strong>
                </div>
                <div><span>group</span><strong>{deployment.deployment_group}</strong></div>
                <div>
                    <span>created</span>
                    <strong className="mono-copyable-value" title={formatISO8601(deployment.created)}>
                        <span>{formatRelativeTimeRounded(deployment.created)}</span>
                        <button
                            type="button"
                            className="mono-copy-button"
                            title="Copy created timestamp (ISO8601)"
                            aria-label="Copy created timestamp (ISO8601)"
                            onClick={() => handleCopy(formatISO8601(deployment.created), 'Created timestamp')}
                        >
                            <span
                                className="mono-copy-icon svg-mask"
                                style={{ maskImage: 'url(/assets/copy.svg)', WebkitMaskImage: 'url(/assets/copy.svg)' }}
                            />
                        </button>
                    </strong>
                </div>
                <div><span>project</span><strong>{projectName}</strong></div>
                <div><span>created_by</span><strong>{deployment.created_by_email || '-'}</strong></div>
                <div>
                    <span>image</span>
                    <strong className="mono-copyable-value">
                        <span>{deployment.image || '-'}</span>
                        {deployment.image && (
                            <button
                                type="button"
                                className="mono-copy-button"
                                title="Copy image"
                                aria-label="Copy image"
                                onClick={() => handleCopy(deployment.image, 'Image')}
                            >
                                <span
                                    className="mono-copy-icon svg-mask"
                                    style={{ maskImage: 'url(/assets/copy.svg)', WebkitMaskImage: 'url(/assets/copy.svg)' }}
                                />
                            </button>
                        )}
                    </strong>
                </div>
                <div><span>digest</span><strong>{deployment.image_digest || '-'}</strong></div>
                <div>
                    <span>primary_url</span>
                    <strong className="mono-copyable-value">
                        <span>
                            {deployment.primary_url ? (
                                <a href={deployment.primary_url} target="_blank" rel="noopener noreferrer" className="underline uppercase">
                                    {deployment.primary_url}
                                </a>
                            ) : '-'}
                        </span>
                        {deployment.primary_url && (
                            <button
                                type="button"
                                className="mono-copy-button"
                                title="Copy primary URL"
                                aria-label="Copy primary URL"
                                onClick={() => handleCopy(deployment.primary_url, 'Primary URL')}
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
                    <span>custom_urls</span>
                    <strong>
                        {deployment.custom_domain_urls && deployment.custom_domain_urls.length > 0
                            ? deployment.custom_domain_urls.map((url, idx) => (
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
                <div><span>completed</span><strong>{deployment.completed_at ? formatDate(deployment.completed_at) : '-'}</strong></div>
                <div><span>expires</span><strong>{deployment.expires_at ? formatTimeRemaining(deployment.expires_at) : '-'}</strong></div>
            </div>

            {deployment.error_message && (
                <div className="mono-inline-status mb-6" style={{ color: '#ffc0c0', borderColor: '#7d4b4b', background: '#1a1212' }}>
                    Error: {deployment.error_message}
                </div>
            )}

            {deployment.build_logs && (
                <details className="mb-6">
                    <summary className="cursor-pointer text-indigo-600 dark:text-indigo-400 hover:text-indigo-700 dark:hover:text-indigo-300 font-semibold">Build Logs</summary>
                    <pre className="mt-2 bg-gray-950 border border-gray-200 dark:border-gray-800 rounded p-4 overflow-x-auto text-xs">
                        <code className="text-gray-700 dark:text-gray-300">{deployment.build_logs}</code>
                    </pre>
                </details>
            )}

            <div className="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-lg p-6 mb-6">
                <h3 className="text-xl font-bold mb-4">Deployment Timeline</h3>
                <div className="space-y-4">
                    {groupedTimeline.map((group) => (
                        <div key={group.phase} className="mono-timeline-group">
                            <h4 className="mono-timeline-phase">{group.phase}</h4>
                            <div className="mono-timeline-list">
                                {group.events.map((event, idx) => (
                                    <div key={`${group.phase}-${idx}`} className="mono-timeline-item">
                                        <span>{formatDate(event.ts || '')}</span>
                                        <span>{event.label}</span>
                                        <span>+{event.delta}</span>
                                    </div>
                                ))}
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            <DeploymentLogs projectName={projectName} deploymentId={deploymentId} deploymentStatus={deployment.status} />

            <h3 className="text-xl font-bold mb-4">Environment Variables</h3>
            <EnvVarsList projectName={projectName} deploymentId={deploymentId} />

            <Modal
                isOpen={rollbackDialogOpen}
                onClose={() => {
                    setRollbackDialogOpen(false);
                    setUseSourceEnvVars(false);
                }}
                title={deployment?.is_active ? 'Redeploy' : 'Rollback to Deployment'}
            >
                <ModalSection>
                    <p className="text-gray-700 dark:text-gray-300">
                        {deployment?.is_active
                            ? `Are you sure you want to redeploy ${deploymentId}? This will create a new deployment with the same image.`
                            : `Are you sure you want to rollback to deployment ${deploymentId}? This will create a new deployment with the same image.`}
                    </p>
                    
                    <div className="bg-gray-50 dark:bg-gray-800 p-4 rounded-lg">
                        <label className="flex items-start gap-3 cursor-pointer">
                            <input
                                type="checkbox"
                                checked={useSourceEnvVars}
                                onChange={(e) => setUseSourceEnvVars(e.target.checked)}
                                className="mt-1 w-4 h-4 text-indigo-600 border-gray-300 rounded focus:ring-indigo-500"
                            />
                            <div className="flex-1">
                                <div className="text-sm font-medium text-gray-900 dark:text-gray-100">
                                    Use source deployment's environment variables
                                </div>
                                <div className="text-xs text-gray-600 dark:text-gray-400 mt-1">
                                    {useSourceEnvVars 
                                        ? "Will copy environment variables from the source deployment" 
                                        : "Will use the current project's environment variables (default)"}
                                </div>
                            </div>
                        </label>
                    </div>

                    <ModalActions>
                        <Button
                            variant="secondary"
                            onClick={() => {
                                setRollbackDialogOpen(false);
                                setUseSourceEnvVars(false);
                            }}
                            disabled={rolling}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={handleRollback}
                            loading={rolling}
                            disabled={rolling}
                        >
                            {deployment?.is_active ? 'Redeploy' : 'Rollback'}
                        </Button>
                    </ModalActions>
                </ModalSection>
            </Modal>
        </section>
    );
}
