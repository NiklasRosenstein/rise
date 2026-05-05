-- Per-project admin-controlled deployment constraints (all nullable = inherit platform defaults)
ALTER TABLE projects
  ADD COLUMN min_replicas INTEGER,
  ADD COLUMN max_replicas INTEGER,
  ADD COLUMN min_cpu TEXT,
  ADD COLUMN max_cpu TEXT,
  ADD COLUMN min_memory TEXT,
  ADD COLUMN max_memory TEXT;

-- Concrete resource values resolved at deployment creation time
ALTER TABLE deployments
  ADD COLUMN replicas INTEGER NOT NULL DEFAULT 1,
  ADD COLUMN cpu TEXT NOT NULL DEFAULT '500m',
  ADD COLUMN memory TEXT NOT NULL DEFAULT '256Mi';
