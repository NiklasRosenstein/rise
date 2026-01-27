-- Add is_retrievable column to project_env_vars table
ALTER TABLE project_env_vars
ADD COLUMN is_retrievable BOOLEAN NOT NULL DEFAULT false;

-- Add is_retrievable column to deployment_env_vars table
ALTER TABLE deployment_env_vars
ADD COLUMN is_retrievable BOOLEAN NOT NULL DEFAULT false;

-- Constraint: only secrets can be retrievable (non-secrets are already retrievable by definition)
ALTER TABLE project_env_vars
ADD CONSTRAINT project_env_vars_retrievable_check
CHECK (is_secret = true OR is_retrievable = false);

ALTER TABLE deployment_env_vars
ADD CONSTRAINT deployment_env_vars_retrievable_check
CHECK (is_secret = true OR is_retrievable = false);
