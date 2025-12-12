pub mod docker; // Note: File still named docker.rs but contains OciClientAuthProvider

#[cfg(feature = "aws")]
pub mod ecr;

pub use docker::OciClientAuthProvider;

#[cfg(feature = "aws")]
pub use ecr::EcrProvider;
