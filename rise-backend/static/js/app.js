// Main application logic

let currentView = 'projects';
let currentProject = null;
let refreshInterval = null;

// Pagination state
let currentPage = 0;
let pageSize = 10;
let deploymentGroupFilter = '';

// Initialize app
async function init() {
    if (!isAuthenticated()) {
        window.location.href = '/';
        return;
    }

    try {
        // Load user info
        const user = await api.getMe();
        document.getElementById('user-email').textContent = user.email;

        // Show initial view
        showView('projects');
    } catch (error) {
        console.error('Failed to initialize:', error);
        logout();
    }
}

// View management
function showView(view) {
    // Hide all views
    document.querySelectorAll('main > section').forEach(el => {
        el.style.display = 'none';
    });

    // Show requested view
    currentView = view;
    const viewEl = document.getElementById(`${view}-view`);
    if (viewEl) {
        viewEl.style.display = 'block';
    }

    // Stop auto-refresh when switching views
    stopAutoRefresh();

    // Load view data
    switch (view) {
        case 'projects':
            loadProjects();
            break;
        case 'teams':
            loadTeams();
            break;
    }
}

// Load projects
async function loadProjects() {
    const listEl = document.getElementById('projects-list');
    listEl.innerHTML = '<p aria-busy="true">Loading projects...</p>';

    try {
        const projects = await api.getProjects();

        if (projects.length === 0) {
            listEl.innerHTML = '<p>No projects found.</p>';
            return;
        }

        listEl.innerHTML = `
            <table>
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Status</th>
                        <th>Owner</th>
                        <th>Visibility</th>
                        <th>URL</th>
                        <th>Actions</th>
                    </tr>
                </thead>
                <tbody>
                    ${projects.map(p => {
                        const owner = p.owner_user_email ? `user:${p.owner_user_email}` :
                                     p.owner_team_name ? `team:${p.owner_team_name}` : '-';
                        return `
                        <tr>
                            <td>${escapeHtml(p.name)}</td>
                            <td><span class="status-badge status-${p.status.toLowerCase()}">${p.status}</span></td>
                            <td>${escapeHtml(owner)}</td>
                            <td>${p.visibility}</td>
                            <td>${p.project_url ? `<a href="${p.project_url}" target="_blank">${p.project_url}</a>` : '-'}</td>
                            <td><a href="#" onclick="showProject('${escapeHtml(p.name)}'); return false;">View</a></td>
                        </tr>
                        `;
                    }).join('')}
                </tbody>
            </table>
        `;
    } catch (error) {
        listEl.innerHTML = `<p>Error loading projects: ${escapeHtml(error.message)}</p>`;
    }
}

// Load teams
async function loadTeams() {
    const listEl = document.getElementById('teams-list');
    listEl.innerHTML = '<p aria-busy="true">Loading teams...</p>';

    try {
        const teams = await api.getTeams();

        if (teams.length === 0) {
            listEl.innerHTML = '<p>No teams found.</p>';
            return;
        }

        listEl.innerHTML = `
            <table>
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Members</th>
                        <th>Owners</th>
                        <th>Created</th>
                        <th>Actions</th>
                    </tr>
                </thead>
                <tbody>
                    ${teams.map(t => `
                        <tr>
                            <td>${escapeHtml(t.name)}</td>
                            <td>${t.members.length}</td>
                            <td>${t.owners.length}</td>
                            <td>${formatDate(t.created)}</td>
                            <td><a href="#" onclick="showTeam('${escapeHtml(t.name)}'); return false;">View</a></td>
                        </tr>
                    `).join('')}
                </tbody>
            </table>
        `;
    } catch (error) {
        listEl.innerHTML = `<p>Error loading teams: ${escapeHtml(error.message)}</p>`;
    }
}

