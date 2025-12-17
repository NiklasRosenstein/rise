-- Create function to soft-delete extensions when project enters Terminating status
CREATE OR REPLACE FUNCTION soft_delete_project_extensions()
RETURNS TRIGGER AS $$
BEGIN
    -- Only act when status changes to 'Terminating' or 'Deleting'
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
CREATE TRIGGER soft_delete_extensions_on_terminating
    BEFORE UPDATE ON projects
    FOR EACH ROW
    EXECUTE FUNCTION soft_delete_project_extensions();
