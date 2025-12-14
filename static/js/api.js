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

        return response.json();
    }

    // User endpoints
    async getMe() {
        return this.request('/me');
    }

    // Team endpoints
    async getTeams() {
        return this.request('/teams');
    }

    async getTeam(idOrName, params = {}) {
        const queryString = new URLSearchParams(params).toString();
        return this.request(`/teams/${idOrName}${queryString ? '?' + queryString : ''}`);
    }

    // Project endpoints
    async getProjects() {
        return this.request('/projects');
    }

    async getProject(idOrName, params = {}) {
        const queryString = new URLSearchParams(params).toString();
        return this.request(`/projects/${idOrName}${queryString ? '?' + queryString : ''}`);
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

    // Service account endpoints
    async getProjectServiceAccounts(projectName) {
        return this.request(`/projects/${projectName}/workload-identities`);
    }

    // Environment variable endpoints
    async getProjectEnvVars(projectName) {
        return this.request(`/projects/${projectName}/env`);
    }

    async getDeploymentEnvVars(projectName, deploymentId) {
        return this.request(`/projects/${projectName}/deployments/${deploymentId}/env`);
    }

    // Custom domain endpoints
    async getProjectDomains(projectName) {
        return this.request(`/projects/${projectName}/domains`);
    }

    async addProjectDomain(projectName, domainName) {
        return this.request(`/projects/${projectName}/domains`, {
            method: 'POST',
            body: JSON.stringify({ domain_name: domainName }),
        });
    }

    async deleteProjectDomain(projectName, domainName) {
        return this.request(`/projects/${projectName}/domains/${domainName}`, {
            method: 'DELETE',
        });
    }

    async verifyProjectDomain(projectName, domainName) {
        return this.request(`/projects/${projectName}/domains/${domainName}/verify`, {
            method: 'POST',
        });
    }

    async getDomainChallenges(projectName, domainName) {
        return this.request(`/projects/${projectName}/domains/${domainName}/challenges`);
    }
}

const api = new RiseAPI();
