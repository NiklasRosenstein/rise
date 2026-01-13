-- Trigger to automatically mark deployments for reconciliation when project access_class changes
--
-- When a project's access_class is updated, all active (Healthy/Unhealthy) deployments
-- need to be reconciled so their Ingress resources get updated with new authentication
-- annotations.

-- Function that marks deployments as needing reconciliation
CREATE OR REPLACE FUNCTION mark_deployments_on_access_class_change()
RETURNS TRIGGER AS $$
BEGIN
    -- Only proceed if access_class actually changed
    IF OLD.access_class IS DISTINCT FROM NEW.access_class THEN
        -- Mark all Healthy/Unhealthy deployments for this project
        UPDATE deployments
        SET needs_reconcile = TRUE,
            updated_at = NOW()
        WHERE project_id = NEW.id
          AND status IN ('Healthy', 'Unhealthy');

        -- Log the change for debugging
        RAISE NOTICE 'Marked deployments for project % (%) for reconciliation due to access_class change: % -> %',
            NEW.id, NEW.name, OLD.access_class, NEW.access_class;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger that fires after project updates
CREATE TRIGGER trigger_access_class_reconcile
    AFTER UPDATE ON projects
    FOR EACH ROW
    WHEN (OLD.access_class IS DISTINCT FROM NEW.access_class)
    EXECUTE FUNCTION mark_deployments_on_access_class_change();
