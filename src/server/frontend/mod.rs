pub mod routes;

use std::path::{Component, Path, PathBuf};

/// Load a static file from the configured static_dir, with path traversal protection.
pub async fn load_static_file(static_dir: &str, rel_path: &str) -> Option<Vec<u8>> {
    let mut safe_path = PathBuf::new();
    for part in Path::new(rel_path).components() {
        match part {
            Component::Normal(seg) => safe_path.push(seg),
            _ => return None,
        }
    }
    let full_path = PathBuf::from(static_dir).join(safe_path);
    tokio::fs::read(&full_path).await.ok()
}
