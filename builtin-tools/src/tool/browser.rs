use async_trait::async_trait;
use brain::error::{Error, Result};
use brain::skills::tool::{Tool, ToolDefinition};
use brain::config::vault::SecretVault;
use headless_chrome::{Browser, LaunchOptions, Tab};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::collections::HashMap;

/// A stateful browser tool that maintains sessions and provides semantic snapshots with refs.
pub struct BrowserTool {
    browser: Arc<Mutex<Option<Browser>>>,
    user_data_dir: Option<PathBuf>,
    /// Cache for ref mapping: "@e1" -> "CSS selector / Internal ID"
    ref_map: Arc<Mutex<HashMap<String, String>>>,
    /// Optional vault for persisting cookies/sessions locally
    vault: Option<Arc<dyn SecretVault>>,
    /// Store the last captured snapshot tree for diffing
    last_snapshot: Arc<Mutex<Option<String>>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct BrowserSnapshot {
    tree: String,
    refs: HashMap<String, String>,
}

impl BrowserTool {
    pub fn new(user_data_dir: Option<PathBuf>, vault: Option<Arc<dyn SecretVault>>) -> Self {
        Self {
            browser: Arc::new(Mutex::new(None)),
            user_data_dir,
            ref_map: Arc::new(Mutex::new(HashMap::new())),
            vault,
            last_snapshot: Arc::new(Mutex::new(None)),
        }
    }

