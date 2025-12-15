-- Add needs_reconcile flag to trigger reconciliation of Healthy/Unhealthy deployments
-- Used when configuration changes (custom domains, env vars) require updating
-- infrastructure for already-deployed applications

ALTER TABLE deployments
ADD COLUMN needs_reconcile BOOLEAN NOT NULL DEFAULT FALSE;

-- Create index for efficient querying of deployments that need reconciliation
CREATE INDEX idx_deployments_needs_reconcile ON deployments(needs_reconcile) WHERE needs_reconcile = TRUE;
