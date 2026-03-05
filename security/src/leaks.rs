use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum LeakAction {
    Block,
    Redact,
    Warn,
}

#[derive(Debug, Clone)]
pub struct LeakDetection {
    pub pattern_name: String,
    pub redacted_value: String,
    pub action: LeakAction,
}

struct SecretPattern {
    name: &'static str,
    regex: Regex,
    action: LeakAction,
}

pub struct LeakDetector {
    patterns: Vec<SecretPattern>,
}

impl LeakDetector {
    pub fn new() -> Self {
        let pattern_defs: Vec<(&str, &str, LeakAction)> = vec![
            // --- API keys (Redact) ---
            (
                "anthropic_api_key",
                r"sk-ant-api[a-zA-Z0-9\-]{20,}",
                LeakAction::Redact,
            ),
            ("openai_api_key", r"sk-[a-zA-Z0-9]{20,}", LeakAction::Redact),
            ("aws_access_key", r"AKIA[A-Z0-9]{16}", LeakAction::Redact),
            (
                "github_pat",
                r"github_pat_[a-zA-Z0-9_]{22,}",
                LeakAction::Redact,
            ),
            ("github_token", r"ghp_[a-zA-Z0-9]{36}", LeakAction::Redact),
            (
                "stripe_live_key",
                r"sk_live_[a-zA-Z0-9]{24,}",
                LeakAction::Redact,
            ),
            (
                "stripe_test_key",
                r"sk_test_[a-zA-Z0-9]{24,}",
                LeakAction::Redact,
            ),
            (
                "google_api_key",
                r"AIza[a-zA-Z0-9_\-]{35}",
                LeakAction::Redact,
            ),
            (
                "slack_bot_token",
                r"xoxb-[a-zA-Z0-9\-]+",
                LeakAction::Redact,
            ),
            (
                "slack_user_token",
                r"xoxp-[a-zA-Z0-9\-]+",
                LeakAction::Redact,
            ),
            (
                "bearer_token",
                r"Bearer [a-zA-Z0-9._\-]{20,}",
                LeakAction::Redact,
            ),
            (
                "minimax_api_key",
                r"mm-[a-zA-Z0-9]{20,}",
                LeakAction::Redact,
            ), // Added for Minimax
            // --- Block ---
            (
                "pem_private_key",
                r"-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
                LeakAction::Block,
            ),
            // --- Warn ---
            (
                "authorization_header",
                r"Authorization:\s*[a-zA-Z0-9._\-]{20,}",
                LeakAction::Warn,
            ),
            (
                "generic_jwt",
                r"eyJ[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]{10,}",
                LeakAction::Warn,
            ),
        ];

        let patterns = pattern_defs
            .into_iter()
            .map(|(name, pattern, action)| SecretPattern {
                name,
                regex: Regex::new(pattern).expect("Invalid regex pattern"),
                action,
            })
            .collect();

        Self { patterns }
    }

    pub fn redact(&self, input: &str) -> (String, Vec<LeakDetection>) {
        let mut result = input.to_string();
        let mut detections = Vec::new();

        for pattern in &self.patterns {
            let matches: Vec<(usize, usize, String)> = pattern
                .regex
                .find_iter(&result)
                .map(|m| (m.start(), m.end(), m.as_str().to_string()))
                .collect();

            for (start, end, matched) in matches.iter().rev() {
                detections.push(LeakDetection {
                    pattern_name: pattern.name.to_string(),
                    redacted_value: matched.clone(),
                    action: pattern.action.clone(),
                });

                if pattern.action == LeakAction::Redact {
                    let redacted = redact_string(&matched);
                    result.replace_range(start..end, &redacted);
                }
            }
        }

        (result, detections)
    }
}

fn redact_string(s: &str) -> String {
    if s.len() <= 8 {
        return "***".to_string();
    }
    let prefix = &s[..4];
    let suffix = &s[s.len() - 4..];
    format!("{prefix}***{suffix}")
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::new()
    }
}
