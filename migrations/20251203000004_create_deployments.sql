-- Create deployments table
CREATE TABLE deployments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    deployment_id TEXT NOT NULL,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_by_id UUID NOT NULL REFERENCES users(id),
    status TEXT NOT NULL CHECK (status IN ('Pending', 'Building', 'Pushing', 'Deploying', 'Completed', 'Failed')),
    completed_at TIMESTAMPTZ,
    error_message TEXT,
    build_logs TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensure deployment_id is unique per project
    UNIQUE (deployment_id, project_id)
);

-- Create indexes for fast lookups
CREATE INDEX idx_deployments_deployment_id ON deployments(deployment_id);
CREATE INDEX idx_deployments_project ON deployments(project_id);
CREATE INDEX idx_deployments_status ON deployments(status);
CREATE INDEX idx_deployments_created_by ON deployments(created_by_id);
-- Compound index for efficient "latest deployments for project" queries
CREATE INDEX idx_deployments_project_created ON deployments(project_id DESC, created_at DESC);

-- Create trigger to automatically update updated_at timestamp
CREATE TRIGGER update_deployments_updated_at
    BEFORE UPDATE ON deployments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
