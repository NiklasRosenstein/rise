-- Create project_app_users junction table to track users who can access deployed apps
-- These users can access the deployed application via ingress auth but have no other project permissions
CREATE TABLE project_app_users (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, user_id)
);

-- Create indexes for efficient querying
CREATE INDEX idx_project_app_users_project ON project_app_users(project_id);
CREATE INDEX idx_project_app_users_user ON project_app_users(user_id);

-- Similar table for team-based app access
CREATE TABLE project_app_teams (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, team_id)
);

-- Create indexes for efficient querying
CREATE INDEX idx_project_app_teams_project ON project_app_teams(project_id);
CREATE INDEX idx_project_app_teams_team ON project_app_teams(team_id);
