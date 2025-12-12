use thiserror::Error;

#[derive(Debug, Error)]
pub enum OciError {
    #[error("Image not found: {0}")]
    ImageNotFound(String),

    #[error(
        "Private image requires authentication: {0}. Public images only are currently supported."
    )]
    PrivateImage(String),

    #[error("Invalid image reference: {0}")]
    InvalidReference(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Registry error: {0}")]
    Registry(String),
}
