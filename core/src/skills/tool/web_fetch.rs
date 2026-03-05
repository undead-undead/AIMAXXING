//! Web Fetch Tool
//!
//! Fetches content from URLs and converts HTML to clean Markdown.
//! Handles: HTML pages, JSON APIs, plain text.
//!
//! Feature-gated behind `http`.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::debug;

use crate::error::Error;
use crate::skills::tool::{Tool, ToolDefinition};

/// Maximum response body size (2 MB).
const MAX_BODY_SIZE: usize = 2 * 1024 * 1024;
/// Maximum output length returned to the Agent (to save tokens).
const MAX_OUTPUT_CHARS: usize = 12_000;
/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Web fetch tool configuration.
#[derive(Debug, Clone)]
pub struct WebFetchConfig {
    /// Request timeout
    pub timeout: Duration,
    /// Maximum response body size in bytes
    pub max_body_size: usize,
    /// Maximum output characters
    pub max_output_chars: usize,
    /// Blocked URL patterns (security)
    pub blocked_patterns: Vec<String>,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_body_size: MAX_BODY_SIZE,
            max_output_chars: MAX_OUTPUT_CHARS,
            blocked_patterns: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "[::1]".to_string(),
                "169.254.".to_string(),      // Link-local
                "10.".to_string(),            // Private
                "192.168.".to_string(),       // Private
                "172.16.".to_string(),        // Private
            ],
        }
    }
}

/// Web fetch tool — retrieves content from a URL and returns clean text.
pub struct WebFetchTool {
    config: WebFetchConfig,
    client: Client,
}

impl WebFetchTool {
    /// Create a new web fetch tool.
    pub fn new(config: WebFetchConfig) -> Result<Self, Error> {
        let client = Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("Mozilla/5.0 (compatible; AIMAXXING-Bot/1.0)")
            .build()
            .map_err(|e| Error::Internal(format!("HTTP client error: {}", e)))?;

        Ok(Self { config, client })
    }

    /// Create with defaults.
    pub fn with_defaults() -> Result<Self, Error> {
        Self::new(WebFetchConfig::default())
    }

    /// Security check: validate URL isn't targeting internal resources.
    fn validate_url(&self, url: &str) -> anyhow::Result<()> {
        let lower = url.to_lowercase();

        // Must be http or https
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            anyhow::bail!("Only http:// and https:// URLs are allowed");
        }

        // Check blocked patterns (SSRF protection)
        for pattern in &self.config.blocked_patterns {
            if lower.contains(pattern) {
                anyhow::bail!(
                    "URL blocked by security policy: contains '{}'",
                    pattern
                );
            }
        }

        Ok(())
    }

    /// Fetch URL and return processed content.
    async fn fetch_url(&self, url: &str) -> anyhow::Result<String> {
        self.validate_url(url)?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!("Request timed out after {:?}", self.config.timeout)
                } else if e.is_connect() {
                    anyhow::anyhow!("Failed to connect to {}", url)
                } else {
                    anyhow::anyhow!("Request failed: {}", e)
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("HTTP {}: {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Check content length
        if let Some(len) = response.content_length() {
            if len > self.config.max_body_size as u64 {
                anyhow::bail!(
                    "Response too large ({} bytes, max {})",
                    len,
                    self.config.max_body_size
                );
            }
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read body: {}", e))?;

        if body.len() > self.config.max_body_size {
            anyhow::bail!(
                "Response body too large ({} bytes, max {})",
                body.len(),
                self.config.max_body_size
            );
        }

        let text = String::from_utf8_lossy(&body).to_string();

        // Process based on content type
        let processed = if content_type.contains("json") {
            // Pretty-print JSON
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => serde_json::to_string_pretty(&v).unwrap_or(text),
                Err(_) => text,
            }
        } else if content_type.contains("html") {
            // Convert HTML to readable text
            html_to_text(&text)
        } else {
            // Plain text / other
            text
        };

        // Truncate if needed
        if processed.len() > self.config.max_output_chars {
            let truncated = &processed[..self.config.max_output_chars];
            Ok(format!(
                "{}\n\n[... truncated, {} total chars]",
                truncated,
                processed.len()
            ))
        } else {
            Ok(processed)
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> String {
        "web_fetch".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_fetch".to_string(),
            description: "Fetch content from a URL. Returns the page content as clean text. Supports HTML (converted to readable text), JSON (pretty-printed), and plain text.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch (must be http:// or https://)"
                    }
                },
                "required": ["url"]
            }),
            parameters_ts: Some("interface WebFetch {\n  url: string; // The URL to fetch (http/https only)\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use to read the content of a specific web page or API endpoint. Prefer `web_search` to find URLs first, then `web_fetch` to read the content.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            url: String,
        }
        let args: Args = serde_json::from_str(arguments)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))?;

        let url = args.url.trim();
        if url.is_empty() {
            anyhow::bail!("URL cannot be empty");
        }

        debug!(url = url, "Fetching URL");
        self.fetch_url(url).await
    }
}

