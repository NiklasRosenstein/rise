-- Add is_primary column to project_custom_domains table
ALTER TABLE project_custom_domains
    ADD COLUMN is_primary BOOLEAN NOT NULL DEFAULT false;

-- Create partial unique index to ensure only one primary domain per project
-- This allows multiple non-primary domains (is_primary = false) while enforcing
-- uniqueness for primary domains (is_primary = true)
CREATE UNIQUE INDEX idx_custom_domains_primary_unique
    ON project_custom_domains(project_id)
    WHERE is_primary = true;

