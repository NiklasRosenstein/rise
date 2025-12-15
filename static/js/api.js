// API client for Rise backend

class RiseAPI {
    constructor(baseUrl) {
        this.baseUrl = baseUrl || window.location.origin;
    }

    async request(endpoint, options = {}) {
        const token = localStorage.getItem('rise_token');
        if (!token && !options.skipAuth) {
            throw new Error('Not authenticated');
        }

        const headers = {
            'Content-Type': 'application/json',
            ...(token && { 'Authorization': `Bearer ${token}` }),
            ...options.headers,
        };

        const response = await fetch(`${this.baseUrl}${endpoint}`, {
            ...options,
            headers,
        });

        if (response.status === 401) {
            // Token expired, redirect to login
            localStorage.removeItem('rise_token');
            window.location.href = '/';
            throw new Error('Authentication expired');
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
        return this.request('/me');
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

    async createProject(name, visibility, owner) {
        return this.request('/projects', {
            method: 'POST',
            body: JSON.stringify({ name, visibility, owner })
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
}

const api = new RiseAPI();
