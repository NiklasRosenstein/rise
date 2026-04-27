-- Add source_url to projects (URL to where the project code lives, e.g. a GitHub/GitLab repo)
ALTER TABLE projects ADD COLUMN source_url TEXT;

-- Add job_url and pull_request_url to deployments
ALTER TABLE deployments ADD COLUMN job_url TEXT;
ALTER TABLE deployments ADD COLUMN pull_request_url TEXT;
