use crate::db::models::User;
use crate::db::service_accounts;
use crate::server::auth::jwt::JwtValidator;
use crate::server::error::{ServerError, ServerErrorExt};
use crate::server::state::AppState;
use axum::{extract::FromRequestParts, http::request::Parts};
use std::collections::HashMap;

/// A JWKS-validated external token (phase 1 of two-phase SA auth).
///
/// Stored in request extensions by the auth middleware after verifying the JWT
/// signature and expiry via JWKS. Custom claims have NOT been validated at this point;
/// that happens in phase 2 when the target project is known.
#[derive(Clone, Debug)]
pub struct VerifiedExternalToken {
    pub issuer: String,
    pub claims: serde_json::Value,
}

/// Authentication context for request handlers.
///
/// This replaces `Extension<User>` in all handlers and supports two-phase
/// service account authentication:
/// - `User`: A Rise JWT was validated — the user is known immediately.
/// - `ExternalToken`: An external JWT was JWKS-validated (signature + expiry).
///   The token's claims still need to be matched against a project's service
///   accounts via `resolve_for_project`.
#[derive(Clone, Debug)]
pub enum AuthContext {
    User(User),
    ExternalToken(VerifiedExternalToken),
}

impl AuthContext {
    /// Get the authenticated Rise user.
    ///
    /// Returns the user for Rise JWTs. Returns 401 for external tokens
    /// (endpoints that don't support service account authentication should call this).
    pub fn user(&self) -> Result<&User, ServerError> {
        match self {
            AuthContext::User(user) => Ok(user),
            AuthContext::ExternalToken(_) => Err(ServerError::unauthorized(
                "This endpoint does not support service account authentication",
            )),
        }
    }

    /// Resolve authentication for a project-scoped endpoint.
    ///
    /// - For Rise JWTs: returns `(user, false)` (the bool indicates `is_service_account`).
    /// - For external tokens: looks up service accounts for `(project_id, issuer)`,
    ///   validates claims against each SA's expected claims, and returns the SA's
    ///   synthetic user on first match. Error messages include expected claim values
    ///   since they are scoped to the same project (no cross-project leakage).
    pub async fn resolve_for_project(
        &self,
        state: &AppState,
        project: &crate::db::models::Project,
    ) -> Result<(User, bool), ServerError> {
        match self {
            AuthContext::User(user) => Ok((user.clone(), false)),
            AuthContext::ExternalToken(token) => {
                // Find service accounts for this project + issuer
                let service_accounts = service_accounts::find_by_project_and_issuer(
                    &state.db_pool,
                    project.id,
                    &token.issuer,
                )
                .await
                .internal_err("Failed to look up service accounts")?;

                if service_accounts.is_empty() {
                    return Err(ServerError::unauthorized(format!(
                        "No service accounts configured for issuer '{}' on project '{}'",
                        token.issuer, project.name
                    )));
                }

                // Try each SA's expected claims against the token
                let mut matching_sas = Vec::new();
                let mut last_error = None;
                for sa in &service_accounts {
                    let expected_claims: HashMap<String, String> =
                        match serde_json::from_value(sa.claims.clone()) {
                            Ok(claims) => claims,
                            Err(e) => {
                                tracing::error!(
                                    "Failed to deserialize claims for service account {}: {}",
                                    sa.id,
                                    e
                                );
                                return Err(ServerError::internal(
                                    "Invalid service account claims configuration",
                                ));
                            }
                        };

                    match JwtValidator::validate_custom_claims(&token.claims, &expected_claims) {
                        Ok(()) => {
                            matching_sas.push(sa);
                        }
                        Err(e) => {
                            tracing::debug!("SA {} claim mismatch: {}", sa.id, e);
                            last_error = Some(e);
                        }
                    }
                }

                if matching_sas.is_empty() {
                    // No SA matched — include validation details (safe: same project)
                    return Err(ServerError::unauthorized(format!(
                        "Token claims do not match any service account for project '{}': {}",
                        project.name,
                        last_error
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "unknown error".to_string()),
                    )));
                }

                if matching_sas.len() > 1 {
                    let sa_ids: Vec<String> =
                        matching_sas.iter().map(|sa| sa.id.to_string()).collect();
                    tracing::error!(
                        "Multiple service accounts matched JWT on project '{}': {:?}. \
                         This indicates ambiguous claim configuration.",
                        project.name,
                        sa_ids
                    );
                    return Err(ServerError::conflict(
                        "Multiple service accounts match the provided claims",
                    ));
                }

                // Exactly one match — look up the SA's synthetic user
                let sa = matching_sas[0];
                let user = crate::db::users::find_by_id(&state.db_pool, sa.user_id)
                    .await
                    .internal_err("Failed to look up service account user")?
                    .ok_or_else(|| {
                        ServerError::internal("Service account user not found in database")
                    })?;

                tracing::info!(
                    "Service account {} authenticated for project '{}'",
                    user.email,
                    project.name
                );

                Ok((user, true))
            }
        }
    }

    /// Returns `true` if this auth context represents a service account token
    /// (i.e. an external JWT that has not yet been resolved to a project).
    ///
    /// After calling `resolve_for_project`, use the returned `is_sa` bool instead.
    pub fn is_service_account(&self) -> bool {
        matches!(self, AuthContext::ExternalToken(_))
    }
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ServerError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try User extension first (Rise JWT path)
        if let Some(user) = parts.extensions.get::<User>().cloned() {
            return Ok(AuthContext::User(user));
        }

        // Try VerifiedExternalToken extension (external JWT path)
        if let Some(token) = parts.extensions.get::<VerifiedExternalToken>().cloned() {
            return Ok(AuthContext::ExternalToken(token));
        }

        // Neither was set — middleware should have rejected the request
        Err(ServerError::unauthorized("Not authenticated"))
    }
}
