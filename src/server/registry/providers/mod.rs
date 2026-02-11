pub mod docker; // Note: File still named docker.rs but contains OciClientAuthProvider

#[cfg(feature = "backend")]
pub mod ecr;

pub use docker::OciClientAuthProvider;

#[cfg(feature = "backend")]
pub use ecr::EcrProvider;
