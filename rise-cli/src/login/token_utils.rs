use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TokenClaims {
    exp: usize,
}

/// Extract expiration from JWT token and format it as a human-readable string
/// Returns formatted string like "December 16, 2025 at 14:30 UTC (in 7 days)"
pub fn format_token_expiration(token: &str) -> Result<String> {
    // Decode token WITHOUT signature validation (we only want to read the expiration)
    // We use an insecure validation since the backend has already validated the token
    let mut validation = Validation::new(Algorithm::RS256);
    validation.insecure_disable_signature_validation();
    validation.validate_exp = false; // Don't validate expiration during decode
    validation.validate_aud = false;

    // Use a dummy decoding key since we're not validating the signature
    let dummy_key = DecodingKey::from_secret(&[]);

    let token_data = decode::<TokenClaims>(token, &dummy_key, &validation)
        .context("Failed to decode token to read expiration")?;

    // Convert UNIX timestamp to DateTime
    let exp_timestamp = token_data.claims.exp as i64;
    let exp_datetime = DateTime::<Utc>::from_timestamp(exp_timestamp, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid expiration timestamp"))?;

    // Format absolute date/time
    let formatted_date = exp_datetime.format("%B %d, %Y at %H:%M UTC");

    // Calculate relative duration
    let now = Utc::now();
    let duration = exp_datetime.signed_duration_since(now);

    let relative = if duration.num_days() > 0 {
        let days = duration.num_days();
        if days == 1 {
            "in 1 day".to_string()
        } else {
            format!("in {} days", days)
        }
    } else if duration.num_hours() > 0 {
        let hours = duration.num_hours();
        if hours == 1 {
            "in 1 hour".to_string()
        } else {
            format!("in {} hours", hours)
        }
    } else if duration.num_minutes() > 0 {
        let minutes = duration.num_minutes();
        if minutes == 1 {
            "in 1 minute".to_string()
        } else {
            format!("in {} minutes", minutes)
        }
    } else {
        "expires soon".to_string()
    };

    Ok(format!("{} ({})", formatted_date, relative))
}
