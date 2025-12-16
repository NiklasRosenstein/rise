-- Add rolled_back_from_deployment_id field to track rollback source
--
-- This field tracks which deployment a rollback deployment was created from.
-- It's used to calculate the correct image tag for build-from-source rollbacks,
-- since the new deployment doesn't build its own image but reuses the source deployment's image.
--
-- NULL = regular deployment (not a rollback)
-- Set = rollback deployment (references the source deployment)

ALTER TABLE deployments
ADD COLUMN rolled_back_from_deployment_id UUID NULL
    REFERENCES deployments(id) ON DELETE SET NULL;

-- Add index for querying rollback deployments
CREATE INDEX idx_deployments_rolled_back_from ON deployments(rolled_back_from_deployment_id)
    WHERE rolled_back_from_deployment_id IS NOT NULL;

-- Add comment for documentation
COMMENT ON COLUMN deployments.rolled_back_from_deployment_id IS
    'References the source deployment ID when this deployment was created via rollback. Used to determine the correct image tag for build-from-source rollbacks.';
