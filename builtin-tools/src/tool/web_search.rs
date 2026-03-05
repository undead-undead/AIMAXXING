//! Web Search Tool
//!
//! Multi-provider web search with automatic fallback:
//! Brave Search → DuckDuckGo HTML (no API key needed).
//!
//! Feature-gated behind `http`.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use brain::error::Error;
use brain::skills::tool::{Tool, ToolDefinition};

/// Maximum cache entries to prevent unbounded growth.
const MAX_CACHE_ENTRIES: usize = 256;
/// Default cache TTL.
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);
/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// Cached search result with expiry.
struct CacheEntry {
    data: String,
    expires_at: Instant,
}

/// Search provider configuration.
#[derive(Debug, Clone)]
pub enum SearchProvider {
    /// Brave Search API (requires API key)
    Brave { api_key: String },
    /// DuckDuckGo HTML scraping (no key needed, fallback)
    DuckDuckGo,
}

/// Web search tool configuration.
#[derive(Debug, Clone)]
pub struct WebSearchConfig {
    /// Primary search provider
    pub provider: SearchProvider,
    /// Whether to fallback to DuckDuckGo on failure
    pub fallback_enabled: bool,
    /// Max results to return
    pub max_results: u8,
    /// Request timeout
    pub timeout: Duration,
    /// Cache TTL
    pub cache_ttl: Duration,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        // Auto-detect provider from environment
        let provider = if let Ok(key) = std::env::var("BRAVE_API_KEY") {
            SearchProvider::Brave { api_key: key }
        } else {
            SearchProvider::DuckDuckGo
        };

        Self {
            provider,
            fallback_enabled: true,
            max_results: 5,
            timeout: DEFAULT_TIMEOUT,
            cache_ttl: DEFAULT_CACHE_TTL,
        }
    }
}

/// Web search tool — lets the Agent search the internet.
pub struct WebSearchTool {
    config: WebSearchConfig,
    client: Client,
    cache: Mutex<HashMap<String, CacheEntry>>,
    /// DuckDuckGo block state (CAPTCHA protection)
    ddg_blocked_until: Mutex<Option<Instant>>,
}

impl WebSearchTool {
    /// Create a new web search tool.
    pub fn new(config: WebSearchConfig) -> Result<Self, Error> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent("Mozilla/5.0 (compatible; AIMAXXING-Bot/1.0)")
            .build()
            .map_err(|e| Error::Internal(format!("HTTP client error: {}", e)))?;

