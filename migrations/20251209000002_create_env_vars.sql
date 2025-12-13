-- Create project_env_vars table
CREATE TABLE project_env_vars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,              -- Encrypted if is_secret = true
    is_secret BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensure unique keys per project
    UNIQUE (project_id, key)
);

-- Create deployment_env_vars table (identical schema)
CREATE TABLE deployment_env_vars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    deployment_id UUID NOT NULL REFERENCES deployments(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,              -- Encrypted if is_secret = true
    is_secret BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensure unique keys per deployment
    UNIQUE (deployment_id, key)
);

-- Indexes for fast lookups
CREATE INDEX idx_project_env_vars_project_id ON project_env_vars(project_id);
CREATE INDEX idx_deployment_env_vars_deployment_id ON deployment_env_vars(deployment_id);

-- Triggers for updated_at
CREATE TRIGGER update_project_env_vars_updated_at
    BEFORE UPDATE ON project_env_vars
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_deployment_env_vars_updated_at
    BEFORE UPDATE ON deployment_env_vars
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
