//! Phase 16: Output Auditor — streaming output sanitization and anomaly circuit-breaker.
//!
//! Extends the existing SecurityManager with:
//! - Enhanced secret pattern detection (API keys, JWTs, SSH keys)
//! - Real-time masking in streaming output
//! - Anomaly circuit-breaker: kills stream on forbidden execution patterns

/// Known high-entropy secret patterns for detection
pub const SECRET_PATTERNS: &[(&str, &str)] = &[
    ("sk-", "OpenAI API Key"),
    ("ghp_", "GitHub Personal Access Token"),
    ("gho_", "GitHub OAuth Token"),
    ("ghu_", "GitHub User Token"),
    ("ghs_", "GitHub Server Token"),
    ("glpat-", "GitLab Personal Access Token"),
    ("xoxb-", "Slack Bot Token"),
    ("xoxp-", "Slack User Token"),
    ("AKIA", "AWS Access Key ID"),
    ("eyJ", "JWT/Base64 Token"),
    ("ssh-rsa ", "SSH RSA Private Key"),
    ("ssh-ed25519 ", "SSH Ed25519 Key"),
    ("-----BEGIN RSA PRIVATE KEY-----", "RSA Private Key"),
    ("-----BEGIN OPENSSH PRIVATE KEY-----", "OpenSSH Private Key"),
    ("-----BEGIN PGP PRIVATE KEY BLOCK-----", "PGP Private Key"),
];

/// Forbidden execution patterns that should trigger circuit-breaker
pub const FORBIDDEN_PATTERNS: &[&str] = &[
    "eval(",
    "exec(",
    "__import__(",
    "subprocess.call(",
    "os.system(",
    "Runtime.getRuntime().exec(",
    "Process.Start(",
    "system(\"",
];

/// Result of an output audit
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutputAuditResult {
    /// Whether the output contains any findings
    pub has_findings: bool,
    /// Whether the stream should be killed
    pub kill_stream: bool,
    /// Detected issues
    pub findings: Vec<OutputFinding>,
    /// The sanitized output (with secrets masked)
    pub sanitized: String,
}

/// A finding in the output
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutputFinding {
    pub pattern_type: String,
    pub description: String,
    pub action: AuditAction,
}

/// Action to take on a finding
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum AuditAction {
    /// Mask the value and continue
    Mask,
    /// Kill the stream immediately
    CircuitBreak,
    /// Log and continue
    Warn,
}

/// Audits streaming output for secrets and forbidden patterns
pub struct OutputAuditor;

impl OutputAuditor {
    /// Audit a chunk of output text
    pub fn audit(text: &str) -> OutputAuditResult {
        let mut findings = Vec::new();
        let mut sanitized = text.to_string();
        let mut kill_stream = false;

        // Check for secret patterns
        for (prefix, secret_type) in SECRET_PATTERNS {
            if text.contains(prefix) {
                findings.push(OutputFinding {
                    pattern_type: "secret_leak".into(),
                    description: format!("Potential {} detected", secret_type),
                    action: AuditAction::Mask,
                });
                // Mask: replace the token with asterisks
                sanitized = mask_secrets(&sanitized, prefix);
            }
        }

        // Check for forbidden execution patterns
        for pattern in FORBIDDEN_PATTERNS {
            if text.contains(pattern) {
                findings.push(OutputFinding {
                    pattern_type: "forbidden_execution".into(),
                    description: format!("Forbidden execution pattern: '{}'", pattern),
                    action: AuditAction::CircuitBreak,
                });
                kill_stream = true;
            }
        }

        OutputAuditResult {
            has_findings: !findings.is_empty(),
            kill_stream,
            findings,
            sanitized,
        }
    }
}

/// Mask secret tokens in text by replacing content after the prefix
fn mask_secrets(text: &str, prefix: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;

    while let Some(pos) = remaining.find(prefix) {
        result.push_str(&remaining[..pos]);
        result.push_str(prefix);

        // Find the end of the token (next whitespace or end of string)
        let after_prefix = &remaining[pos + prefix.len()..];
        let token_end = after_prefix
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}')
            .unwrap_or(after_prefix.len());

        // Replace with asterisks
        let masked_len = token_end.min(20);
        result.push_str(&"*".repeat(masked_len));

        remaining = &remaining[pos + prefix.len() + token_end..];
    }

    result.push_str(remaining);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_output() {
        let result = OutputAuditor::audit("Hello, how can I help you?");
        assert!(!result.has_findings);
        assert!(!result.kill_stream);
    }

    #[test]
    fn test_secret_detection() {
        let result = OutputAuditor::audit("Your key is sk-abc123def456");
        assert!(result.has_findings);
        assert!(!result.kill_stream); // Mask, don't kill
        assert!(result.sanitized.contains("sk-"));
        assert!(result.sanitized.contains("****"));
        assert!(!result.sanitized.contains("abc123"));
    }

    #[test]
    fn test_forbidden_execution_kills_stream() {
        let result = OutputAuditor::audit("Let me run eval(user_input) for you");
        assert!(result.has_findings);
        assert!(result.kill_stream); // Should circuit-break
    }

    #[test]
    fn test_jwt_masking() {
        let result =
            OutputAuditor::audit("Token: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0");
        assert!(result.has_findings);
        assert!(result.sanitized.contains("eyJ"));
        assert!(result.sanitized.contains("****"));
    }

    #[test]
    fn test_github_token_masking() {
        let result = OutputAuditor::audit("ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
        assert!(result.has_findings);
        assert!(result.sanitized.contains("ghp_"));
    }
}
