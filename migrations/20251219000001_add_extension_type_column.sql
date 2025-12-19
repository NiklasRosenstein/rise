-- Add extension_type column to project_extensions table
-- This migration separates the concept of extension type (handler) from extension name (instance)
-- allowing multiple instances of the same extension type per project

-- Add the extension_type column (nullable initially)
ALTER TABLE project_extensions
  ADD COLUMN extension_type VARCHAR;

-- Backfill existing rows with 'aws-rds-provisioner' (the only existing extension type)
UPDATE project_extensions
  SET extension_type = 'aws-rds-provisioner'
  WHERE extension_type IS NULL;

-- Make the column NOT NULL now that all rows have values
ALTER TABLE project_extensions
  ALTER COLUMN extension_type SET NOT NULL;

-- Create index for efficient lookups by extension type
CREATE INDEX idx_project_extensions_type
  ON project_extensions(extension_type);
