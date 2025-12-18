-- Create project_extensions table for extension system
CREATE TABLE project_extensions (
    project_id UUID NOT NULL REFERENCES projects(id),
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

-- Create function to soft-delete extensions when project enters Deleting status
CREATE OR REPLACE FUNCTION soft_delete_project_extensions()
RETURNS TRIGGER AS $$
BEGIN
    -- Only act when status changes to 'Deleting'
    IF NEW.status = 'Deleting' THEN
        -- Mark all extensions for this project as deleted
        UPDATE project_extensions
        SET deleted_at = NOW()
        WHERE project_id = NEW.id
          AND deleted_at IS NULL;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger that fires BEFORE project status update
CREATE TRIGGER soft_delete_extensions_on_deleting
    BEFORE UPDATE ON projects
    FOR EACH ROW
    EXECUTE FUNCTION soft_delete_project_extensions();
