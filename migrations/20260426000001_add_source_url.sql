-- Add source_url to projects (URL to where the project code lives, e.g. a GitHub/GitLab repo)
ALTER TABLE projects ADD COLUMN source_url TEXT;

-- Add source_url to deployments (URL to the source of the deployment, e.g. a CI job URL)
ALTER TABLE deployments ADD COLUMN source_url TEXT;
