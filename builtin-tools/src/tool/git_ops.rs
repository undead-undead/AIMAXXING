//! Git operations tool — version control automation for agents.
//!
//! Provides Git and GitHub REST API integration:
//! - Repository info, search, stars/forks
//! - Pull request management (list, create, merge)
//! - Issue operations (list, create, comment)
//! - Code search
//! - Local git commands (status, diff, log, commit)

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use brain::error::Error;
use brain::skills::tool::{Tool, ToolDefinition};

pub struct GitOpsTool;

#[derive(Deserialize)]
struct GitOpsArgs {
    action: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    repo: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    head: String,
    #[serde(default)]
    base: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    number: Option<u64>,
    #[serde(default)]
    path: String,
    #[serde(default)]
    message: String,
}

#[async_trait]
impl Tool for GitOpsTool {
    fn name(&self) -> String {
        "git_ops".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "git_ops".to_string(),
            description: "Git and GitHub operations — manage repos, PRs, issues, and local git commands".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["repo_info", "list_prs", "create_pr", "list_issues", "create_issue",
                                 "search_code", "local_status", "local_diff", "local_log", "local_commit", "info"],
                        "description": "Git operation to perform"
                    },
                    "owner": { "type": "string", "description": "Repository owner/organization" },
                    "repo": { "type": "string", "description": "Repository name" },
                    "token": { "type": "string", "description": "GitHub personal access token (overrides env GITHUB_TOKEN)" },
                    "title": { "type": "string", "description": "PR/Issue title" },
                    "body": { "type": "string", "description": "PR/Issue body" },
                    "head": { "type": "string", "description": "PR head branch" },
                    "base": { "type": "string", "description": "PR base branch (default: main)" },
                    "query": { "type": "string", "description": "Search query" },
                    "number": { "type": "integer", "description": "PR/Issue number" },
                    "path": { "type": "string", "description": "Working directory for local git commands" },
                    "message": { "type": "string", "description": "Commit message" }
                },
                "required": ["action"]
            }),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use for GitHub API operations and local git commands. Requires GITHUB_TOKEN env var or token param for API calls.".into()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: GitOpsArgs = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "git_ops".into(),
            message: e.to_string(),
        })?;

        let result = match args.action.as_str() {
            "info" => detect_capabilities().await,
            "repo_info" => github_api_get(&args, &format!("repos/{}/{}", args.owner, args.repo)).await,
            "list_prs" => github_api_get(&args, &format!("repos/{}/{}/pulls", args.owner, args.repo)).await,
            "create_pr" => {
                let body = json!({
                    "title": args.title,
                    "body": args.body,
                    "head": args.head,
                    "base": if args.base.is_empty() { "main" } else { &args.base },
                });
                github_api_post(&args, &format!("repos/{}/{}/pulls", args.owner, args.repo), &body).await
            }
            "list_issues" => github_api_get(&args, &format!("repos/{}/{}/issues", args.owner, args.repo)).await,
            "create_issue" => {
                let body = json!({ "title": args.title, "body": args.body });
                github_api_post(&args, &format!("repos/{}/{}/issues", args.owner, args.repo), &body).await
            }
            "search_code" => {
                let q = if args.query.contains("repo:") {
                    args.query.clone()
                } else {
                    format!("{} repo:{}/{}", args.query, args.owner, args.repo)
                };
                github_api_get(&args, &format!("search/code?q={}", urlencoding::encode(&q))).await
            }
            "local_status" | "local_diff" | "local_log" | "local_commit" => {
                local_git(&args).await
            }
            _ => Ok(json!({"error": format!("Unknown action: {}", args.action)})),
        }?;

        Ok(serde_json::to_string_pretty(&result)?)
    }
}

async fn detect_capabilities() -> anyhow::Result<serde_json::Value> {
    let has_git = which::which("git").is_ok();
    let has_token = std::env::var("GITHUB_TOKEN").is_ok();
    
    Ok(json!({
        "git_binary": has_git,
        "github_token_env": has_token,
        "actions": {
            "local_git": has_git,
            "github_api": true, // Always available but needs token for some ops
        },
        "degradation": if !has_git {
            "Local git operations (status, commit, etc.) are unavailable. Install git for local repo management."
        } else {
            "All git operations available."
        }
    }))
}

fn resolve_token(args: &GitOpsArgs) -> String {
    if !args.token.is_empty() {
        args.token.clone()
    } else {
        std::env::var("GITHUB_TOKEN").unwrap_or_default()
    }
}

async fn github_api_get(args: &GitOpsArgs, endpoint: &str) -> anyhow::Result<serde_json::Value> {
    let token = resolve_token(args);
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/{}", endpoint);
    let mut req = client.get(&url)
        .header("User-Agent", "aimaxxing-agent")
        .header("Accept", "application/vnd.github.v3+json");
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    let resp = req.send().await?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Ok(json!({"error": body, "status": status.as_u16()}));
    }
    Ok(body)
}

async fn github_api_post(args: &GitOpsArgs, endpoint: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let token = resolve_token(args);
    if token.is_empty() {
        return Ok(json!({"error": "GitHub token required for write operations"}));
    }
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/{}", endpoint);
    let resp = client.post(&url)
        .header("User-Agent", "aimaxxing-agent")
        .header("Accept", "application/vnd.github.v3+json")
        .header("Authorization", format!("Bearer {}", token))
        .json(body)
        .send()
        .await?;
    let status = resp.status();
    let result: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Ok(json!({"error": result, "status": status.as_u16()}));
    }
    Ok(result)
}

async fn local_git(args: &GitOpsArgs) -> anyhow::Result<serde_json::Value> {
    let cwd = if args.path.is_empty() { "." } else { &args.path };
    let (cmd_args, needs_input) = match args.action.as_str() {
        "local_status" => (vec!["status", "--porcelain"], false),
        "local_diff" => (vec!["diff", "--stat"], false),
        "local_log" => (vec!["log", "--oneline", "-20"], false),
        "local_commit" => {
            if args.message.is_empty() {
                return Ok(json!({"error": "commit message required"}));
            }
            (vec!["commit", "-am", &args.message], true)
        }
        _ => return Ok(json!({"error": "unknown local action"})),
    };
    let _ = needs_input;

    let output = tokio::process::Command::new("git")
        .args(&cmd_args)
        .current_dir(cwd)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(json!({
        "success": output.status.success(),
        "stdout": stdout,
        "stderr": stderr,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_definition() {
        let tool = GitOpsTool;
        let def = tool.definition().await;
        assert_eq!(def.name, "git_ops");
        assert!(def.is_verified);
    }

    #[tokio::test]
    async fn test_local_status() {
        let tool = GitOpsTool;
        let result = tool.call(r#"{"action": "local_status", "path": "."}"#).await;
        assert!(result.is_ok());
    }
}
