-- Add is_active column to deployments table
ALTER TABLE deployments
ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT FALSE;

-- Create unique partial index to enforce constraint:
-- Only one active deployment per (project_id, deployment_group)
CREATE UNIQUE INDEX idx_deployments_active_per_project_group
ON deployments(project_id, deployment_group)
WHERE is_active = TRUE;

-- Create index for efficient querying of active deployments
CREATE INDEX idx_deployments_active
ON deployments(is_active)
WHERE is_active = TRUE;

-- Drop projects.active_deployment_id column as it's now replaced by deployments.is_active
ALTER TABLE projects
DROP COLUMN active_deployment_id;
