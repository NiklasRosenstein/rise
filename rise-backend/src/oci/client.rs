use super::error::OciError;
use anyhow::Result;
use oci_distribution::{secrets::RegistryAuth, Client, Reference};

pub struct OciClient {
    client: Client,
}

impl OciClient {
    pub fn new() -> Result<Self> {
        let client = Client::new(Default::default());
        Ok(Self { client })
    }

    /// Resolve image reference to digest-pinned reference
    /// Uses OCI Distribution API to fetch manifest only (~2-10KB)
    pub async fn resolve_image_digest(&self, image_ref: &str) -> Result<String, OciError> {
        // Parse image reference
        let reference = Reference::try_from(image_ref)
            .map_err(|e| OciError::InvalidReference(format!("{}: {}", image_ref, e)))?;

        // Fetch manifest (anonymous access for public images)
        let auth = RegistryAuth::Anonymous;
        let (_manifest, digest) = self
            .client
            .pull_manifest(&reference, &auth)
            .await
            .map_err(|e| self.classify_error(e, image_ref))?;

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

        // Classify based on error type
        if error_string.contains("404") || error_string.contains("not found") {
            OciError::ImageNotFound(image.to_string())
        } else if error_string.contains("401")
            || error_string.contains("403")
            || error_string.contains("unauthorized")
        {
            OciError::PrivateImage(image.to_string())
        } else {
            OciError::Network(error_string)
        }
    }
}
