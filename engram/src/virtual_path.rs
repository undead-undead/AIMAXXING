use crate::error::{EngramError, Result};
use serde::{Deserialize, Serialize};

/// Virtual path: aimaxxing://collection/path/to/file.md
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualPath {
    pub collection: String,
    pub path: String,
}

use std::fmt;

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Self::build(&self.collection, &self.path))
    }
}

impl VirtualPath {
    /// Parse virtual path from string
    ///
    /// Supports formats:
    /// - `aimaxxing://collection/path.md`
    /// - `//collection/path.md` (missing prefix)
    /// - `aimaxxing:////collection/path.md` (extra slashes)
    ///
    /// # Examples
    ///
    /// ```
    /// # use aimaxxing_engram::virtual_path::VirtualPath;
    /// let vpath = VirtualPath::parse("aimaxxing://trading/strategies/sol.md").unwrap();
    /// assert_eq!(vpath.collection, "trading");
    /// assert_eq!(vpath.path, "strategies/sol.md");
    /// ```
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();

        // Normalize: aimaxxing:// with any number of slashes
        let normalized = if let Some(rest) = trimmed.strip_prefix("aimaxxing:") {
            format!("aimaxxing://{}", rest.trim_start_matches('/'))
        } else if let Some(rest) = trimmed.strip_prefix("//") {
            format!("aimaxxing://{}", rest)
        } else {
            trimmed.to_string()
        };

        // Security check: Prevent path traversal by checking components
        if normalized
            .split('/')
            .any(|part| part == ".." || part == ".")
        {
            return Err(EngramError::InvalidVirtualPath(format!(
                "Path traversal detected in virtual path: {}",
                input
            )));
        }

        // Parse: aimaxxing://collection/path
        if let Some(rest) = normalized.strip_prefix("aimaxxing://") {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();

            if parts.is_empty() || parts[0].is_empty() {
                return Err(EngramError::InvalidVirtualPath(
                    "Empty collection name".to_string(),
                ));
            }

            Ok(VirtualPath {
                collection: parts[0].to_string(),
                path: parts.get(1).unwrap_or(&"").to_string(),
            })
        } else {
            Err(EngramError::InvalidVirtualPath(format!(
                "Invalid virtual path format: {}",
                input
            )))
        }
    }

    /// Build virtual path from components
    ///
    /// # Examples
    ///
    /// ```
    /// # use aimaxxing_engram::virtual_path::VirtualPath;
    /// let vpath = VirtualPath::build("trading", "strategies/sol.md");
    /// assert_eq!(vpath, "aimaxxing://trading/strategies/sol.md");
    /// ```
    pub fn build(collection: &str, path: &str) -> String {
        if path.is_empty() {
            format!("aimaxxing://{}", collection)
        } else {
            format!("aimaxxing://{}/{}", collection, path)
        }
    }

    /// Check if a string is a virtual path
    ///
    /// # Examples
    ///
    /// ```
    /// # use aimaxxing_engram::virtual_path::VirtualPath;
    /// assert!(VirtualPath::is_virtual("aimaxxing://trading/sol.md"));
    /// assert!(VirtualPath::is_virtual("//trading/sol.md"));
    /// assert!(!VirtualPath::is_virtual("trading/sol.md"));
    /// assert!(!VirtualPath::is_virtual("/absolute/path.md"));
    /// ```
    pub fn is_virtual(path: &str) -> bool {
        let trimmed = path.trim();
        trimmed.starts_with("aimaxxing:") || trimmed.starts_with("//")
    }

    // to_string() is provided by Display trait

    /// Get display path (collection/path)
    pub fn display_path(&self) -> String {
        if self.path.is_empty() {
            self.collection.clone()
        } else {
            format!("{}/{}", self.collection, self.path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_standard() {
        let vpath = VirtualPath::parse("aimaxxing://trading/strategies/sol.md").unwrap();
        assert_eq!(vpath.collection, "trading");
        assert_eq!(vpath.path, "strategies/sol.md");
    }

    #[test]
    fn test_parse_missing_prefix() {
        let vpath = VirtualPath::parse("//trading/strategies/sol.md").unwrap();
        assert_eq!(vpath.collection, "trading");
        assert_eq!(vpath.path, "strategies/sol.md");
    }

    #[test]
    fn test_parse_extra_slashes() {
        let vpath = VirtualPath::parse("aimaxxing:////trading/strategies/sol.md").unwrap();
        assert_eq!(vpath.collection, "trading");
        assert_eq!(vpath.path, "strategies/sol.md");
    }

    #[test]
    fn test_parse_collection_only() {
        let vpath = VirtualPath::parse("aimaxxing://trading").unwrap();
        assert_eq!(vpath.collection, "trading");
        assert_eq!(vpath.path, "");
    }

    #[test]
    fn test_parse_invalid() {
        assert!(VirtualPath::parse("trading/sol.md").is_err());
        assert!(VirtualPath::parse("/absolute/path.md").is_err());
        assert!(VirtualPath::parse("aimaxxing://").is_err());
    }

    #[test]
    fn test_parse_traversal_attack() {
        // Test key security fix
        assert!(VirtualPath::parse("aimaxxing://../etc/passwd").is_err());
        assert!(VirtualPath::parse("aimaxxing://collection/../secret.txt").is_err());
        assert!(VirtualPath::parse("aimaxxing://collection/subdir/../secret.txt").is_err());
        // Normal paths should still work
        assert!(VirtualPath::parse("aimaxxing://collection/file.md").is_ok());
    }

    #[test]
    fn test_build() {
        assert_eq!(
            VirtualPath::build("trading", "strategies/sol.md"),
            "aimaxxing://trading/strategies/sol.md"
        );
        assert_eq!(VirtualPath::build("trading", ""), "aimaxxing://trading");
    }

    #[test]
    fn test_is_virtual() {
        assert!(VirtualPath::is_virtual("aimaxxing://trading/sol.md"));
        assert!(VirtualPath::is_virtual("//trading/sol.md"));
        assert!(!VirtualPath::is_virtual("trading/sol.md"));
        assert!(!VirtualPath::is_virtual("/absolute/path.md"));
    }

    #[test]
    fn test_display_path() {
        let vpath = VirtualPath {
            collection: "trading".to_string(),
            path: "strategies/sol.md".to_string(),
        };
        assert_eq!(vpath.display_path(), "trading/strategies/sol.md");

        let vpath_root = VirtualPath {
            collection: "trading".to_string(),
            path: "".to_string(),
        };
        assert_eq!(vpath_root.display_path(), "trading");
    }
}
