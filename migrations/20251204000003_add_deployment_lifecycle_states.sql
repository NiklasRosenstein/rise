-- Add termination_reason enum to track why deployments were terminated
CREATE TYPE termination_reason AS ENUM ('UserStopped', 'Superseded', 'Cancelled');

-- Add termination_reason column to deployments
ALTER TABLE deployments ADD COLUMN termination_reason termination_reason NULL;

-- Update status constraint to include new lifecycle states
-- New states: Healthy, Unhealthy, Cancelling, Cancelled, Terminating, Stopped, Superseded
-- Keep Completed and Failed for backward compatibility during transition
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
    -- Legacy/Terminal states
    'Completed', 'Failed'
));

-- Index for finding healthy deployments (used in health checks)
CREATE INDEX idx_deployments_healthy ON deployments(status, updated_at)
WHERE status = 'Healthy';

-- Index for finding cancellable deployments (used when creating new deployment)
CREATE INDEX idx_deployments_cancellable ON deployments(project_id, created_at DESC)
WHERE status IN ('Pending', 'Building', 'Pushing', 'Pushed', 'Deploying');

-- Index for finding non-terminal deployments (used in reconciliation loop)
CREATE INDEX idx_deployments_non_terminal ON deployments(updated_at)
WHERE status NOT IN ('Cancelled', 'Stopped', 'Superseded', 'Completed', 'Failed');
