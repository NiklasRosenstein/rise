-- Add deployment groups and expiration support
-- This migration adds the ability to organize deployments into groups (e.g., 'default', 'mr/27')
-- and set expiration timestamps for automatic cleanup.

-- Add deployment_group column with default value 'default'
-- This ensures existing deployments are automatically assigned to the default group
ALTER TABLE deployments
ADD COLUMN deployment_group TEXT NOT NULL DEFAULT 'default';

-- Add expires_at column for automatic deployment cleanup
-- NULL means the deployment never expires
ALTER TABLE deployments
ADD COLUMN expires_at TIMESTAMPTZ;

-- Validate group name format: must be 'default' or match [a-z0-9][a-z0-9/-]*[a-z0-9]
-- This ensures group names are lowercase, URL-safe, and follow a consistent pattern
ALTER TABLE deployments
ADD CONSTRAINT valid_group_name
  CHECK (deployment_group ~ '^[a-z0-9][a-z0-9/-]*[a-z0-9]$|^default$');

-- Index for efficient group-based queries
CREATE INDEX idx_deployments_project_group
  ON deployments(project_id, deployment_group);

-- Index for group and status queries
CREATE INDEX idx_deployments_group_status
  ON deployments(deployment_group, status);

-- Partial index for expiration cleanup queries
CREATE INDEX idx_deployments_expires_at
  ON deployments(expires_at)
  WHERE expires_at IS NOT NULL;

-- Add comments for documentation
COMMENT ON COLUMN deployments.deployment_group IS
  'Deployment group identifier (e.g., "default", "mr/27"). Only one active deployment per project+group combination.';

COMMENT ON COLUMN deployments.expires_at IS
  'Timestamp when deployment should be automatically terminated. NULL means no expiration.';
