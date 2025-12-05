pub mod docker; // Note: File still named docker.rs but contains OciClientAuthProvider
pub mod ecr;

pub use docker::OciClientAuthProvider;
pub use ecr::EcrProvider;
