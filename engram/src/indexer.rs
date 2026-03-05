//! Document indexer for active knowledge ingestion
//!
//! Watches filesystem and indexes documents into the Engram store.

use crate::error::Result;
use crate::store::EngramStore;
use std::path::Path;
use tracing::info;

/// Index a file into the store
pub fn index_file(store: &EngramStore, collection: &str, file_path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(file_path)?;
    let title = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");
    let path = file_path.to_str().unwrap_or("unknown");

    store.store_document(collection, path, title, &content, false)?;
    info!("Indexed file: {}", path);
    Ok(())
}

/// Index all matching files in a directory
pub fn index_directory(
    store: &EngramStore,
    collection: &str,
    dir: &Path,
    pattern: &str,
) -> Result<usize> {
    let glob_pattern = format!("{}/{}", dir.display(), pattern);
    let mut count = 0;

    for entry in
        glob::glob(&glob_pattern).map_err(|e| crate::error::EngramError::Custom(e.to_string()))?
    {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    if let Err(e) = index_file(store, collection, &path) {
                        tracing::warn!("Failed to index {}: {}", path.display(), e);
                    } else {
                        count += 1;
                    }
                }
            }
            Err(e) => tracing::warn!("Glob error: {}", e),
        }
    }

    info!("Indexed {} files from {}", count, dir.display());
    Ok(count)
}
