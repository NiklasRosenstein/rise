use super::error::OciError;
use anyhow::Result;
use oci_distribution::{
    client::{ClientConfig, ClientProtocol},
    secrets::RegistryAuth,
    Client, Reference,
};
use tracing::{debug, info, warn};

pub struct OciClient {
    client: Client,
}

impl OciClient {
    pub fn new() -> Result<Self> {
        // Configure client to allow HTTP for localhost registries (common in dev)
        // while still requiring HTTPS for public registries
        let config = ClientConfig {
            protocol: ClientProtocol::HttpsExcept(vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "localhost:5000".to_string(),
            ]),
            ..Default::default()
        };

        let client = Client::new(config);
        info!("Initialized OCI client with insecure registries allowed for localhost");

        Ok(Self { client })
    }

    /// Resolve image reference to digest-pinned reference
    /// Uses OCI Distribution API to fetch manifest only (~2-10KB)
    pub async fn resolve_image_digest(&self, image_ref: &str) -> Result<String, OciError> {
        debug!("Attempting to resolve image reference: {}", image_ref);

        // Parse image reference
        let reference = Reference::try_from(image_ref).map_err(|e| {
            warn!("Failed to parse image reference '{}': {}", image_ref, e);
            OciError::InvalidReference(format!("{}: {}", image_ref, e))
        })?;

        debug!(
            "Parsed reference - registry: {}, repository: {}, tag: {:?}",
            reference.registry(),
            reference.repository(),
            reference.tag()
        );

        // Fetch manifest (anonymous access for public images)
        let auth = RegistryAuth::Anonymous;
        debug!("Fetching manifest for {} using anonymous auth", image_ref);

        let (_manifest, digest) =
            self.client
                .pull_manifest(&reference, &auth)
                .await
                .map_err(|e| {
                    warn!(
                        "Failed to pull manifest for '{}' from registry '{}': {}",
                        image_ref,
                        reference.registry(),
                        e
                    );
                    self.classify_error(e, image_ref)
                })?;

        debug!("Successfully fetched manifest with digest: {}", digest);

        // Construct digest-pinned reference using Reference::with_digest
        // This ensures proper formatting: registry/namespace/image@sha256:digest
        let digest_reference = Reference::with_digest(
            reference.registry().to_string(),
            reference.repository().to_string(),
            digest,
        );

        // Use whole() to get the complete reference string
        let digest_ref = digest_reference.whole();

        Ok(digest_ref)
    }

    fn classify_error(
        &self,
        err: oci_distribution::errors::OciDistributionError,
        image: &str,
    ) -> OciError {
        let error_string = err.to_string();
        let error_lower = error_string.to_lowercase();

        debug!("Classifying OCI error: {}", error_string);

        // Classify based on error type with detailed logging
        if error_lower.contains("404") || error_lower.contains("not found") {
            warn!("Image not found: {}", image);
            OciError::ImageNotFound(image.to_string())
        } else if error_lower.contains("401")
            || error_lower.contains("403")
            || error_lower.contains("unauthorized")
        {
            warn!("Image requires authentication: {}", image);
            OciError::PrivateImage(image.to_string())
        } else if error_lower.contains("certificate")
            || error_lower.contains("ssl")
            || error_lower.contains("tls")
            || error_lower.contains("https")
        {
            warn!(
                "TLS/Certificate error accessing registry for image '{}': {}",
                image, error_string
            );
            OciError::Registry(format!(
                "TLS/Certificate error for {}: {}. If this is an insecure registry (HTTP), configure the OCI client to allow insecure connections.",
                image, error_string
            ))
        } else if error_lower.contains("connection") || error_lower.contains("timeout") {
            warn!(
                "Network connectivity issue for image '{}': {}",
                image, error_string
            );
            OciError::Network(format!("Connection failed for {}: {}", image, error_string))
        } else {
            warn!(
                "Unclassified registry error for '{}': {}",
                image, error_string
            );
            OciError::Registry(format!("Registry error for {}: {}", image, error_string))
        }
    }
}
