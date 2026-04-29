-- Remove is_default from environments (default env selection moves to rise.toml)
DROP INDEX IF EXISTS idx_environments_default;
ALTER TABLE environments DROP COLUMN IF EXISTS is_default;

-- Add source column to deployment_env_vars for provenance tracking
-- Valid values: system, global, env:<name>, extension, toml, cli
ALTER TABLE deployment_env_vars ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'system';
