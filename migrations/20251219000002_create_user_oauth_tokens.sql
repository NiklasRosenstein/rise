-- Create table for storing user OAuth tokens (for token caching and automatic refresh)
CREATE TABLE user_oauth_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    extension VARCHAR NOT NULL,  -- e.g., "oauth-snowflake", "oauth-google"
    session_id VARCHAR NOT NULL,  -- From rise_oauth_session cookie (UUID)

    -- Encrypted tokens
    access_token_encrypted TEXT NOT NULL,
    refresh_token_encrypted TEXT,
    id_token_encrypted TEXT,

    -- Metadata
    expires_at TIMESTAMPTZ,
    last_refreshed_at TIMESTAMPTZ,
    last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),  -- Track user activity

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(project_id, extension, session_id)
);

-- Index for efficient token lookups
CREATE INDEX idx_user_oauth_tokens_lookup
  ON user_oauth_tokens(project_id, extension, session_id);

-- For token refresh job (find tokens that are expired but have refresh tokens)
CREATE INDEX idx_user_oauth_tokens_expiry
  ON user_oauth_tokens(expires_at)
  WHERE expires_at IS NOT NULL AND refresh_token_encrypted IS NOT NULL;

-- For token cleanup job (find inactive tokens)
CREATE INDEX idx_user_oauth_tokens_inactive
  ON user_oauth_tokens(last_accessed_at);
