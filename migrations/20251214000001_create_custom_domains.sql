-- Create custom domains table
-- This allows projects to have custom domain names in addition to the default project URL
CREATE TABLE custom_domains (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    domain_name TEXT NOT NULL UNIQUE CHECK (domain_name ~ '^[a-zA-Z0-9]([a-zA-Z0-9\.\-]*[a-zA-Z0-9])?$'),
    -- Verification status
    verification_status TEXT NOT NULL CHECK (verification_status IN ('Pending', 'Verified', 'Failed')),
    verified_at TIMESTAMPTZ,
    -- Certificate information
    certificate_status TEXT NOT NULL CHECK (certificate_status IN ('None', 'Pending', 'Issued', 'Failed', 'Expired')) DEFAULT 'None',
    certificate_issued_at TIMESTAMPTZ,
    certificate_expires_at TIMESTAMPTZ,
    -- Store certificate data (PEM format) - will be encrypted at application level
    certificate_pem TEXT,
    certificate_key_pem TEXT,
    -- ACME order URL for tracking certificate issuance
    acme_order_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for fast lookups
CREATE INDEX idx_custom_domains_project_id ON custom_domains(project_id);
CREATE INDEX idx_custom_domains_verification_status ON custom_domains(verification_status);
CREATE INDEX idx_custom_domains_certificate_status ON custom_domains(certificate_status);

-- Create trigger to automatically update updated_at timestamp
-- Note: update_updated_at_column() is defined in 20251203000001_create_users.sql
CREATE TRIGGER update_custom_domains_updated_at
    BEFORE UPDATE ON custom_domains
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE custom_domains IS
  'Custom domain names for projects. Each domain requires DNS verification (CNAME) and certificate issuance via ACME.';
