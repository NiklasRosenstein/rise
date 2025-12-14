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
    const serviceAccountListEl = document.getElementById('service-account-list');
    const deploymentListEl = document.getElementById('deployment-list');

    detailEl.innerHTML = '<p aria-busy="true">Loading project...</p>';
    serviceAccountListEl.innerHTML = '';
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

        // Load custom domains (default tab)
        await loadCustomDomains(projectName);

        // Load service accounts
        await loadServiceAccounts(projectName);

        // Load environment variables
        await loadProjectEnvVars(projectName);

        // Load deployments
        await loadDeployments(projectName, 0);

        // Start auto-refresh for deployment status (maintain current page)
        startAutoRefresh(() => loadDeployments(projectName, currentPage), 5000);
    } catch (error) {
        detailEl.innerHTML = `<p>Error loading project: ${escapeHtml(error.message)}</p>`;
    }
}

// Switch between project tabs
function switchProjectTab(tabName) {
    // Update tab buttons
    document.querySelectorAll('.tab-button').forEach(btn => {
        btn.classList.remove('active');
    });
    event.target.classList.add('active');

    // Hide all tab contents
    document.querySelectorAll('.tab-content').forEach(content => {
        content.style.display = 'none';
    });

    // Show selected tab
    const tabContent = document.getElementById(`tab-${tabName}`);
    if (tabContent) {
        tabContent.style.display = 'block';
    }

    // Stop auto-refresh when not on deployments tab
    if (tabName !== 'deployments') {
        stopAutoRefresh();
    } else {
        // Restart auto-refresh for deployments
        startAutoRefresh(() => loadDeployments(currentProject, currentPage), 5000);
    }
}

// Load custom domains for a project
async function loadCustomDomains(projectName) {
    const listEl = document.getElementById('custom-domains-list');
    listEl.innerHTML = '<p aria-busy="true">Loading custom domains...</p>';

    try {
        const domains = await api.getProjectDomains(projectName);

        if (domains.length === 0) {
            listEl.innerHTML = `
                <p>No custom domains configured for this project.</p>
                <button onclick="showAddDomainForm()">Add Custom Domain</button>
            `;
            return;
        }

        listEl.innerHTML = `
            <button onclick="showAddDomainForm()" style="margin-bottom: 1rem;">Add Custom Domain</button>
            <table>
                <thead>
                    <tr>
                        <th>Domain</th>
                        <th>CNAME Target</th>
                        <th>Verification</th>
                        <th>Certificate</th>
                        <th>Created</th>
                        <th>Actions</th>
                    </tr>
                </thead>
                <tbody>
                    ${domains.map(d => {
                        const verificationBadge = d.verification_status === 'Verified' ? 
                            `<span class="status-badge verification-verified">✓ Verified</span>` :
                            d.verification_status === 'Failed' ?
                            `<span class="status-badge verification-failed">✗ Failed</span>` :
                            `<span class="status-badge verification-pending">⏳ Pending</span>`;

                        const certBadge = d.certificate_status === 'Issued' ?
                            `<span class="status-badge status-healthy">✓ Issued</span>` :
                            d.certificate_status === 'Pending' ?
                            `<span class="status-badge status-deploying">⏳ Pending</span>` :
                            d.certificate_status === 'Failed' ?
                            `<span class="status-badge status-failed">✗ Failed</span>` :
                            `<span class="status-badge status-stopped">-</span>`;

                        return `
                        <tr>
                            <td><strong>${escapeHtml(d.domain_name)}</strong></td>
                            <td><code>${escapeHtml(d.cname_target)}</code></td>
                            <td>${verificationBadge}</td>
                            <td>${certBadge}</td>
                            <td>${formatDate(d.created)}</td>
                            <td>
                                ${d.verification_status !== 'Verified' ? 
                                    `<button onclick="verifyDomain('${escapeHtml(projectName)}', '${escapeHtml(d.domain_name)}')" class="secondary" style="padding: 0.25rem 0.5rem; font-size: 0.875rem;">Verify</button>` : ''}
                                <button onclick="viewDomainDetails('${escapeHtml(projectName)}', '${escapeHtml(d.domain_name)}')" class="secondary" style="padding: 0.25rem 0.5rem; font-size: 0.875rem;">Details</button>
                                <button onclick="deleteDomain('${escapeHtml(projectName)}', '${escapeHtml(d.domain_name)}')" class="contrast" style="padding: 0.25rem 0.5rem; font-size: 0.875rem;">Delete</button>
                            </td>
                        </tr>
                        `;
                    }).join('')}
                </tbody>
            </table>
        `;
    } catch (error) {
        listEl.innerHTML = `<p class="error">Error loading domains: ${escapeHtml(error.message)}</p>`;
    }
}

