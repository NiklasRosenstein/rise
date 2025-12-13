-- Create projects table
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE CHECK (name ~ '^[a-z0-9-]+$'),
    status TEXT NOT NULL CHECK (status IN ('Stopped', 'Running', 'Failed', 'Deploying')),
    visibility TEXT NOT NULL CHECK (visibility IN ('Public', 'Private')),
    owner_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    owner_team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensure exactly one of owner_user_id or owner_team_id is set
    CHECK ((owner_user_id IS NULL) != (owner_team_id IS NULL))
);

-- Create indexes for fast lookups
CREATE UNIQUE INDEX idx_projects_name ON projects(name);
CREATE INDEX idx_projects_owner_user ON projects(owner_user_id);
CREATE INDEX idx_projects_owner_team ON projects(owner_team_id);
CREATE INDEX idx_projects_status ON projects(status);

-- Create trigger to automatically update updated_at timestamp
CREATE TRIGGER update_projects_updated_at
    BEFORE UPDATE ON projects
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
