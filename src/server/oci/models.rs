/// Parsed OCI image reference
#[allow(dead_code)]
pub struct ImageReference {
    pub registry: String,
    pub namespace: String,
    pub image: String,
    pub tag: String,
}
