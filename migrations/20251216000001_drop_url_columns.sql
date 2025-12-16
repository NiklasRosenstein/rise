-- Drop URL columns from deployments and projects tables
-- URLs are now calculated dynamically using the deployment backend

-- Drop deployment_url from deployments table
ALTER TABLE deployments DROP COLUMN IF EXISTS deployment_url;

-- Drop project_url from projects table
ALTER TABLE projects DROP COLUMN IF EXISTS project_url;
