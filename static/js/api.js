// API client for Rise backend

class RiseAPI {
    constructor(baseUrl) {
        this.baseUrl = baseUrl || window.location.origin;
    }

    async request(endpoint, options = {}) {
        const headers = {
            'Content-Type': 'application/json',
            ...options.headers,
        };

        const response = await fetch(`${this.baseUrl}/api/v1${endpoint}`, {
            ...options,
            headers,
            credentials: 'include',  // Always include cookies (rise_jwt)
        });

        if (response.status === 401) {
            // Authentication required - let the app handle showing login page
            throw new Error('Authentication required');
        }

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`API error: ${response.status} ${errorText}`);
        }

        // Handle responses with no body (204 No Content, 202 Accepted)
        if (response.status === 204 || response.status === 202) {
            return null;
        }

        return response.json();
    }

    // User endpoints
    async getMe() {
        return this.request('/users/me');
    }

    async lookupUsers(emails) {
        return this.request('/users/lookup', {
            method: 'POST',
            body: JSON.stringify({ emails })
        });
    }

    // Team endpoints
    async getTeams() {
        return this.request('/teams');
    }

    async getTeam(idOrName, params = {}) {
        const queryString = new URLSearchParams(params).toString();
        return this.request(`/teams/${idOrName}${queryString ? '?' + queryString : ''}`);
    }

    async createTeam(name, members, owners) {
        return this.request('/teams', {
            method: 'POST',
            body: JSON.stringify({ name, members, owners })
        });
    }

    async updateTeam(idOrName, updates) {
        return this.request(`/teams/${idOrName}`, {
            method: 'PUT',
            body: JSON.stringify(updates)
        });
    }

    async deleteTeam(idOrName) {
        return this.request(`/teams/${idOrName}`, {
            method: 'DELETE'
        });
    }

    // Project endpoints
    async getProjects() {
        return this.request('/projects');
    }

    async getProject(idOrName, params = {}) {
        const queryString = new URLSearchParams(params).toString();
        return this.request(`/projects/${idOrName}${queryString ? '?' + queryString : ''}`);
    }

    async createProject(name, access_class, owner) {
        return this.request('/projects', {
            method: 'POST',
            body: JSON.stringify({ name, access_class, owner })
        });
    }

    async getAccessClasses() {
        return this.request('/projects/access-classes');
    }

    async updateProject(nameOrId, updates) {
        return this.request(`/projects/${nameOrId}`, {
            method: 'PUT',
            body: JSON.stringify(updates)
        });
    }

    async deleteProject(nameOrId) {
        return this.request(`/projects/${nameOrId}`, {
            method: 'DELETE'
        });
    }

    // Deployment endpoints
    async getProjectDeployments(projectName, params = {}) {
        const queryString = new URLSearchParams(
            Object.entries(params).filter(([_, v]) => v !== null && v !== undefined && v !== '')
        ).toString();
        return this.request(`/projects/${projectName}/deployments${queryString ? '?' + queryString : ''}`);
    }

    async getDeployment(projectName, deploymentId) {
        return this.request(`/projects/${projectName}/deployments/${deploymentId}`);
    }

    async getDeploymentGroups(projectName) {
        return this.request(`/projects/${projectName}/deployment-groups`);
    }

    async stopDeployment(projectName, deploymentId) {
        return this.request(`/projects/${projectName}/deployments/${deploymentId}/stop`, {
            method: 'POST'
        });
    }

    async rollbackDeployment(projectName, deploymentId) {
        return this.request(`/projects/${projectName}/deployments/${deploymentId}/rollback`, {
            method: 'POST'
        });
    }

    // Service account endpoints
    async getProjectServiceAccounts(projectName) {
        return this.request(`/projects/${projectName}/workload-identities`);
    }

    async createServiceAccount(projectName, issuerUrl, claims) {
        return this.request(`/projects/${projectName}/workload-identities`, {
            method: 'POST',
            body: JSON.stringify({ issuer_url: issuerUrl, claims })
        });
    }

    async updateServiceAccount(projectName, saId, issuerUrl, claims) {
        return this.request(`/projects/${projectName}/workload-identities/${saId}`, {
            method: 'PUT',
            body: JSON.stringify({ issuer_url: issuerUrl, claims })
        });
    }

    async deleteServiceAccount(projectName, saId) {
        return this.request(`/projects/${projectName}/workload-identities/${saId}`, {
            method: 'DELETE'
        });
    }

    // Environment variable endpoints
    async getProjectEnvVars(projectName) {
        return this.request(`/projects/${projectName}/env`);
    }

    async getDeploymentEnvVars(projectName, deploymentId) {
        return this.request(`/projects/${projectName}/deployments/${deploymentId}/env`);
    }

    async setEnvVar(projectName, key, value, isSecret) {
        return this.request(`/projects/${projectName}/env/${encodeURIComponent(key)}`, {
            method: 'PUT',
            body: JSON.stringify({ value, is_secret: isSecret })
        });
    }

    async deleteEnvVar(projectName, key) {
        return this.request(`/projects/${projectName}/env/${encodeURIComponent(key)}`, {
            method: 'DELETE'
        });
    }

    // Custom domain endpoints
    async getProjectDomains(projectName) {
        return this.request(`/projects/${projectName}/domains`);
    }

    async addCustomDomain(projectName, domain) {
        return this.request(`/projects/${projectName}/domains`, {
            method: 'POST',
            body: JSON.stringify({ domain })
        });
    }

    async deleteCustomDomain(projectName, domain) {
        return this.request(`/projects/${projectName}/domains/${encodeURIComponent(domain)}`, {
            method: 'DELETE'
        });
    }

    // Extension endpoints

    /**
     * Get all available extension types (globally registered providers)
     */
    async getExtensionTypes() {
        return this.request('/extensions/types');
    }

    /**
     * Get enabled extensions for a project
     */
    async getProjectExtensions(projectName) {
        return this.request(`/projects/${projectName}/extensions`);
    }

    /**
     * Get specific extension details
     */
    async getProjectExtension(projectName, extensionName) {
        return this.request(`/projects/${projectName}/extensions/${extensionName}`);
    }

    /**
     * Enable/create an extension for a project
     * @param {string} extensionType - Extension type identifier (e.g., "aws-rds-provisioner")
     */
    async createExtension(projectName, extensionName, extensionType, spec) {
        return this.request(`/projects/${projectName}/extensions/${extensionName}`, {
            method: 'POST',
            body: JSON.stringify({ extension_type: extensionType, spec })
        });
    }

    /**
     * Update an extension's spec (full replace)
     */
    async updateExtension(projectName, extensionName, spec) {
        return this.request(`/projects/${projectName}/extensions/${extensionName}`, {
            method: 'PUT',
            body: JSON.stringify({ spec })
        });
    }

    /**
     * Delete an extension
     */
    async deleteExtension(projectName, extensionName) {
        return this.request(`/projects/${projectName}/extensions/${extensionName}`, {
            method: 'DELETE'
        });
    }
}

const api = new RiseAPI();
