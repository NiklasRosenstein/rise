-- Create ACME challenges table
-- This tracks DNS-01 challenges for domain verification and certificate issuance
CREATE TABLE acme_challenges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain_id UUID NOT NULL REFERENCES custom_domains(id) ON DELETE CASCADE,
    challenge_type TEXT NOT NULL CHECK (challenge_type IN ('dns-01', 'http-01')),
    -- DNS record name (e.g., "_acme-challenge.example.com")
    record_name TEXT NOT NULL,
    -- DNS record value/token that needs to be configured
    record_value TEXT NOT NULL,
    -- Challenge status
    status TEXT NOT NULL CHECK (status IN ('Pending', 'Valid', 'Invalid', 'Expired')),
    -- ACME authorization URL
    authorization_url TEXT,
    -- When the challenge was validated
    validated_at TIMESTAMPTZ,
    -- Challenge expiry (ACME challenges typically expire after a few days)
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for fast lookups
CREATE INDEX idx_acme_challenges_domain_id ON acme_challenges(domain_id);
CREATE INDEX idx_acme_challenges_status ON acme_challenges(status);

-- Create trigger to automatically update updated_at timestamp
CREATE TRIGGER update_acme_challenges_updated_at
    BEFORE UPDATE ON acme_challenges
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE acme_challenges IS
  'ACME challenges for domain verification. Tracks DNS-01 challenges required for certificate issuance.';
