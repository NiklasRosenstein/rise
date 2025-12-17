-- Create function to soft-delete extensions when project is about to be deleted
CREATE OR REPLACE FUNCTION soft_delete_project_extensions()
RETURNS TRIGGER AS $$
BEGIN
    -- Mark all extensions for this project as deleted
    UPDATE project_extensions
    SET deleted_at = NOW()
    WHERE project_id = OLD.id
      AND deleted_at IS NULL;
    
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

-- Create trigger that fires BEFORE project deletion
CREATE TRIGGER soft_delete_extensions_before_project_delete
    BEFORE DELETE ON projects
    FOR EACH ROW
    EXECUTE FUNCTION soft_delete_project_extensions();
