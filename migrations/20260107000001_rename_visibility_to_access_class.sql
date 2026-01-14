-- Rename visibility column to access_class
ALTER TABLE projects RENAME COLUMN visibility TO access_class;

-- Drop the CHECK constraint on visibility (constraint name may vary)
ALTER TABLE projects DROP CONSTRAINT IF EXISTS projects_visibility_check;

-- Normalize existing values to lowercase for consistency
UPDATE projects SET access_class = LOWER(access_class);

-- No new CHECK constraint is added - validation happens at application layer
-- This allows deployment controllers to define custom access classes
