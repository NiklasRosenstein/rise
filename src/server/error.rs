use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Server error type that provides automatic logging and clean error responses.
///
/// This type:
/// - Automatically logs errors when converted to HTTP responses (via IntoResponse)
/// - Preserves full error chains from anyhow::Error for debugging
/// - Allows attaching structured context (user IDs, project names, etc.)
/// - Returns clean, user-friendly error messages to clients
///
/// # Example
///
/// ```rust,ignore
/// use crate::server::error::{ServerError, ServerErrorExt};
///
/// // Simple error with just a message
/// let err = ServerError::bad_request("Invalid project name");
///
/// // Error from anyhow with context
/// let result: Result<_, anyhow::Error> = fetch_user();
/// let user = result
///     .internal_err("Failed to fetch user")?
///     .with_context("user_id", user_id.to_string());
///
/// // Error with full context
/// let err = ServerError::from_anyhow(
///     anyhow!("Database connection failed"),
///     StatusCode::INTERNAL_SERVER_ERROR,
///     "Failed to connect to database"
/// )
/// .with_context("operation", "create_project")
/// .with_context("project_name", &project_name);
/// ```
#[derive(Debug)]
pub struct ServerError {
    /// HTTP status code to return
    pub status: StatusCode,
    /// User-facing error message (returned in response)
    pub message: String,
    /// Internal error with full chain (logged but not exposed to client)
    pub source: Option<anyhow::Error>,
    /// Structured context for logging (key-value pairs)
    pub context: Vec<(&'static str, String)>,
}

impl ServerError {
    /// Create a new error with just status and message (no source error)
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            source: None,
            context: Vec::new(),
        }
    }

    /// Create an error from an anyhow::Error with full error chain
    pub fn from_anyhow(
        source: anyhow::Error,
        status: StatusCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            source: Some(source),
            context: Vec::new(),
        }
    }

    /// Add a context field for logging (chainable)
    pub fn with_context(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.context.push((key, value.into()));
        self
    }

    /// Create a 500 Internal Server Error
    #[allow(dead_code)]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Create a 500 Internal Server Error from an anyhow::Error
    pub fn internal_anyhow(source: anyhow::Error, message: impl Into<String>) -> Self {
        Self::from_anyhow(source, StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Create a 400 Bad Request error
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// Create a 403 Forbidden error
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    /// Create a 404 Not Found error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        // Log server errors (5xx) with full context using structured fields
        if self.status.is_server_error() {
            // Log with structured fields to prevent log injection
            if let Some(source) = &self.source {
                tracing::error!(
                    status = self.status.as_u16(),
                    message = %self.message,
                    context = ?self.context,
                    error = ?source,
                    "Server error"
                );
            } else {
                tracing::error!(
                    status = self.status.as_u16(),
                    message = %self.message,
                    context = ?self.context,
                    "Server error"
                );
            }
        }

        // Return clean JSON error response to client
        let body = Json(json!({
            "error": self.message,
        }));

        (self.status, body).into_response()
    }
}

// Implement From for common error types
impl From<sqlx::Error> for ServerError {
    fn from(err: sqlx::Error) -> Self {
        Self::internal_anyhow(err.into(), "Database operation failed")
    }
}

impl From<anyhow::Error> for ServerError {
    fn from(err: anyhow::Error) -> Self {
        Self::internal_anyhow(err, "Internal server error")
    }
}

/// Extension trait for Result types to easily convert to ServerError
///
/// This trait provides ergonomic methods for converting Result<T, E> to Result<T, ServerError>
/// where E can be converted to anyhow::Error.
///
/// # Example
///
/// ```rust,ignore
/// use crate::server::error::ServerErrorExt;
///
/// // Convert with custom status and message
/// let result = some_operation()
///     .server_err(StatusCode::BAD_REQUEST, "Invalid operation")?;
///
/// // Convert to internal server error (500)
/// let result = database_query()
///     .internal_err("Failed to query database")?;
/// ```
pub trait ServerErrorExt<T> {
    /// Convert error to ServerError with custom status and message
    fn server_err(self, status: StatusCode, message: impl Into<String>) -> Result<T, ServerError>;

    /// Convert error to internal server error (500)
    fn internal_err(self, message: impl Into<String>) -> Result<T, ServerError>;
}

impl<T, E> ServerErrorExt<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    fn server_err(self, status: StatusCode, message: impl Into<String>) -> Result<T, ServerError> {
        self.map_err(|e| ServerError::from_anyhow(e.into(), status, message))
    }

    fn internal_err(self, message: impl Into<String>) -> Result<T, ServerError> {
        self.map_err(|e| ServerError::internal_anyhow(e.into(), message))
    }
}
