-- Add platform access control to users table
--
-- This migration adds the is_platform_user column to distinguish between:
-- - Platform users: Can use Rise Dashboard/CLI/API to deploy and manage applications
-- - Application users: Can only authenticate to access deployed applications (ingress auth)

ALTER TABLE users
ADD COLUMN is_platform_user BOOLEAN NOT NULL DEFAULT true;

CREATE INDEX idx_users_is_platform_user ON users(is_platform_user);

COMMENT ON COLUMN users.is_platform_user IS
  'Whether user has access to Rise platform features (CLI/API/Dashboard). '
  'When false, user can only authenticate to access deployed applications.';
