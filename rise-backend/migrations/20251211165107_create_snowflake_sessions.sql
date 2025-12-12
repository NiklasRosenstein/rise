-- Snowflake OAuth session management
-- Sessions track authenticated users, tokens are stored per project

CREATE TABLE rise_snowflake_sessions (
    session_id TEXT PRIMARY KEY,
    user_email TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE snowflake_app_tokens (
    session_id TEXT NOT NULL REFERENCES rise_snowflake_sessions(session_id) ON DELETE CASCADE,
    project_name TEXT NOT NULL,
    access_token_encrypted TEXT NOT NULL,
    refresh_token_encrypted TEXT NOT NULL,
    token_expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (session_id, project_name)
);

-- Index for finding tokens that need refresh
CREATE INDEX idx_snowflake_tokens_expires ON snowflake_app_tokens(token_expires_at);

-- Index for looking up sessions by user
CREATE INDEX idx_snowflake_sessions_user ON rise_snowflake_sessions(user_email);
