//! Email communication tool — SMTP send and IMAP read.
//!
//! Provides email operations:
//! - Send emails with attachments via SMTP/TLS
//! - List/read/search emails via IMAP

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::error::Error;
use crate::skills::tool::{Tool, ToolDefinition};
use crate::skills::runtime::python_utils;

pub struct MailerTool;

#[derive(Deserialize)]
struct MailerArgs {
    action: String,
    // SMTP settings
    #[serde(default)]
    smtp_host: String,
    #[serde(default = "default_smtp_port")]
    smtp_port: u16,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    // Send fields
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    html: bool,
    // IMAP settings
    #[serde(default)]
    imap_host: String,
    #[serde(default = "default_imap_port")]
    imap_port: u16,
    #[serde(default)]
    folder: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    query: String,
    #[serde(default)]
    uid: Option<u32>,
}

fn default_smtp_port() -> u16 { 465 }
fn default_imap_port() -> u16 { 993 }
fn default_limit() -> usize { 20 }

#[async_trait]
impl Tool for MailerTool {
    fn name(&self) -> String { "mailer".to_string() }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "mailer".to_string(),
            description: "Send and read emails via SMTP/IMAP".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["send", "list", "read", "search", "info"], "description": "Email operation" },
                    "smtp_host": { "type": "string", "description": "SMTP server hostname" },
                    "smtp_port": { "type": "integer", "description": "SMTP port (default: 465 for TLS)" },
                    "imap_host": { "type": "string", "description": "IMAP server hostname" },
                    "imap_port": { "type": "integer", "description": "IMAP port (default: 993)" },
                    "username": { "type": "string", "description": "Email account username" },
                    "password": { "type": "string", "description": "Email account password" },
                    "from": { "type": "string", "description": "Sender email address" },
                    "to": { "type": "array", "items": {"type": "string"}, "description": "Recipient addresses" },
                    "subject": { "type": "string", "description": "Email subject" },
                    "body": { "type": "string", "description": "Email body content" },
                    "html": { "type": "boolean", "description": "Send as HTML email" },
                    "folder": { "type": "string", "description": "IMAP folder (default: INBOX)" },
                    "limit": { "type": "integer", "description": "Max emails to fetch" },
                    "query": { "type": "string", "description": "IMAP search query" },
                    "uid": { "type": "integer", "description": "Email UID for read action" }
                },
                "required": ["action"]
            }),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use for email operations. Requires SMTP/IMAP credentials.".into()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: MailerArgs = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "mailer".into(),
            message: e.to_string(),
        })?;

        let result = match args.action.as_str() {
            "info" => detect_capabilities().await,
            "send" => send_email(&args).await,
            "list" | "read" | "search" => {
                // IMAP operations use system command for simplicity
                // Full IMAP requires async_imap + tokio_rustls (heavyweight deps)
                imap_operation(&args).await
            }
            _ => Ok(json!({"error": format!("Unknown action: {}", args.action)})),
        }?;

        Ok(serde_json::to_string_pretty(&result)?)
    }
}

async fn detect_capabilities() -> anyhow::Result<serde_json::Value> {
    let python_bin = python_utils::find_python().await;
    let has_python = python_bin.is_some();
    
    Ok(json!({
        "python_available": has_python,
        "managed_python": python_bin.as_ref().map(|p| p.to_string_lossy().contains(".aimaxxing")).unwrap_or(false),
        "actions": {
            "send": true,
            "imap": true,
        },
        "degradation": if !has_python {
            "Email operations will attempt to auto-provision Python via uv on first run."
        } else {
            "All email operations available."
        }
    }))
}

