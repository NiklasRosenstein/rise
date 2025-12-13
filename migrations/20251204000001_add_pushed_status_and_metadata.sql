-- Add 'Pushed' status between 'Pushing' and 'Deploying'
ALTER TABLE deployments DROP CONSTRAINT IF EXISTS deployments_status_check;
ALTER TABLE deployments ADD CONSTRAINT deployments_status_check
CHECK (status IN ('Pending', 'Building', 'Pushing', 'Pushed', 'Deploying', 'Completed', 'Failed'));

-- Add JSONB field for controller-specific metadata
-- This allows each controller implementation (Docker, K8s, etc.) to store its own data
ALTER TABLE deployments ADD COLUMN controller_metadata JSONB DEFAULT '{}'::jsonb;

-- Add deployment URL field for easy access
ALTER TABLE deployments ADD COLUMN deployment_url TEXT;

-- Create partial index for efficient controller polling
-- This index helps the controller quickly find deployments that need reconciliation
CREATE INDEX IF NOT EXISTS idx_deployments_status_pushed
ON deployments(status) WHERE status = 'Pushed';