    fn get_browser(&self) -> Result<Browser> {
        let mut guard = self.browser.lock();
        if let Some(browser) = guard.as_ref() {
            return Ok(browser.clone());
        }

        let mut options = LaunchOptions::default();
        options.headless = true;
        
        options.args = vec![
            std::ffi::OsStr::new("--disable-blink-features=AutomationControlled"),
            std::ffi::OsStr::new("--no-sandbox"),
            std::ffi::OsStr::new("--disable-infobars"),
            std::ffi::OsStr::new("--window-size=1920,1080"),
            std::ffi::OsStr::new("--user-agent=Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
        ];

        if let Some(dir) = &self.user_data_dir {
            options.user_data_dir = Some(dir.clone());
        }

        let browser = Browser::new(options)
            .map_err(|e| Error::Internal(format!("Failed to launch browser: {}", e)))?;
        
        *guard = Some(browser.clone());
        Ok(browser)
    }

    async fn extract_aria_tree_with_refs(&self, tab: Arc<Tab>, interactive_only: bool, compact: bool) -> Result<BrowserSnapshot> {
        let script = format!(r#"
            (function() {{
                let refCounter = 0;
                let refs = {{}};
                let nameCounter = {{}};

                function getSemanticInfo(node, depth = 0) {{
                    if (depth > 15) return "";
                    if (!node || node.nodeType !== 1) return "";

                    const style = getComputedStyle(node);
                    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return "";

                    let role = node.getAttribute ? node.getAttribute('role') : null;
                    let label = node.ariaLabel || node.innerText || node.value || "";
                    
                    const interactiveTags = ['BUTTON', 'A', 'INPUT', 'SELECT', 'TEXTAREA', 'DETAILS', 'SUMMARY'];
                    const isAlwaysInteractive = interactiveTags.includes(node.tagName);
                    const hasCursorPointer = style.cursor === 'pointer';
                    const hasOnClick = node.hasAttribute('onclick') || node.onclick !== null;
                    const hasTabIndex = node.hasAttribute('tabindex') && node.getAttribute('tabindex') !== '-1';

                    const isInteractive = role || isAlwaysInteractive || hasCursorPointer || hasOnClick || hasTabIndex;
                    const isHeading = node.tagName.startsWith('H') && node.tagName.length <= 2;

                    let info = "";
                    if (isInteractive || isHeading || !{}) {{
                        let indent = "  ".repeat(depth);
                        let name = node.tagName.toLowerCase();
                        let cleanLabel = label.trim().substring(0, 100).replace(/\n/g, ' ');
                        
                        if (isInteractive || (isHeading && !{})) {{
                            let refId = `e${{++refCounter}}`;
                            node.setAttribute('data-aimaxxing-ref', refId);
                            let selector = `[data-aimaxxing-ref="${{refId}}"]`;
                            
                            let key = `${{role || name}}:${{cleanLabel}}`;
                            nameCounter[key] = (nameCounter[key] || 0) + 1;
                            let nth = nameCounter[key] > 1 ? ` [nth=${{nameCounter[key]-1}}]` : "";
                            
                            refs[refId] = selector;
                            info += `${{indent}}[${{role || name}}] "${{cleanLabel}}" [ref=@${{refId}}]${{nth}}\n`;
                            
                            for (let child of node.children) {{
                                info += getSemanticInfo(child, depth + 1);
                            }}
                        }} else if (!{}) {{
                            if (cleanLabel || !{}) {{
                                info += `${{indent}}<${{name}}>\n`;
                                for (let child of node.children) {{
                                    info += getSemanticInfo(child, depth + 1);
                                }}
                            }}
                        }}
                    }} else {{
                        for (let child of node.children) {{
                            info += getSemanticInfo(child, depth);
                        }}
                    }}
                    return info;
                }}
                
                const tree = getSemanticInfo(document.body);
                return {{ tree, refs }};
            }})()
        "#, interactive_only, compact, interactive_only, compact);

        let remote_object = tab.evaluate(&script, false)
            .map_err(|e| Error::Internal(format!("Failed to evaluate ARIA script: {}", e)))?;
        
        let value: Value = remote_object.value.ok_or_else(|| Error::Internal("No value returned from ARIA script".to_string()))?;
        let snapshot: BrowserSnapshot = serde_json::from_value(value)
            .map_err(|e| Error::Internal(format!("Failed to parse snapshot JSON: {}", e)))?;
        
        let mut map_guard = self.ref_map.lock();
        map_guard.clear();
        for (k, v) in &snapshot.refs {
            map_guard.insert(format!("@{}", k), v.clone());
        }

        Ok(snapshot)
    }

    fn resolve_selector(&self, selector: &str) -> String {
        let guard = self.ref_map.lock();
        if let Some(resolved) = guard.get(selector) {
            resolved.clone()
        } else {
            selector.to_string()
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> String {
        "browser_browse".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browser_browse".to_string(),
            description: "Advanced browser automation tool. Supports navigation, clicking, filling forms, and stateful sessions with diffing. Uses Deterministic Refs (@eN) for reliable interaction. 'screenshot' captures visual state.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["navigate", "click", "fill", "snapshot", "hover", "scroll", "save_session", "load_session", "diff", "screenshot"],
                        "description": "The action to perform"
                    },
                    "url": {
                        "type": "string",
                        "description": "URL for 'navigate' action"
                    },
                    "selector": {
                        "type": "string",
                        "description": "CSS selector or Ref (e.g., @e1) for interaction"
                    },
                    "text": {
                        "type": "string",
                        "description": "Text for 'fill' or session key for 'save/load'"
                    },
                    "wait_ms": {
                        "type": "integer",
                        "description": "Wait time after action (ms)",
                        "default": 1000
                    },
                    "interactive_only": {
                        "type": "boolean",
                        "description": "For 'snapshot': Only include interactive elements",
                        "default": true
                    },
                    "compact": {
                        "type": "boolean",
                        "description": "For 'snapshot': Filter out structural nodes without text",
                        "default": true
                    }
                },
                "required": ["action"]
            }),
            parameters_ts: Some("interface BrowserActionArgs {\n  action: 'navigate' | 'click' | 'fill' | 'snapshot' | 'hover' | 'scroll' | 'save_session' | 'load_session' | 'diff' | 'screenshot';\n  url?: string;\n  selector?: string;\n  text?: string;\n  wait_ms?: number;\n  interactive_only?: boolean;\n  compact?: boolean;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use 'snapshot' to discover @eN refs. Use 'diff' to see changes. Use 'screenshot' for visual verification.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            action: String,
            url: Option<String>,
            selector: Option<String>,
            text: Option<String>,
            wait_ms: Option<u64>,
            interactive_only: Option<bool>,
            compact: Option<bool>,
        }

        let args: Args = serde_json::from_str(arguments)?;
        let browser = self.get_browser()?;
        let wait_time = tokio::time::Duration::from_millis(args.wait_ms.unwrap_or(1000));

        let tabs_arc = browser.get_tabs();
        let tab = {
            let tabs = tabs_arc.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
            if tabs.is_empty() {
                browser.new_tab().map_err(|e| anyhow::anyhow!("Failed to open tab: {}", e))?
            } else {
                tabs[0].clone()
            }
        };

        match args.action.as_str() {
            "navigate" => {
                let url = args.url.ok_or_else(|| anyhow::anyhow!("URL required"))?;
                tab.navigate_to(&url)?
                    .wait_until_navigated()?;
                tokio::time::sleep(wait_time).await;
                Ok(format!("Navigated to {}", url))
            }
            "snapshot" => {
                let interactive = args.interactive_only.unwrap_or(true);
                let compact = args.compact.unwrap_or(true);
                let snapshot = self.extract_aria_tree_with_refs(tab, interactive, compact).await?;
                
                // Store for diffing
                *self.last_snapshot.lock() = Some(snapshot.tree.clone());
                
                Ok(format!("Snapshot:\n\n{}", snapshot.tree))
            }
            "diff" => {
                let interactive = args.interactive_only.unwrap_or(true);
                let compact = args.compact.unwrap_or(true);
                let current = self.extract_aria_tree_with_refs(tab, interactive, compact).await?;
                let last_opt = self.last_snapshot.lock().clone();
                
                if let Some(last) = last_opt {
                    if last == current.tree {
                        Ok("No changes detected since last snapshot.".to_string())
                    } else {
                        // Improved line-based diff showing additions and removals
                        let last_lines: Vec<&str> = last.lines().collect();
                        let current_lines: Vec<&str> = current.tree.lines().collect();
                        let last_set: std::collections::HashSet<&str> = last_lines.iter().cloned().collect();
                        let current_set: std::collections::HashSet<&str> = current_lines.iter().cloned().collect();
                        
                        let mut diff = String::new();
                        diff.push_str("### Snapshot Diff:\n");
                        
                        // Show removals
                        for line in last_lines {
                            if !current_set.contains(line) {
                                diff.push_str(&format!("- {}\n", line));
                            }
                        }
                        
                        // Show additions
                        for line in current_lines {
                            if !last_set.contains(line) {
                                diff.push_str(&format!("+ {}\n", line));
                            }
                        }
                        
                        *self.last_snapshot.lock() = Some(current.tree);
                        Ok(diff)
                    }
                } else {
                    *self.last_snapshot.lock() = Some(current.tree.clone());
                    Ok(format!("No previous snapshot found. Captured initial snapshot:\n\n{}", current.tree))
                }
            }
            "screenshot" => {
                let png_data = tab.capture_screenshot(
                    headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                    None,
                    None,
                    true
                ).map_err(|e| anyhow::anyhow!("Failed to capture screenshot: {}", e))?;
                
                // For now, return as base64 or save to a known location
                let b64 = base64::Engine::encode(&base64::prelude::BASE64_STANDARD, &png_data);
                Ok(format!("Screenshot captured successfully (base64: {}...)", &b64[..50]))
            }
            "click" => {
                let sel = args.selector.ok_or_else(|| anyhow::anyhow!("Selector required"))?;
                tab.wait_for_element(&self.resolve_selector(&sel))?.click()?;
                tokio::time::sleep(wait_time).await;
                Ok(format!("Clicked {}", sel))
            }
            "fill" => {
                let sel = args.selector.ok_or_else(|| anyhow::anyhow!("Selector required"))?;
                let text = args.text.ok_or_else(|| anyhow::anyhow!("Text required"))?;
                let el = tab.wait_for_element(&self.resolve_selector(&sel))?;
                el.click().ok();
                el.type_into(&text)?;
                tokio::time::sleep(wait_time).await;
                Ok(format!("Filled '{}' into '{}'", text, sel))
            }
            "hover" => {
                let sel = args.selector.ok_or_else(|| anyhow::anyhow!("Selector required"))?;
                let el = tab.wait_for_element(&self.resolve_selector(&sel))?;
                let midpoint = el.get_midpoint().map_err(|e| anyhow::anyhow!("Failed to get element midpoint: {}", e))?;
                tab.move_mouse_to_point(midpoint).map_err(|e| anyhow::anyhow!("Failed to move mouse: {}", e))?;
                tokio::time::sleep(wait_time).await;
                Ok(format!("Hovered {}", sel))
            }
            "scroll" => {
                if let Some(sel) = args.selector {
                    tab.wait_for_element(&self.resolve_selector(&sel))?.scroll_into_view()?;
                } else {
                    tab.evaluate("window.scrollBy(0, 500)", false)?;
                }
                tokio::time::sleep(wait_time).await;
                Ok("Scrolled".to_string())
            }
            "save_session" => {
                let key = args.text.ok_or_else(|| anyhow::anyhow!("Session key required in 'text' field"))?;
                let vault = self.vault.as_ref().ok_or_else(|| anyhow::anyhow!("SecretVault not configured"))?;
                
                let cookies = tab.get_cookies().map_err(|e| anyhow::anyhow!("Failed to get cookies: {}", e))?;
                let serialized = serde_json::to_string(&cookies)?;
                vault.set(&format!("browser_session_{}", key), &serialized)?;
                
                Ok(format!("Successfully saved {} cookies to local Vault under key '{}'", cookies.len(), key))
            }
            "load_session" => {
                let key = args.text.ok_or_else(|| anyhow::anyhow!("Session key required in 'text' field"))?;
                let vault = self.vault.as_ref().ok_or_else(|| anyhow::anyhow!("SecretVault not configured"))?;
                
                if let Some(data) = vault.get(&format!("browser_session_{}", key))? {
                    // Use tab.call_method with correct protocol path
                    tab.call_method(headless_chrome::protocol::cdp::Network::SetCookies {
                        cookies: serde_json::from_str::<serde_json::Value>(&data)?
                            .as_array()
                            .ok_or_else(|| anyhow::anyhow!("Invalid cookie data"))?
                            .iter()
                            .map(|v| serde_json::from_value(v.clone()).unwrap())
                            .collect()
                    }).map_err(|e| anyhow::anyhow!("Failed to set cookies: {}", e))?;
                    Ok(format!("Successfully loaded session '{}' from local Vault.", key))
                } else {
                    Err(anyhow::anyhow!("Session '{}' not found in local Vault", key))
                }
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", args.action)),
        }
    }
}
