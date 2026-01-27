-- Add is_protected column to project_env_vars table
-- Start with false default to avoid violating constraints on existing non-secret rows
ALTER TABLE project_env_vars
ADD COLUMN is_protected BOOLEAN NOT NULL DEFAULT false;

-- Add is_protected column to deployment_env_vars table
ALTER TABLE deployment_env_vars
ADD COLUMN is_protected BOOLEAN NOT NULL DEFAULT false;

-- Update all secret rows to have is_protected = true (protected by default)
UPDATE project_env_vars SET is_protected = true WHERE is_secret = true;
UPDATE deployment_env_vars SET is_protected = true WHERE is_secret = true;

-- Now change the default to true for new rows
ALTER TABLE project_env_vars ALTER COLUMN is_protected SET DEFAULT true;
ALTER TABLE deployment_env_vars ALTER COLUMN is_protected SET DEFAULT true;

-- Constraint: only secrets can have protection settings
-- (non-secrets are always "unprotected" by definition, so is_protected must be false)
ALTER TABLE project_env_vars
ADD CONSTRAINT project_env_vars_protected_check
CHECK (is_secret = true OR is_protected = false);

ALTER TABLE deployment_env_vars
ADD CONSTRAINT deployment_env_vars_protected_check
CHECK (is_secret = true OR is_protected = false);