// ─── HTML to Text Converter ────────────────────────────────────────────

/// Convert HTML to readable plain text.
/// This is a lightweight HTML-to-text converter that handles common cases.
fn html_to_text(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 3);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut last_was_space = false;

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if ch == '<' {
            in_tag = true;
            tag_name.clear();
            i += 1;
            continue;
        }

        if in_tag {
            if ch == '>' {
                in_tag = false;
                let tag_lower = tag_name.to_lowercase();

                // Track script/style blocks
                if tag_lower.starts_with("script") {
                    in_script = true;
                } else if tag_lower.starts_with("/script") {
                    in_script = false;
                } else if tag_lower.starts_with("style") {
                    in_style = true;
                } else if tag_lower.starts_with("/style") {
                    in_style = false;
                }

                // Block elements → newline
                let block_tags = [
                    "p", "/p", "div", "/div", "br", "br/", "br /",
                    "h1", "/h1", "h2", "/h2", "h3", "/h3",
                    "h4", "/h4", "h5", "/h5", "h6", "/h6",
                    "li", "/li", "tr", "/tr", "blockquote", "/blockquote",
                    "hr", "hr/",
                ];
                let clean_tag = tag_lower.split_whitespace().next().unwrap_or("");
                if block_tags.contains(&clean_tag) {
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    last_was_space = true;

                    // Add markdown-like prefix for headings
                    if clean_tag.starts_with('h') && clean_tag.len() == 2 {
                        if let Some(level) = clean_tag.chars().nth(1).and_then(|c| c.to_digit(10)) {
                            for _ in 0..level {
                                result.push('#');
                            }
                            result.push(' ');
                        }
                    }

                    // List items
                    if clean_tag == "li" {
                        result.push_str("• ");
                    }

                    // Horizontal rule
                    if clean_tag == "hr" || clean_tag == "hr/" {
                        result.push_str("---\n");
                    }
                }
            } else {
                tag_name.push(ch);
            }
            i += 1;
            continue;
        }

        // Skip script/style content
        if in_script || in_style {
            i += 1;
            continue;
        }

        // Handle HTML entities
        if ch == '&' {
            let rest: String = chars[i..].iter().take(10).collect();
            if rest.starts_with("&amp;") {
                result.push('&');
                i += 5;
                last_was_space = false;
                continue;
            } else if rest.starts_with("&lt;") {
                result.push('<');
                i += 4;
                last_was_space = false;
                continue;
            } else if rest.starts_with("&gt;") {
                result.push('>');
                i += 4;
                last_was_space = false;
                continue;
            } else if rest.starts_with("&quot;") {
                result.push('"');
                i += 6;
                last_was_space = false;
                continue;
            } else if rest.starts_with("&#39;") || rest.starts_with("&apos;") {
                result.push('\'');
                i += if rest.starts_with("&#39;") { 5 } else { 6 };
                last_was_space = false;
                continue;
            } else if rest.starts_with("&nbsp;") {
                result.push(' ');
                i += 6;
                last_was_space = true;
                continue;
            }
        }

        // Collapse whitespace
        if ch.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(ch);
            last_was_space = false;
        }

        i += 1;
    }

    // Clean up excessive blank lines
    let mut cleaned = String::with_capacity(result.len());
    let mut consecutive_newlines = 0;
    for ch in result.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                cleaned.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            cleaned.push(ch);
        }
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_to_text_basic() {
        assert_eq!(
            html_to_text("<p>Hello <b>world</b></p>"),
            "Hello world"
        );
    }

    #[test]
    fn test_html_to_text_headings() {
        let html = "<h1>Title</h1><p>Content</p>";
        let text = html_to_text(html);
        assert!(text.contains("# Title"));
        assert!(text.contains("Content"));
    }

    #[test]
    fn test_html_to_text_scripts_removed() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        let text = html_to_text(html);
        assert!(!text.contains("alert"));
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
    }

    #[test]
    fn test_html_to_text_entities() {
        assert_eq!(html_to_text("a &amp; b &lt; c"), "a & b < c");
    }

    #[test]
    fn test_html_to_text_list() {
        let html = "<ul><li>One</li><li>Two</li></ul>";
        let text = html_to_text(html);
        assert!(text.contains("• One"));
        assert!(text.contains("• Two"));
    }

    #[test]
    fn test_validate_url_blocks_internal() {
        let tool = WebFetchTool::with_defaults().unwrap();
        assert!(tool.validate_url("http://localhost:8080").is_err());
        assert!(tool.validate_url("http://127.0.0.1").is_err());
        assert!(tool.validate_url("http://192.168.1.1").is_err());
        assert!(tool.validate_url("ftp://example.com").is_err());
        assert!(tool.validate_url("https://example.com").is_ok());
    }
}
