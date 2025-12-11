-- Add idp_managed column to teams table
-- This column indicates whether team membership is managed by an Identity Provider
ALTER TABLE teams ADD COLUMN idp_managed BOOLEAN NOT NULL DEFAULT FALSE;

-- Create index for filtering by idp_managed status
CREATE INDEX idx_teams_idp_managed ON teams(idp_managed);
