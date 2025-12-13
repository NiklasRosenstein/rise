-- Add http_port column to deployments table
ALTER TABLE deployments
ADD COLUMN http_port INTEGER NOT NULL DEFAULT 8080;

-- Add constraint to validate port range (1-65535)
ALTER TABLE deployments
ADD CONSTRAINT valid_http_port
  CHECK (http_port >= 1 AND http_port <= 65535);
