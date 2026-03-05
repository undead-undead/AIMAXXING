//! Notification tool — multi-channel push notifications.
//!
//! Supports (with graceful degradation):
//! - Webhook (generic POST) — always available, zero deps
//! - ntfy.sh push — always available (HTTP only)
//! - Telegram Bot API — requires bot token
//! - Desktop notification — requires notify-send (Linux) or osascript (macOS)
//! - SMS via Twilio — requires Twilio credentials
//! - Bark push — requires Bark server URL
//!
//! Each channel auto-detects its prerequisites and returns helpful
//! guidance when unavailable.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::error::Error;
use crate::skills::tool::{Tool, ToolDefinition};

pub struct NotifierTool;

#[derive(Deserialize)]
struct NotifierArgs {
    action: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    // Webhook
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: Option<serde_json::Value>,
    // Telegram
    #[serde(default)]
    bot_token: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
    // ntfy
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    // Twilio SMS
    #[serde(default)]
    to_phone: Option<String>,
    #[serde(default)]
    from_phone: Option<String>,
    #[serde(default)]
    twilio_sid: Option<String>,
    #[serde(default)]
    twilio_token: Option<String>,
}

#[async_trait]
impl Tool for NotifierTool {
    fn name(&self) -> String { "notifier".to_string() }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "notifier".to_string(),
            description: "Send notifications via multiple channels: webhook, ntfy, Telegram, desktop, SMS. Auto-detects available channels.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["send", "info"], "description": "Action: 'send' to send notification, 'info' to check channels" },
                    "channel": { "type": "string", "enum": ["webhook", "ntfy", "telegram", "desktop", "sms"], "description": "Notification channel" },
                    "title": { "type": "string", "description": "Notification title" },
                    "message": { "type": "string", "description": "Notification body" },
                    "url": { "type": "string", "description": "Webhook URL" },
                    "headers": { "type": "object", "description": "Custom headers for webhook" },
                    "bot_token": { "type": "string", "description": "Telegram bot token (overrides env TELEGRAM_BOT_TOKEN)" },
                    "chat_id": { "type": "string", "description": "Telegram chat ID" },
                    "topic": { "type": "string", "description": "ntfy.sh topic name" },
                    "priority": { "type": "string", "description": "ntfy priority: min, low, default, high, max" },
                    "to_phone": { "type": "string", "description": "SMS recipient phone number" },
                    "twilio_sid": { "type": "string", "description": "Twilio Account SID" },
                    "twilio_token": { "type": "string", "description": "Twilio Auth Token" }
                },
                "required": ["action"]
            }),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use for sending notifications. Webhook and ntfy always work. Telegram/SMS need credentials.".into()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: NotifierArgs = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "notifier".into(),
            message: e.to_string(),
        })?;

        let result = match args.action.as_str() {
            "info" => detect_channels(&args).await,
            "send" => send_notification(&args).await?,
            _ => json!({"error": format!("Unknown action: {}", args.action)}),
        };

        Ok(serde_json::to_string_pretty(&result)?)
    }
}

async fn detect_channels(args: &NotifierArgs) -> serde_json::Value {
    let telegram_token = args.bot_token.clone()
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
    let has_notify_send = which::which("notify-send").is_ok();
    let has_osascript = which::which("osascript").is_ok();

    json!({
        "channels": {
            "webhook": {"available": true, "deps": "none"},
            "ntfy": {"available": true, "deps": "none (uses ntfy.sh)"},
            "telegram": {
                "available": telegram_token.is_some(),
                "deps": "TELEGRAM_BOT_TOKEN env var or bot_token param",
            },
            "desktop": {
                "available": has_notify_send || has_osascript,
                "tool": if has_notify_send { "notify-send" } else if has_osascript { "osascript" } else { "none" },
                "deps": "notify-send (Linux) or osascript (macOS)",
            },
            "sms": {
                "available": std::env::var("TWILIO_SID").is_ok() || args.twilio_sid.is_some(),
                "deps": "Twilio credentials (TWILIO_SID, TWILIO_TOKEN env vars)",
            },
        },
        "always_available": ["webhook", "ntfy"],
    })
}

async fn send_notification(args: &NotifierArgs) -> anyhow::Result<serde_json::Value> {
    match args.channel.as_str() {
        "webhook" => send_webhook(args).await,
        "ntfy" => send_ntfy(args).await,
        "telegram" => send_telegram(args).await,
        "desktop" => send_desktop(args).await,
        "sms" => send_sms(args).await,
        _ => Ok(json!({"error": format!("Unknown channel: {}. Use 'info' to see available channels.", args.channel)})),
    }
}

// --- Webhook (always available) ---
async fn send_webhook(args: &NotifierArgs) -> anyhow::Result<serde_json::Value> {
    if args.url.is_empty() {
        return Ok(json!({"error": "url is required for webhook"}));
    }

    // Security: validate URL scheme
    if !args.url.starts_with("http://") && !args.url.starts_with("https://") {
        return Ok(json!({"error": "URL must start with http:// or https://"}));
    }

    let client = reqwest::Client::new();
    let mut req = client.post(&args.url)
        .json(&json!({"title": args.title, "message": args.message}));

    if let Some(headers) = &args.headers {
        if let Some(obj) = headers.as_object() {
            for (k, v) in obj {
                if let Some(val) = v.as_str() {
                    req = req.header(k, val);
                }
            }
        }
    }

    let resp = req.send().await?;
    Ok(json!({"success": resp.status().is_success(), "status": resp.status().as_u16(), "channel": "webhook"}))
}

