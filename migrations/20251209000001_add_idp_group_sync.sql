-- Add IdP group synchronization support to teams
--
-- This migration adds:
-- 1. idp_managed flag to track which teams are managed by Identity Provider
-- 2. Case-insensitive unique constraint on team names to prevent duplicates like "DevOps" and "devops"

-- Add idp_managed column to teams table
ALTER TABLE teams ADD COLUMN idp_managed BOOLEAN NOT NULL DEFAULT FALSE;

-- Add index for efficient filtering of IdP-managed teams
CREATE INDEX idx_teams_idp_managed ON teams(idp_managed);

-- Add comment explaining the column
COMMENT ON COLUMN teams.idp_managed IS
    'Whether this team is managed by an Identity Provider.
     When true, membership and ownership are controlled by IdP groups claim.
     Only administrators can modify IdP-managed teams.';

-- Drop the existing case-sensitive unique index on name
DROP INDEX idx_teams_name;

-- Create case-insensitive unique index on team name
-- This prevents having both "DevOps" and "devops" as separate teams
CREATE UNIQUE INDEX idx_teams_name_lower ON teams(LOWER(name));

COMMENT ON INDEX idx_teams_name_lower IS
    'Ensures team names are unique (case-insensitive).
     Prevents creating both "DevOps" and "devops" as separate teams.';
