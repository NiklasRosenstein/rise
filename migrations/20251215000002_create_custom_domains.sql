-- Create project_custom_domains table for managing custom domains
CREATE TABLE project_custom_domains (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    domain TEXT NOT NULL UNIQUE CHECK (
        domain ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$'
        AND length(domain) <= 253
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, domain)
);

CREATE INDEX idx_custom_domains_project_id ON project_custom_domains(project_id);
CREATE INDEX idx_custom_domains_domain ON project_custom_domains(domain);

CREATE TRIGGER update_project_custom_domains_updated_at
    BEFORE UPDATE ON project_custom_domains
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
