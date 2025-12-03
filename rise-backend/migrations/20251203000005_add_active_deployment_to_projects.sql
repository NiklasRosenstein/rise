-- Add active_deployment_id to projects table
-- This tracks the currently active (successfully deployed) version of the project

ALTER TABLE projects
ADD COLUMN active_deployment_id UUID REFERENCES deployments(id) ON DELETE SET NULL;

-- Add index for faster lookups
CREATE INDEX idx_projects_active_deployment ON projects(active_deployment_id);

-- Add a helper function to get the last deployment for a project
-- This is useful for determining project status
CREATE OR REPLACE FUNCTION get_last_deployment_id(p_project_id UUID)
RETURNS UUID AS $$
    SELECT id FROM deployments
    WHERE project_id = p_project_id
    ORDER BY created_at DESC
    LIMIT 1;
$$ LANGUAGE SQL STABLE;
