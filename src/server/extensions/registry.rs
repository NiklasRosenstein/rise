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

    /// Register an extension implementation
    pub fn register(&mut self, extension: Arc<dyn Extension>) {
        let name = extension.name().to_string();
        self.extensions.insert(name, extension);
    }

    /// Get extension by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Extension>> {
        self.extensions.get(name).cloned()
    }

    /// List all registered extension names
    #[allow(dead_code)]
    pub fn list(&self) -> Vec<String> {
        self.extensions.keys().cloned().collect()
    }

    /// Iterate over all registered extensions
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Arc<dyn Extension>)> {
        self.extensions.iter()
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