// Show add domain form
function showAddDomainForm() {
    const listEl = document.getElementById('custom-domains-list');
    const currentContent = listEl.innerHTML;
    
    listEl.innerHTML = `
        <div style="background: rgba(255,255,255,0.05); padding: 1.5rem; border-radius: 0.5rem; margin-bottom: 1rem;">
            <h4>Add Custom Domain</h4>
            <label for="new-domain-name">
                Domain Name
                <input type="text" id="new-domain-name" placeholder="example.com or www.example.com" />
            </label>
            <div style="display: flex; gap: 0.5rem; margin-top: 1rem;">
                <button onclick="addDomain('${currentProject}')">Add Domain</button>
                <button onclick="loadCustomDomains('${currentProject}')" class="secondary">Cancel</button>
            </div>
        </div>
        ${currentContent}
    `;
    
    document.getElementById('new-domain-name').focus();
}

// Add a custom domain
async function addDomain(projectName) {
    const domainInput = document.getElementById('new-domain-name');
    const domainName = domainInput.value.trim();

    if (!domainName) {
        alert('Please enter a domain name');
        return;
    }

    try {
        const response = await api.addProjectDomain(projectName, domainName);
        
        // Show success message with instructions
        const listEl = document.getElementById('custom-domains-list');
        listEl.innerHTML = `
            <div class="domain-instructions">
                <h4>✅ Domain Added Successfully!</h4>
                <p><strong>Domain:</strong> ${escapeHtml(response.domain.domain_name)}</p>
                <p style="margin-top: 1rem;"><strong>Next Steps:</strong></p>
                <ol>
                    <li>Configure a CNAME record for your domain:
                        <ul style="list-style: none; margin-top: 0.5rem;">
                            <li><strong>Name:</strong> <code>${escapeHtml(response.instructions.cname_record.name)}</code></li>
                            <li><strong>Value:</strong> <code>${escapeHtml(response.instructions.cname_record.value)}</code></li>
                        </ul>
                    </li>
                    <li style="margin-top: 0.5rem;">Wait for DNS propagation (this can take a few minutes)</li>
                    <li style="margin-top: 0.5rem;">Click the "Verify" button to validate your DNS configuration</li>
                </ol>
                <button onclick="loadCustomDomains('${projectName}')" style="margin-top: 1rem;">Back to Domains List</button>
            </div>
        `;
    } catch (error) {
        alert(`Failed to add domain: ${error.message}`);
    }
}

// Verify a custom domain
async function verifyDomain(projectName, domainName) {
    try {
        const response = await api.verifyProjectDomain(projectName, domainName);
        
        if (response.verification_result.success) {
            alert(`✅ Domain verified successfully!\n\n${response.verification_result.message}`);
        } else {
            alert(`❌ Domain verification failed\n\n${response.verification_result.message}\n\nExpected: ${response.verification_result.expected_value || 'N/A'}\nActual: ${response.verification_result.actual_value || 'N/A'}`);
        }
        
        // Reload domains list
        await loadCustomDomains(projectName);
    } catch (error) {
        alert(`Failed to verify domain: ${error.message}`);
    }
}

// View domain details
async function viewDomainDetails(projectName, domainName) {
    const listEl = document.getElementById('custom-domains-list');
    
    try {
        const domains = await api.getProjectDomains(projectName);
        const domain = domains.find(d => d.domain_name === domainName);
        
        if (!domain) {
            alert('Domain not found');
            return;
        }

        let challengesHtml = '';
        try {
            const challenges = await api.getDomainChallenges(projectName, domainName);
            if (challenges.length > 0) {
                challengesHtml = `
                    <h5>ACME Challenges</h5>
                    <table>
                        <thead>
                            <tr>
                                <th>Type</th>
                                <th>Record Name</th>
                                <th>Record Value</th>
                                <th>Status</th>
                            </tr>
                        </thead>
                        <tbody>
                            ${challenges.map(c => `
                                <tr>
                                    <td>${escapeHtml(c.challenge_type)}</td>
                                    <td><code>${escapeHtml(c.record_name)}</code></td>
                                    <td><code>${escapeHtml(c.record_value)}</code></td>
                                    <td><span class="status-badge status-${c.status.toLowerCase()}">${c.status}</span></td>
                                </tr>
                            `).join('')}
                        </tbody>
                    </table>
                `;
            }
        } catch (e) {
            // Challenges not available
        }

        listEl.innerHTML = `
            <button onclick="loadCustomDomains('${projectName}')" class="secondary" style="margin-bottom: 1rem;">← Back to Domains</button>
            <article>
                <header><h4>Domain: ${escapeHtml(domain.domain_name)}</h4></header>
                <dl>
                    <dt>CNAME Target</dt>
                    <dd><code>${escapeHtml(domain.cname_target)}</code></dd>
                    <dt>Verification Status</dt>
                    <dd>${domain.verification_status}${domain.verified_at ? ` (verified at ${formatDate(domain.verified_at)})` : ''}</dd>
                    <dt>Certificate Status</dt>
                    <dd>${domain.certificate_status}${domain.certificate_issued_at ? ` (issued at ${formatDate(domain.certificate_issued_at)})` : ''}</dd>
                    ${domain.certificate_expires_at ? `
                        <dt>Certificate Expires</dt>
                        <dd>${formatDate(domain.certificate_expires_at)}</dd>
                    ` : ''}
                    <dt>Created</dt>
                    <dd>${formatDate(domain.created)}</dd>
                </dl>
                ${challengesHtml}
            </article>
        `;
    } catch (error) {
        alert(`Failed to load domain details: ${error.message}`);
    }
}

