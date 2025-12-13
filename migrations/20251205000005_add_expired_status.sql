-- Add Expired to termination_reason enum
ALTER TYPE termination_reason ADD VALUE 'Expired';

-- Add Expired to deployment status constraint
ALTER TABLE deployments DROP CONSTRAINT IF EXISTS deployments_status_check;
ALTER TABLE deployments ADD CONSTRAINT deployments_status_check
CHECK (status IN (
    -- Build/Deploy states
    'Pending', 'Building', 'Pushing', 'Pushed', 'Deploying',
    -- Running states
    'Healthy', 'Unhealthy',
    -- Cancellation states (pre-infrastructure)
    'Cancelling', 'Cancelled',
    -- Termination states (post-infrastructure)
    'Terminating', 'Stopped', 'Superseded',
    -- Terminal states
    'Completed', 'Failed', 'Expired'
));
