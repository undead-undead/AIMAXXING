use aho_corasick::AhoCorasick;
use once_cell::sync::Lazy;

/// Result of scanning and optionally sanitizing an input string.
#[derive(Debug, Clone)]
pub struct SanitizedOutput {
    /// The (possibly modified) content after sanitization.
    pub content: String,
    /// Human-readable warnings describing each detected pattern.
    pub warnings: Vec<String>,
    /// Whether the content was modified during sanitization.
    pub was_modified: bool,
}

// ---------------------------------------------------------------------------
// Pattern definitions
// ---------------------------------------------------------------------------

const PHRASE_PATTERNS: &[&str] = &[
    // Instruction override attempts
    "ignore previous",
    "ignore all previous",
    "disregard",
    "forget everything",
    "new instructions",
    "updated instructions",
    // Role impersonation
    "you are now",
    "act as",
    "pretend to be",
    // Role markers (colon-delimited)
    "system:",
    "assistant:",
    "user:",
    // Special tokens (LLM-specific delimiters)
    "<|",
    "|>",
    "[INST]",
    "[/INST]",
    // Fenced code block injection
    "```system",
];

static AC: Lazy<AhoCorasick> = Lazy::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(PHRASE_PATTERNS)
        .expect("Failed to build Aho-Corasick automaton")
});

pub struct InjectionDetector;

impl InjectionDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn check_injection(&self, input: &str) -> SanitizedOutput {
        let mut warnings = Vec::new();
        let mut was_modified = false;

        // Find all matches
        let matches: Vec<_> = AC.find_iter(input).collect();

        if matches.is_empty() {
            return SanitizedOutput {
                content: input.to_string(),
                warnings,
                was_modified,
            };
        }

        was_modified = true;
        let mut result = String::with_capacity(input.len() + matches.len() * 10);
        let mut last_end = 0;

        for mat in matches {
            // Append safe text before match
            result.push_str(&input[last_end..mat.start()]);

            // Append sanitization marker
            let matched_text = &input[mat.start()..mat.end()];
            result.push_str(&format!("[DETECTED: {}]", matched_text));

            warnings.push(format!(
                "Injection pattern matched: '{}'",
                PHRASE_PATTERNS[mat.pattern()]
            ));

            last_end = mat.end();
        }

        // Append remaining text
        result.push_str(&input[last_end..]);

        SanitizedOutput {
            content: result,
            warnings,
            was_modified,
        }
    }

    pub fn has_injection(&self, input: &str) -> bool {
        AC.find(input).is_some()
    }
}

impl Default for InjectionDetector {
    fn default() -> Self {
        Self::new()
    }
}
