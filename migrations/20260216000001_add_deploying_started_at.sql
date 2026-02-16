-- Add deploying_started_at timestamp to track when deployment enters Deploying status
-- This allows accurate timeout measurement for the deployment phase only, not including build/push time

ALTER TABLE deployments
ADD COLUMN deploying_started_at TIMESTAMPTZ;
