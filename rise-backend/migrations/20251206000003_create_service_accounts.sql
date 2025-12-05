-- Service Accounts for Workload Identity
-- Service accounts allow external OIDC providers (like GitLab CI, GitHub Actions)
-- to authenticate and deploy to a specific project using JWT tokens

CREATE TABLE service_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    issuer_url TEXT NOT NULL,
    claims JSONB NOT NULL,
    sequence INTEGER NOT NULL,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique sequence per project (for email generation like my-app+1@sa.rise.local)
    CONSTRAINT unique_project_sequence UNIQUE (project_id, sequence)
);

-- Index for soft delete queries (filter out deleted_at IS NOT NULL)
CREATE INDEX idx_service_accounts_deleted_at ON service_accounts(deleted_at);

-- Index for finding by issuer during authentication (hot path)
CREATE INDEX idx_service_accounts_issuer_url ON service_accounts(issuer_url) WHERE deleted_at IS NULL;

-- Index for finding by user (for permission checks)
CREATE INDEX idx_service_accounts_user_id ON service_accounts(user_id) WHERE deleted_at IS NULL;
