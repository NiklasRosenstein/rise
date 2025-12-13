-- Add 'Terminated' status to project status CHECK constraint
ALTER TABLE projects
DROP CONSTRAINT IF EXISTS projects_status_check;

ALTER TABLE projects
ADD CONSTRAINT projects_status_check
CHECK (status IN ('Stopped', 'Running', 'Failed', 'Deploying', 'Deleting', 'Terminated'));
