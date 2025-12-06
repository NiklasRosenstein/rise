-- Add finalizers column to projects table
-- Finalizers prevent project deletion until cleanup controllers have processed them
-- Inspired by Kubernetes finalizer pattern

ALTER TABLE projects ADD COLUMN finalizers TEXT[] NOT NULL DEFAULT '{}';

-- Add index for finding projects with specific finalizers
CREATE INDEX idx_projects_finalizers ON projects USING GIN (finalizers);
