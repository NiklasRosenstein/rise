-- Environments: named deployment environments within a project (e.g., production, staging, dev)

-- 1. Create the environments table
CREATE TABLE environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    primary_deployment_group TEXT,
    is_default BOOLEAN NOT NULL DEFAULT false,
    is_production BOOLEAN NOT NULL DEFAULT false,
    color TEXT NOT NULL DEFAULT 'green',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (project_id, name),
    UNIQUE (project_id, primary_deployment_group),
    CONSTRAINT valid_environment_name CHECK (name ~ '^[a-z][a-z0-9-]*$' AND name !~ '--'),
    CONSTRAINT valid_environment_color CHECK (color IN ('green', 'blue', 'yellow', 'red', 'purple', 'orange', 'gray'))
);

-- Only one default per project
CREATE UNIQUE INDEX idx_environments_default ON environments(project_id) WHERE is_default = true;
-- Only one production per project
CREATE UNIQUE INDEX idx_environments_production ON environments(project_id) WHERE is_production = true;

CREATE TRIGGER update_environments_updated_at
    BEFORE UPDATE ON environments FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- 2. Add environment_id to deployments
ALTER TABLE deployments ADD COLUMN environment_id UUID REFERENCES environments(id) ON DELETE SET NULL;
CREATE INDEX idx_deployments_environment_id ON deployments(environment_id);

-- 3. Add environment_id to project_env_vars
ALTER TABLE project_env_vars ADD COLUMN environment_id UUID REFERENCES environments(id) ON DELETE CASCADE;
ALTER TABLE project_env_vars DROP CONSTRAINT project_env_vars_project_id_key_key;
CREATE UNIQUE INDEX idx_project_env_vars_unique_key
    ON project_env_vars(project_id, key, environment_id) NULLS NOT DISTINCT;

-- 4. Add allowed_environment_ids to service_accounts
ALTER TABLE service_accounts ADD COLUMN allowed_environment_ids UUID[];

-- 5. Data migration: create default "production" environment for all existing projects
INSERT INTO environments (project_id, name, primary_deployment_group, is_default, is_production)
SELECT id, 'production', 'default', true, true FROM projects;

UPDATE deployments d SET environment_id = e.id
FROM environments e
WHERE d.project_id = e.project_id AND d.deployment_group = 'default' AND e.name = 'production';