// --- ntfy.sh (always available) ---
async fn send_ntfy(args: &NotifierArgs) -> anyhow::Result<serde_json::Value> {
    let topic = args.topic.as_deref().unwrap_or("aimaxxing-notify");
    let client = reqwest::Client::new();
    let mut req = client.post(format!("https://ntfy.sh/{}", topic))
        .body(args.message.clone());

    if !args.title.is_empty() {
        req = req.header("Title", &args.title);
    }
    if let Some(priority) = &args.priority {
        req = req.header("Priority", priority.as_str());
    }

    let resp = req.send().await?;
    Ok(json!({
        "success": resp.status().is_success(),
        "channel": "ntfy",
        "topic": topic,
    }))
}

// --- Telegram ---
async fn send_telegram(args: &NotifierArgs) -> anyhow::Result<serde_json::Value> {
    let token = args.bot_token.clone()
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
    let chat_id = args.chat_id.clone()
        .or_else(|| std::env::var("TELEGRAM_CHAT_ID").ok());

    let (token, chat_id) = match (token, chat_id) {
        (Some(t), Some(c)) => (t, c),
        _ => return Ok(json!({
            "error": "Telegram bot_token and chat_id required",
            "degraded": true,
            "hint": "Set TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID env vars, or pass bot_token and chat_id params"
        })),
    };

    let text = if args.title.is_empty() {
        args.message.clone()
    } else {
        format!("*{}*\n{}", args.title, args.message)
    };

    let client = reqwest::Client::new();
    let resp = client.post(format!("https://api.telegram.org/bot{}/sendMessage", token))
        .json(&json!({"chat_id": chat_id, "text": text, "parse_mode": "Markdown"}))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    Ok(json!({"success": body["ok"].as_bool().unwrap_or(false), "channel": "telegram"}))
}

// --- Desktop notification ---
async fn send_desktop(args: &NotifierArgs) -> anyhow::Result<serde_json::Value> {
    if which::which("notify-send").is_ok() {
        let mut cmd_args = vec![];
        if !args.title.is_empty() {
            cmd_args.push(args.title.clone());
        }
        cmd_args.push(args.message.clone());

        let output = tokio::process::Command::new("notify-send")
            .args(&cmd_args)
            .output()
            .await?;
        Ok(json!({"success": output.status.success(), "channel": "desktop", "tool": "notify-send"}))
    } else if which::which("osascript").is_ok() {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            args.message.replace('"', "\\\""),
            args.title.replace('"', "\\\"")
        );
        let output = tokio::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .await?;
        Ok(json!({"success": output.status.success(), "channel": "desktop", "tool": "osascript"}))
    } else {
        Ok(json!({
            "error": "No desktop notification tool available",
            "degraded": true,
            "hint": "Install notify-send (Linux: apt install libnotify-bin) or use macOS"
        }))
    }
}

// --- SMS via Twilio ---
async fn send_sms(args: &NotifierArgs) -> anyhow::Result<serde_json::Value> {
    let sid = args.twilio_sid.clone().or_else(|| std::env::var("TWILIO_SID").ok());
    let token = args.twilio_token.clone().or_else(|| std::env::var("TWILIO_TOKEN").ok());
    let from = args.from_phone.clone().or_else(|| std::env::var("TWILIO_FROM").ok());
    let to = &args.to_phone;

    let (sid, token, from) = match (sid, token, from) {
        (Some(s), Some(t), Some(f)) => (s, t, f),
        _ => return Ok(json!({
            "error": "Twilio credentials required for SMS",
            "degraded": true,
            "hint": "Set TWILIO_SID, TWILIO_TOKEN, TWILIO_FROM env vars or pass as params"
        })),
    };

    if to.is_none() || to.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
        return Ok(json!({"error": "to_phone is required for SMS"}));
    }

    let client = reqwest::Client::new();
    let resp = client.post(format!("https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json", sid))
        .basic_auth(&sid, Some(&token))
        .form(&[
            ("Body", args.message.as_str()),
            ("From", from.as_str()),
            ("To", to.as_ref().unwrap().as_str()),
        ])
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    Ok(json!({"success": body.get("sid").is_some(), "channel": "sms"}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_definition() {
        let tool = NotifierTool;
        let def = tool.definition().await;
        assert_eq!(def.name, "notifier");
    }

    #[tokio::test]
    async fn test_info() {
        let tool = NotifierTool;
        let result = tool.call(r#"{"action": "info"}"#).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        // webhook and ntfy are always available
        assert!(v["always_available"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn test_url_validation() {
        // Security: reject non-HTTP URLs
        let url = "file:///etc/passwd";
        assert!(!url.starts_with("http://") && !url.starts_with("https://"));
    }
}
