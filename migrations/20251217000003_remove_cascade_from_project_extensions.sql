-- Remove CASCADE from project_extensions foreign key
-- This allows us to control extension cleanup via soft-delete and reconciliation
-- instead of having PostgreSQL automatically delete extension records

ALTER TABLE project_extensions
DROP CONSTRAINT project_extensions_project_id_fkey;

ALTER TABLE project_extensions
ADD CONSTRAINT project_extensions_project_id_fkey
FOREIGN KEY (project_id) REFERENCES projects(id);
