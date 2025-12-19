use super::Extension;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for extension implementations
pub struct ExtensionRegistry {
    extensions: HashMap<String, Arc<dyn Extension>>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    /// Register an extension type implementation
    ///
    /// This method now registers extensions by their type identifier rather than instance name,
    /// allowing multiple instances of the same extension type to be created.
    #[allow(dead_code)]
    pub fn register_type(&mut self, extension: Arc<dyn Extension>) {
        let extension_type = extension.extension_type().to_string();
        self.extensions.insert(extension_type, extension);
    }

    /// Get extension handler by type
    ///
    /// This returns the extension handler for a given extension type (e.g., "aws-rds-provisioner").
    /// The returned handler can be used to manage multiple instances of this extension type.
    pub fn get(&self, extension_type: &str) -> Option<Arc<dyn Extension>> {
        self.extensions.get(extension_type).cloned()
    }

    /// List all registered extension types
    #[allow(dead_code)]
    pub fn list(&self) -> Vec<String> {
        self.extensions.keys().cloned().collect()
    }

    /// Iterate over all registered extension types
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Arc<dyn Extension>)> {
        self.extensions.iter()
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
