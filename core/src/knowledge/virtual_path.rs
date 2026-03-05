use serde::{Deserialize, Serialize};
use std::fmt;

/// A virtual path in the AIMAXXING knowledge system
/// Format: aimaxxing://<collection>/<path>
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VirtualPath {
    pub collection: String,
    pub path: String,
}

impl VirtualPath {
    pub fn new(collection: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            path: path.into(),
        }
    }

    pub fn parse(uri: &str) -> Option<Self> {
        let uri = uri.trim();
        if !uri.starts_with("aimaxxing://") {
            return None;
        }

        let without_scheme = &uri["aimaxxing://".len()..];
        let mut parts = without_scheme.splitn(2, '/');

        let collection = parts.next()?;
        let path = parts.next().unwrap_or("");

        Some(Self {
            collection: collection.to_string(),
            path: path.to_string(),
        })
    }

    pub fn to_string(&self) -> String {
        format!("aimaxxing://{}/{}", self.collection, self.path)
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