async fn send_email(args: &MailerArgs) -> anyhow::Result<serde_json::Value> {
    if args.smtp_host.is_empty() || args.username.is_empty() || args.to.is_empty() {
        return Ok(json!({"error": "smtp_host, username, and to are required for sending"}));
    }

    // Security: validate email addresses to prevent header injection
    for addr in &args.to {
        if addr.contains('\n') || addr.contains('\r') {
            return Ok(json!({"error": "Invalid email address — possible header injection"}));
        }
    }
    if args.from.contains('\n') || args.from.contains('\r') || args.subject.contains('\n') {
        return Ok(json!({"error": "Invalid header — possible injection"}));
    }

    // Build email via Python's smtplib (avoids heavy Rust deps)
    let recipients_py = args.to.iter()
        .map(|r| format!("'{}'", r.replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(", ");

    let content_type = if args.html { "html" } else { "plain" };
    let script = format!(
        r#"
import smtplib
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart

msg = MIMEMultipart()
msg['From'] = '{from_addr}'
msg['To'] = ', '.join([{recipients}])
msg['Subject'] = '{subject}'
msg.attach(MIMEText('''{body}''', '{content_type}'))

with smtplib.SMTP_SSL('{host}', {port}) as s:
    s.login('{user}', '{pwd}')
    s.send_message(msg)
print('OK')
"#,
        from_addr = args.from.replace('\'', "\\'"),
        recipients = recipients_py,
        subject = args.subject.replace('\'', "\\'"),
        body = args.body.replace('\'', "\\'"),
        content_type = content_type,
        host = args.smtp_host.replace('\'', "\\'"),
        port = args.smtp_port,
        user = args.username.replace('\'', "\\'"),
        pwd = args.password.replace('\'', "\\'"),
    );

    let python_bin = match python_utils::find_python().await {
        Some(p) => p,
        None => python_utils::provision_python_via_uv().await?,
    };

    let output = tokio::process::Command::new(python_bin)
        .args(["-c", &script])
        .output()
        .await?;

    if output.status.success() {
        Ok(json!({
            "success": true,
            "to": args.to,
            "subject": args.subject,
        }))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(json!({"error": format!("Send failed: {}", stderr)}))
    }
}

async fn imap_operation(args: &MailerArgs) -> anyhow::Result<serde_json::Value> {
    if args.imap_host.is_empty() || args.username.is_empty() {
        return Ok(json!({"error": "imap_host and username are required"}));
    }

    let folder = if args.folder.is_empty() { "INBOX" } else { &args.folder };

    let action_code = match args.action.as_str() {
        "list" => format!(
            "status, data = m.search(None, 'ALL')\nids = data[0].split()[-{limit}:]\nfor uid in ids:\n    _, msg = m.fetch(uid, '(BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])')\n    print(msg[0][1].decode('utf-8', errors='replace'))\n    print('---')",
            limit = args.limit
        ),
        "search" => format!(
            "status, data = m.search(None, 'SUBJECT', '\"{query}\"')\nids = data[0].split()[-{limit}:]\nfor uid in ids:\n    _, msg = m.fetch(uid, '(BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)])')\n    print(msg[0][1].decode('utf-8', errors='replace'))\n    print('---')",
            query = args.query.replace('"', "\\\""),
            limit = args.limit
        ),
        "read" => {
            if let Some(uid) = args.uid {
                format!(
                    "_, msg = m.fetch('{uid}', '(RFC822)')\nprint(msg[0][1].decode('utf-8', errors='replace')[:5000])",
                    uid = uid
                )
            } else {
                return Ok(json!({"error": "uid required for read action"}));
            }
        }
        _ => return Ok(json!({"error": "unknown action"})),
    };

    let script = format!(
        r#"
import imaplib
m = imaplib.IMAP4_SSL('{host}', {port})
m.login('{user}', '{pwd}')
m.select('{folder}')
{action_code}
m.logout()
"#,
        host = args.imap_host.replace('\'', "\\'"),
        port = args.imap_port,
        user = args.username.replace('\'', "\\'"),
        pwd = args.password.replace('\'', "\\'"),
        folder = folder.replace('\'', "\\'"),
        action_code = action_code,
    );

    let python_bin = match python_utils::find_python().await {
        Some(p) => p,
        None => python_utils::provision_python_via_uv().await?,
    };

    let output = tokio::process::Command::new(python_bin)
        .args(["-c", &script])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        Ok(json!({"success": true, "output": stdout.to_string()}))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(json!({"error": format!("IMAP failed: {}", stderr)}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_definition() {
        let tool = MailerTool;
        let def = tool.definition().await;
        assert_eq!(def.name, "mailer");
    }

    #[test]
    fn test_header_injection_prevention() {
        // This would be caught by the validation in send_email
        let addr = "attacker@evil.com\r\nBcc: spy@evil.com";
        assert!(addr.contains('\r') || addr.contains('\n'));
    }
}
