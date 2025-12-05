-- Add 'Failed' variant to termination_reason enum
-- This is used for deployments that time out or fail to become healthy

ALTER TYPE termination_reason ADD VALUE 'Failed';
