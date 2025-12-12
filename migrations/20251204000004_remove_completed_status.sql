-- Migration: Remove legacy 'Completed' status
-- Convert all Completed deployments to Healthy and remove Completed from valid statuses

-- Step 1: Convert all Completed deployments to Healthy
UPDATE deployments
SET status = 'Healthy'
WHERE status = 'Completed';

-- Step 2: Update status constraint to exclude Completed
ALTER TABLE deployments DROP CONSTRAINT IF EXISTS deployments_status_check;
ALTER TABLE deployments ADD CONSTRAINT deployments_status_check
CHECK (status IN (
    'Pending', 'Building', 'Pushing', 'Pushed', 'Deploying',
    'Healthy', 'Unhealthy', 'Cancelling', 'Cancelled',
    'Terminating', 'Stopped', 'Superseded',
    'Failed'
));
