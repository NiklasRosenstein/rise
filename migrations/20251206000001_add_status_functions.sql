-- Add PostgreSQL functions to centralize deployment status categorization
-- This eliminates hardcoded status lists across multiple queries

-- Function to check if a deployment status is terminal (no further transitions)
CREATE OR REPLACE FUNCTION is_terminal(status TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN status IN ('Cancelled', 'Stopped', 'Superseded', 'Failed', 'Expired');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to check if a deployment can be cancelled
-- Only deployments in pre-infrastructure states can be cancelled
CREATE OR REPLACE FUNCTION is_cancellable(status TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN status IN ('Pending', 'Building', 'Pushing', 'Pushed', 'Deploying');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to check if status is protected from reconciliation updates
-- Includes both transitional cleanup states and terminal states
CREATE OR REPLACE FUNCTION is_protected(status TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN status IN (
        'Terminating', 'Cancelling',
        'Cancelled', 'Stopped', 'Superseded', 'Failed', 'Expired'
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Function to check if deployment is in active running state
CREATE OR REPLACE FUNCTION is_active(status TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN status IN ('Healthy', 'Unhealthy');
END;
$$ LANGUAGE plpgsql IMMUTABLE;
