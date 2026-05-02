-- Shared OAuth transient state: replaces per-replica in-memory moka caches for OAuth flows.
-- lookup_key is a CSPRNG nonce, never an OAuth bearer/access token.
-- Covers: PKCE state, completed custom-domain sessions, extension OAuth state, extension auth codes.
CREATE TABLE oauth_transient_state (
    lookup_key VARCHAR(128) PRIMARY KEY,
    data       JSONB        NOT NULL,
    expires_at TIMESTAMPTZ  NOT NULL
);
CREATE INDEX idx_oauth_transient_state_expires ON oauth_transient_state(expires_at);

-- Leader leases: prevents background controller loops from running on every replica.
-- Each controller acquires the lease for its name; only one replica holds it at a time.
-- Lease expires automatically, so a crashed replica's lock is reclaimed within expires_at.
CREATE TABLE leader_leases (
    name         VARCHAR(64) PRIMARY KEY,
    holder_id    UUID        NOT NULL,
    heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at   TIMESTAMPTZ NOT NULL
);
