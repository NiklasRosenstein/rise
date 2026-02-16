-- Add deploying_started_at timestamp to track when deployment enters Deploying status
-- This allows accurate timeout measurement for the deployment phase only, not including build/push time

ALTER TABLE deployments
ADD COLUMN deploying_started_at TIMESTAMPTZ;

-- Backfill deploying_started_at for existing deployments currently in Deploying status
-- Use updated_at when available to approximate when the deployment entered Deploying;
-- fall back to NOW() if updated_at is NULL.
UPDATE deployments
SET deploying_started_at = COALESCE(updated_at, NOW())
WHERE status = 'Deploying'
  AND deploying_started_at IS NULL;
