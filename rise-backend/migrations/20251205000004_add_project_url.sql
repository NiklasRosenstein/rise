-- Add project URL field for stable project access
-- This migration adds a separate project_url field that is controller-assigned
-- and independent of individual deployment URLs.

-- Add project_url column
-- NULL initially for existing projects; will be set by controller when deployments become active
ALTER TABLE projects
ADD COLUMN project_url TEXT;

-- Add comment for documentation
COMMENT ON COLUMN projects.project_url IS
  'Stable URL assigned by deployment controller that routes to the active deployment in the default group. For Docker: http://localhost:PORT, for Kubernetes: https://project-name.rise.net';
