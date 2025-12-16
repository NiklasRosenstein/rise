-- Trigger to automatically clear is_active when deployment reaches terminal state
-- This ensures deployments in terminal states (Cancelled, Stopped, Superseded, Failed, Expired)
-- cannot be marked as active, which would be a data integrity issue

CREATE OR REPLACE FUNCTION clear_is_active_on_terminal()
RETURNS TRIGGER AS $$
BEGIN
    -- If the new status is terminal and is_active is still true, clear it
    IF is_terminal(NEW.status) AND NEW.is_active = TRUE THEN
        NEW.is_active = FALSE;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger that fires BEFORE UPDATE or INSERT
-- This ensures is_active is always false for terminal states
CREATE TRIGGER ensure_terminal_not_active
    BEFORE INSERT OR UPDATE OF status, is_active ON deployments
    FOR EACH ROW
    EXECUTE FUNCTION clear_is_active_on_terminal();
