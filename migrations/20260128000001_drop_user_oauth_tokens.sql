-- Drop user_oauth_tokens table
-- Token caching has been removed in favor of stateless OAuth flow
-- Tokens are now stored temporarily in the authorization code cache (in-memory)
-- Clients own their tokens after exchange and manage refresh themselves

DROP TABLE IF EXISTS user_oauth_tokens;
