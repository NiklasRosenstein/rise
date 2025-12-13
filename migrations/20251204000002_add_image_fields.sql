-- Add image and image_digest fields to deployments table
--
-- image: User-provided image reference (e.g., "nginx:latest", "nginx")
-- image_digest: Resolved digest-pinned image (e.g., "docker.io/library/nginx@sha256:abc123...")
--
-- Both NULL = image built from source and pushed to registry
-- Both set = pre-built image deployment

ALTER TABLE deployments
ADD COLUMN image TEXT NULL,
ADD COLUMN image_digest TEXT NULL;

-- Add index for querying deployments by image
CREATE INDEX idx_deployments_image ON deployments(image) WHERE image IS NOT NULL;
