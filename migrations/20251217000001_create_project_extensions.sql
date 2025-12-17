-- Create project_extensions table
CREATE TABLE project_extensions (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    extension VARCHAR NOT NULL,
    spec JSONB NOT NULL,
    status JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (project_id, extension)
);

-- Indexes for fast lookups
CREATE INDEX idx_project_extensions_project_id ON project_extensions(project_id);
CREATE INDEX idx_project_extensions_deleted_at ON project_extensions(deleted_at) WHERE deleted_at IS NULL;

-- Trigger for updated_at
CREATE TRIGGER update_project_extensions_updated_at
    BEFORE UPDATE ON project_extensions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
