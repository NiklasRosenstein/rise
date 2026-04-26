-- Change owner_team_id FK constraint from ON DELETE CASCADE to ON DELETE RESTRICT
-- This prevents cascade deletion of projects when a team is deleted.
-- Teams that own projects must have their projects reassigned or deleted first.
ALTER TABLE projects DROP CONSTRAINT projects_owner_team_id_fkey;
ALTER TABLE projects ADD CONSTRAINT projects_owner_team_id_fkey
    FOREIGN KEY (owner_team_id) REFERENCES teams(id) ON DELETE RESTRICT;
