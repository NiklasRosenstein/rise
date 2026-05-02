ALTER TABLE oauth_transient_state
    ADD COLUMN claimed_at TIMESTAMPTZ NULL,
    ADD COLUMN claim_expires_at TIMESTAMPTZ NULL,
    ADD COLUMN claimed_by UUID NULL;

CREATE INDEX idx_oauth_transient_state_claim_expires
    ON oauth_transient_state(claim_expires_at);
