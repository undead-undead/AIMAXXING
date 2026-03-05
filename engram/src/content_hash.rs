use sha2::{Digest, Sha256};

/// Compute SHA-256 hash of content (content-addressable storage)
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract short docid from full hash (first 6 characters)
/// Example: "abc123def456..." -> "abc123"
pub fn get_docid(hash: &str) -> String {
    hash.chars().take(6).collect()
}

/// Validate docid format (6 hex characters)
pub fn validate_docid(docid: &str) -> bool {
    docid.len() == 6 && docid.chars().all(|c| c.is_ascii_hexdigit())
}

/// Normalize docid (remove # prefix if present, lowercase)
pub fn normalize_docid(docid: &str) -> String {
    docid.trim_start_matches('#').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_content() {
        let content = "Hello, World!";
        let hash = hash_content(content);

        assert_eq!(hash.len(), 64); // SHA-256 = 64 hex chars
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[test]
    fn test_get_docid() {
        let hash = "abc123def456789";
        assert_eq!(get_docid(hash), "abc123");
    }

    #[test]
    fn test_validate_docid() {
        assert!(validate_docid("abc123"));
        assert!(validate_docid("ABCDEF"));
        assert!(!validate_docid("abc12")); // too short
        assert!(!validate_docid("abc1234")); // too long
        assert!(!validate_docid("ghijkl")); // invalid hex
    }

    #[test]
    fn test_normalize_docid() {
        assert_eq!(normalize_docid("#ABC123"), "abc123");
        assert_eq!(normalize_docid("abc123"), "abc123");
        assert_eq!(normalize_docid("#abc123"), "abc123");
    }
}
