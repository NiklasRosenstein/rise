-- Remove is_default from environments (default env selection moves to rise.toml)
DROP INDEX IF EXISTS idx_environments_default;
ALTER TABLE environments DROP COLUMN IF EXISTS is_default;

-- Add source column to deployment_env_vars for provenance tracking
ALTER TABLE deployment_env_vars ADD COLUMN IF NOT EXISTS source TEXT;