        Ok(Self {
            config,
            client,
            cache: Mutex::new(HashMap::new()),
            ddg_blocked_until: Mutex::new(None),
        })
    }

    /// Create with default config (auto-detects provider from env).
    pub fn from_env() -> Result<Self, Error> {
        Self::new(WebSearchConfig::default())
    }

    /// Check and return cached result.
    fn cache_get(&self, key: &str) -> Option<String> {
        let cache = self.cache.lock().ok()?;
        if let Some(entry) = cache.get(key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.data.clone());
            }
        }
        None
    }

    /// Store a result in cache (with eviction).
    fn cache_set(&self, key: String, data: String) {
        if let Ok(mut cache) = self.cache.lock() {
            // Evict expired entries when cache is full
            if cache.len() >= MAX_CACHE_ENTRIES {
                let now = Instant::now();
                cache.retain(|_, v| v.expires_at > now);
                // If still full, remove oldest
                if cache.len() >= MAX_CACHE_ENTRIES {
                    if let Some(oldest_key) = cache
                        .iter()
                        .min_by_key(|(_, v)| v.expires_at)
                        .map(|(k, _)| k.clone())
                    {
                        cache.remove(&oldest_key);
                    }
                }
            }
            cache.insert(
                key,
                CacheEntry {
                    data,
                    expires_at: Instant::now() + self.config.cache_ttl,
                },
            );
        }
    }

    /// Search using Brave Search API.
    async fn search_brave(&self, query: &str, api_key: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .query(&[
                ("q", query),
                ("count", &self.config.max_results.to_string()),
            ])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Brave request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Brave API error {}: {}", status, &body[..body.len().min(200)]);
        }

        let body: serde_json::Value = resp.json().await?;

        // Extract results
        let results = body["web"]["results"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(self.config.max_results as usize)
                    .map(|r| SearchResult {
                        title: r["title"].as_str().unwrap_or("").to_string(),
                        url: r["url"].as_str().unwrap_or("").to_string(),
                        snippet: r["description"].as_str().unwrap_or("").to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(serde_json::to_string_pretty(&results)?)
    }

    /// Search using DuckDuckGo HTML (no API key needed).
    async fn search_duckduckgo(&self, query: &str) -> anyhow::Result<String> {
        // Check if DDG is blocked
        if let Ok(guard) = self.ddg_blocked_until.lock() {
            if let Some(until) = *guard {
                if Instant::now() < until {
                    anyhow::bail!("DuckDuckGo temporarily blocked (CAPTCHA)");
                }
            }
        }

        let resp = self
            .client
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("DuckDuckGo request failed: {}", e))?;

        let body = resp.text().await?;

        // Check for CAPTCHA
        if body.contains("bot") && body.contains("automated") {
            if let Ok(mut guard) = self.ddg_blocked_until.lock() {
                *guard = Some(Instant::now() + Duration::from_secs(300));
            }
            anyhow::bail!("DuckDuckGo returned CAPTCHA, blocking for 5 minutes");
        }

        // Parse HTML results (simple extraction via string processing)
        let mut results = Vec::new();
        for chunk in body.split("class=\"result__a\"") {
            if results.len() >= self.config.max_results as usize {
                break;
            }
            if chunk.contains("href=\"") {
                let url = extract_between(chunk, "href=\"", "\"").unwrap_or_default();
                // DuckDuckGo redirects — extract actual URL
                let actual_url = if url.contains("uddg=") {
                    extract_between(&url, "uddg=", "&")
                        .map(|u| urlencoding_decode(&u))
                        .unwrap_or(url.clone())
                } else {
                    url
                };

                let title = extract_between(chunk, ">", "</a>").unwrap_or_default();
                let snippet = extract_between(chunk, "class=\"result__snippet\">", "</a>")
                    .unwrap_or_default();

                if !actual_url.is_empty() && !title.is_empty() {
                    results.push(SearchResult {
                        title: strip_html_tags(&title),
                        url: actual_url,
                        snippet: strip_html_tags(&snippet),
                    });
                }
            }
        }

        Ok(serde_json::to_string_pretty(&results)?)
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> String {
        "web_search".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web for information. Returns a list of results with titles, URLs, and snippets.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    }
                },
                "required": ["query"]
            }),
            parameters_ts: Some("interface WebSearch {\n  query: string; // The search query\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use to find current information, URLs, documentation, or facts from the internet.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            query: String,
        }
        let args: Args = serde_json::from_str(arguments)
            .map_err(|e| anyhow::anyhow!("Invalid arguments: {}", e))?;

        let query = args.query.trim();
        if query.is_empty() {
            anyhow::bail!("Search query cannot be empty");
        }

        // Check cache
        if let Some(cached) = self.cache_get(query) {
            debug!(query = query, "Cache hit for web search");
            return Ok(cached);
        }

        // Try primary provider
        let result = match &self.config.provider {
            SearchProvider::Brave { api_key } => {
                match self.search_brave(query, api_key).await {
                    Ok(r) => r,
                    Err(e) if self.config.fallback_enabled => {
                        warn!(error = %e, "Brave search failed, falling back to DuckDuckGo");
                        self.search_duckduckgo(query).await?
                    }
                    Err(e) => return Err(e),
                }
            }
            SearchProvider::DuckDuckGo => self.search_duckduckgo(query).await?,
        };

        // Cache and return
        self.cache_set(query.to_string(), result.clone());
        Ok(result)
    }
}

/// A single search result.
#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

// ─── HTML Parsing Helpers ──────────────────────────────────────────────

fn extract_between(s: &str, start: &str, end: &str) -> Option<String> {
    let start_idx = s.find(start)? + start.len();
    let end_idx = s[start_idx..].find(end)? + start_idx;
    Some(s[start_idx..end_idx].to_string())
}

fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn urlencoding_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                result.push(byte as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<b>Hello</b> <i>world</i>"), "Hello world");
        assert_eq!(strip_html_tags("no tags"), "no tags");
        assert_eq!(strip_html_tags("&amp; &lt;"), "& <");
    }

    #[test]
    fn test_extract_between() {
        let s = r#"href="https://example.com" class="foo""#;
        assert_eq!(
            extract_between(s, "href=\"", "\""),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(urlencoding_decode("hello%20world"), "hello world");
        assert_eq!(urlencoding_decode("a%26b"), "a&b");
    }
}