// Delete a custom domain
async function deleteDomain(projectName, domainName) {
    if (!confirm(`Are you sure you want to delete the domain "${domainName}"?`)) {
        return;
    }

    try {
        await api.deleteProjectDomain(projectName, domainName);
        alert('Domain deleted successfully');
        await loadCustomDomains(projectName);
    } catch (error) {
        alert(`Failed to delete domain: ${error.message}`);
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
                        <th>Created by</th>
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
                            <td>${escapeHtml(d.created_by_email || '-')}</td>
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

// Load service accounts for a project
async function loadServiceAccounts(projectName) {
    const listEl = document.getElementById('service-account-list');
    listEl.innerHTML = '<p aria-busy="true">Loading service accounts...</p>';

    try {
        const response = await api.getProjectServiceAccounts(projectName);
        const serviceAccounts = response.workload_identities || [];

        if (serviceAccounts.length === 0) {
            listEl.innerHTML = '<p>No service accounts found.</p>';
            return;
        }

        const tableHtml = `
            <figure>
                <table role="grid">
                    <thead>
                        <tr>
                            <th>Email</th>
                            <th>Issuer URL</th>
                            <th>Claims</th>
                            <th>Created</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${serviceAccounts.map(sa => `
                            <tr>
                                <td>${escapeHtml(sa.email)}</td>
                                <td style="word-break: break-all; max-width: 300px;">${escapeHtml(sa.issuer_url)}</td>
                                <td style="font-family: monospace; font-size: 0.85em;">
                                    ${Object.entries(sa.claims || {})
                                        .map(([key, value]) => `${escapeHtml(key)}=${escapeHtml(value)}`)
                                        .join('<br>')
                                    }
                                </td>
                                <td>${formatDate(sa.created_at)}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            </figure>
        `;

        listEl.innerHTML = tableHtml;
    } catch (error) {
        listEl.innerHTML = `<p>Error loading service accounts: ${escapeHtml(error.message)}</p>`;
    }
}

// Load environment variables for a project
async function loadProjectEnvVars(projectName) {
    const listEl = document.getElementById('project-env-vars-list');
    listEl.innerHTML = '<p aria-busy="true">Loading environment variables...</p>';

    try {
        const response = await api.getProjectEnvVars(projectName);
        const envVars = response.env_vars || [];

        if (envVars.length === 0) {
            listEl.innerHTML = '<p>No environment variables configured.</p>';
            return;
        }

        const tableHtml = `
            <figure>
                <table role="grid">
                    <thead>
                        <tr>
                            <th>Key</th>
                            <th>Value</th>
                            <th>Type</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${envVars.map(env => `
                            <tr>
                                <td><code>${escapeHtml(env.key)}</code></td>
                                <td><code>${escapeHtml(env.value)}</code></td>
                                <td>${env.is_secret ? '<span class="status-badge" style="background-color: var(--pico-color-yellow-500);">secret</span>' : '<span class="status-badge" style="background-color: var(--pico-color-grey-500);">plain</span>'}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            </figure>
        `;

        listEl.innerHTML = tableHtml;
    } catch (error) {
        listEl.innerHTML = `<p>Error loading environment variables: ${escapeHtml(error.message)}</p>`;
    }
}

// Load environment variables for a deployment
async function loadDeploymentEnvVars(projectName, deploymentId) {
    const listEl = document.getElementById('deployment-env-vars-list');
    listEl.innerHTML = '<p aria-busy="true">Loading environment variables...</p>';

    try {
        const response = await api.getDeploymentEnvVars(projectName, deploymentId);
        const envVars = response.env_vars || [];

        if (envVars.length === 0) {
            listEl.innerHTML = '<p>No environment variables configured.</p>';
            return;
        }

        const tableHtml = `
            <figure>
                <table role="grid">
                    <thead>
                        <tr>
                            <th>Key</th>
                            <th>Value</th>
                            <th>Type</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${envVars.map(env => `
                            <tr>
                                <td><code>${escapeHtml(env.key)}</code></td>
                                <td><code>${escapeHtml(env.value)}</code></td>
                                <td>${env.is_secret ? '<span class="status-badge" style="background-color: var(--pico-color-yellow-500);">secret</span>' : '<span class="status-badge" style="background-color: var(--pico-color-grey-500);">plain</span>'}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            </figure>
            <p style="font-size: 0.9em; color: var(--pico-color-grey-500); margin-top: 1rem;">
                <strong>Note:</strong> Environment variables are read-only snapshots taken at deployment time.
                Secret values are always masked for security.
            </p>
        `;

        listEl.innerHTML = tableHtml;
    } catch (error) {
        listEl.innerHTML = `<p>Error loading environment variables: ${escapeHtml(error.message)}</p>`;
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
                    <dt>Created by</dt>
                    <dd>${escapeHtml(deployment.created_by_email || '-')}</dd>
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

        // Load environment variables
        await loadDeploymentEnvVars(projectName, deploymentId);

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
