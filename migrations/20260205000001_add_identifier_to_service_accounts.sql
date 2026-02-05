-- Add identifier column to service_accounts for declarative configuration
-- Allows specifying identifiers instead of auto-incrementing sequence numbers
-- Email format becomes: {project_name}-sa+{identifier}@rise.local

-- Add identifier column (nullable for backward compatibility)
ALTER TABLE service_accounts
ADD COLUMN identifier TEXT;

-- Add unique constraint on (project_id, identifier) to prevent duplicates
-- Only applies when identifier is not NULL (for declarative service accounts)
CREATE UNIQUE INDEX unique_project_identifier 
ON service_accounts(project_id, identifier) 
WHERE identifier IS NOT NULL AND deleted_at IS NULL;

-- Add check constraint to ensure identifier format (alphanumeric, hyphens, underscores only)
-- Must start and end with alphanumeric (or be a single alphanumeric character)
ALTER TABLE service_accounts
ADD CONSTRAINT identifier_format_check
CHECK (identifier IS NULL OR identifier ~ '^[a-z0-9]([a-z0-9_-]*[a-z0-9])?$');

-- Comment on column
COMMENT ON COLUMN service_accounts.identifier IS 'Optional identifier for declarative service accounts (e.g., "ci", "staging"). When set, email format is {project}-sa+{identifier}@rise.local instead of {project}+{sequence}@sa.rise.local';
