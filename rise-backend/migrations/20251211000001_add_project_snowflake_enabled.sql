-- Add snowflake_enabled flag to projects
-- When true, Rise handles Snowflake OAuth and injects X-Snowflake-Token header
ALTER TABLE projects ADD COLUMN snowflake_enabled BOOLEAN NOT NULL DEFAULT FALSE;