// Show team detail
async function showTeam(teamName) {
    // Hide all views
    document.querySelectorAll('main > section').forEach(el => {
        el.style.display = 'none';
    });

    // Show team detail view
    const viewEl = document.getElementById('team-detail-view');
    viewEl.style.display = 'block';

    const detailEl = document.getElementById('team-detail');
    detailEl.innerHTML = '<p aria-busy="true">Loading team...</p>';

    try {
        // Fetch team with expanded member and owner emails
        const team = await api.getTeam(teamName, { expand: 'members,owners' });

        detailEl.innerHTML = `
            <article>
                <header><h3>Team ${escapeHtml(team.name)}</h3></header>
                <dl>
                    <dt>Created</dt>
                    <dd>${formatDate(team.created)}</dd>
                    <dt>Updated</dt>
                    <dd>${formatDate(team.updated)}</dd>
                </dl>
            </article>

            <h4>Owners</h4>
            ${team.owners && team.owners.length > 0 ? `
                <table>
                    <thead>
                        <tr>
                            <th>Email</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${team.owners.map(owner => `
                            <tr>
                                <td>${escapeHtml(owner.email)}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            ` : '<p>No owners</p>'}

            <h4>Members</h4>
            ${team.members && team.members.length > 0 ? `
                <table>
                    <thead>
                        <tr>
                            <th>Email</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${team.members.map(member => `
                            <tr>
                                <td>${escapeHtml(member.email)}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            ` : '<p>No members</p>'}
        `;
    } catch (error) {
        detailEl.innerHTML = `<p>Error loading team: ${escapeHtml(error.message)}</p>`;
    }
}

// Show project detail
async function showProject(projectName) {
    currentProject = projectName;

    // Hide all views
    document.querySelectorAll('main > section').forEach(el => {
        el.style.display = 'none';
    });

    // Show project detail view
    const viewEl = document.getElementById('project-detail-view');
    viewEl.style.display = 'block';

    const detailEl = document.getElementById('project-detail');
    const deploymentListEl = document.getElementById('deployment-list');

    detailEl.innerHTML = '<p aria-busy="true">Loading project...</p>';
    deploymentListEl.innerHTML = '';

    // Reset pagination and filter state
    currentPage = 0;
    deploymentGroupFilter = '';
    const filterInput = document.getElementById('deployment-group-filter');
    if (filterInput) {
        filterInput.value = '';
    }

    try {
        const project = await api.getProject(projectName, { expand: 'owner' });

        detailEl.innerHTML = `
            <article>
                <header><h3>Project ${escapeHtml(project.name)}</h3></header>
                <dl>
                    <dt>Status</dt>
                    <dd><span class="status-badge status-${project.status.toLowerCase()}">${project.status}</span></dd>
                    <dt>Visibility</dt>
                    <dd>${project.visibility}</dd>
                    <dt>URL</dt>
                    <dd>${project.project_url ? `<a href="${project.project_url}" target="_blank">${project.project_url}</a>` : '-'}</dd>
                    <dt>Created</dt>
                    <dd>${formatDate(project.created)}</dd>
                </dl>
            </article>
        `;

        // Load deployments
        await loadDeployments(projectName, 0);

        // Start auto-refresh for deployment status (maintain current page)
        startAutoRefresh(() => loadDeployments(projectName, currentPage), 5000);
    } catch (error) {
        detailEl.innerHTML = `<p>Error loading project: ${escapeHtml(error.message)}</p>`;
    }
}

// Load deployments for a project
async function loadDeployments(projectName, page = 0) {
    const listEl = document.getElementById('deployment-list');
    const pageInfoEl = document.getElementById('page-info');
    const prevBtn = document.getElementById('prev-page');
    const nextBtn = document.getElementById('next-page');

    try {
        const params = {
            limit: pageSize,
            offset: page * pageSize,
        };

        // Add group filter if set
        if (deploymentGroupFilter) {
            params.group = deploymentGroupFilter;
        }

        const deployments = await api.getProjectDeployments(projectName, params);

        if (deployments.length === 0 && page === 0) {
            listEl.innerHTML = '<p>No deployments found.</p>';
            pageInfoEl.textContent = '';
            prevBtn.disabled = true;
            nextBtn.disabled = true;
            return;
        }

        listEl.innerHTML = `
            <table>
                <thead>
                    <tr>
                        <th>ID</th>
                        <th>Status</th>
                        <th>Image</th>
                        <th>Group</th>
                        <th>URL</th>
                        <th>Expires</th>
                        <th>Created</th>
                        <th>Actions</th>
                    </tr>
                </thead>
                <tbody>
                    ${deployments.map(d => `
                        <tr>
                            <td><code>${escapeHtml(d.deployment_id)}</code></td>
                            <td><span class="status-badge status-${d.status.toLowerCase()}">${d.status}</span></td>
                            <td><small>${d.image ? escapeHtml(d.image.split('/').pop()) : '-'}</small></td>
                            <td>${escapeHtml(d.deployment_group)}</td>
                            <td>${d.deployment_url ? `<a href="${d.deployment_url}" target="_blank">Link</a>` : '-'}</td>
                            <td>${d.expires_at ? formatDate(d.expires_at) : '-'}</td>
                            <td>${formatDate(d.created)}</td>
                            <td><a href="#" onclick="showDeployment('${escapeHtml(projectName)}', '${escapeHtml(d.deployment_id)}'); return false;">View</a></td>
                        </tr>
                    `).join('')}
                </tbody>
            </table>
        `;

        // Update pagination controls
        currentPage = page;
        pageInfoEl.textContent = `Page ${page + 1} (showing ${deployments.length} deployments)`;
        prevBtn.disabled = page === 0;
        nextBtn.disabled = deployments.length < pageSize;
    } catch (error) {
        listEl.innerHTML = `<p>Error loading deployments: ${escapeHtml(error.message)}</p>`;
        pageInfoEl.textContent = '';
    }
}

// Show deployment detail
async function showDeployment(projectName, deploymentId) {
    // Hide all views
    document.querySelectorAll('main > section').forEach(el => {
        el.style.display = 'none';
    });

    // Show deployment detail view
    const viewEl = document.getElementById('deployment-detail-view');
    viewEl.style.display = 'block';

    const detailEl = document.getElementById('deployment-detail');
    detailEl.innerHTML = '<p aria-busy="true">Loading deployment...</p>';

    try {
        const deployment = await api.getDeployment(projectName, deploymentId);

        detailEl.innerHTML = `
            <article>
                <header>
                    <h3>Deployment ${escapeHtml(deployment.deployment_id)}</h3>
                    <span class="status-badge status-${deployment.status.toLowerCase()}">${deployment.status}</span>
                </header>
                <dl>
                    <dt>Project</dt>
                    <dd>${escapeHtml(deployment.project)}</dd>
                    <dt>Image</dt>
                    <dd><code>${deployment.image ? escapeHtml(deployment.image) : '-'}</code></dd>
                    <dt>Image Digest</dt>
                    <dd><small><code>${deployment.image_digest ? escapeHtml(deployment.image_digest) : '-'}</code></small></dd>
                    <dt>Group</dt>
                    <dd>${escapeHtml(deployment.deployment_group)}</dd>
                    <dt>URL</dt>
                    <dd>${deployment.deployment_url ? `<a href="${deployment.deployment_url}" target="_blank">${deployment.deployment_url}</a>` : '-'}</dd>
                    <dt>Created</dt>
                    <dd>${formatDate(deployment.created)}</dd>
                    ${deployment.completed_at ? `<dt>Completed</dt><dd>${formatDate(deployment.completed_at)}</dd>` : ''}
                    ${deployment.error_message ? `<dt>Error</dt><dd class="error">${escapeHtml(deployment.error_message)}</dd>` : ''}
                </dl>
                ${deployment.build_logs ? `
                    <details>
                        <summary>Build Logs</summary>
                        <pre><code>${escapeHtml(deployment.build_logs)}</code></pre>
                    </details>
                ` : ''}
            </article>
        `;

        // Auto-refresh if deployment is in progress
        const inProgressStatuses = ['Pending', 'Building', 'Pushing', 'Pushed', 'Deploying'];
        if (inProgressStatuses.includes(deployment.status)) {
            startAutoRefresh(() => showDeployment(projectName, deploymentId), 3000);
        }
    } catch (error) {
        detailEl.innerHTML = `<p>Error loading deployment: ${escapeHtml(error.message)}</p>`;
    }
}

// Auto-refresh management
function startAutoRefresh(fn, interval) {
    stopAutoRefresh();
    refreshInterval = setInterval(fn, interval);
}

function stopAutoRefresh() {
    if (refreshInterval) {
        clearInterval(refreshInterval);
        refreshInterval = null;
    }
}

// Navigation
function goBack() {
    stopAutoRefresh();
    if (currentProject) {
        showProject(currentProject);
    } else {
        showView('projects');
    }
}

// Utility functions
function escapeHtml(text) {
    if (text === null || text === undefined) return '';
    const div = document.createElement('div');
    div.textContent = String(text);
    return div.innerHTML;
}

function formatDate(dateString) {
    const date = new Date(dateString);
    return date.toLocaleString();
}

// Pagination functions
function nextPage() {
    if (currentProject) {
        loadDeployments(currentProject, currentPage + 1);
    }
}

function prevPage() {
    if (currentProject && currentPage > 0) {
        loadDeployments(currentProject, currentPage - 1);
    }
}

// Filter functions
function applyDeploymentFilter() {
    const filterInput = document.getElementById('deployment-group-filter');
    deploymentGroupFilter = filterInput.value.trim();
    if (currentProject) {
        loadDeployments(currentProject, 0); // Reset to first page when filtering
    }
}

function clearDeploymentFilter() {
    const filterInput = document.getElementById('deployment-group-filter');
    filterInput.value = '';
    deploymentGroupFilter = '';
    if (currentProject) {
        loadDeployments(currentProject, 0); // Reset to first page
    }
}

// Initialize on load
document.addEventListener('DOMContentLoaded', init);
