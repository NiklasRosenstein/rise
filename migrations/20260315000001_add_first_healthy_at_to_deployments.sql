ALTER TABLE deployments
ADD COLUMN first_healthy_at TIMESTAMPTZ;

UPDATE deployments
SET first_healthy_at = COALESCE(updated_at, created_at, NOW())
WHERE first_healthy_at IS NULL
  AND status IN ('Healthy', 'Unhealthy');
