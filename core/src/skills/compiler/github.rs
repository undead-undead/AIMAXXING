use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use std::sync::Arc;
use crate::notification::{Notifier, NotifyChannel};
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct GithubCompiler {
    repo: String, // e.g., "undead-undead/aimaxxing-compiler"
    token: String,
    workflow_id: String, // e.g., "compiler.yml"
    notifier: Option<Arc<dyn Notifier>>,
}

impl std::fmt::Debug for GithubCompiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubCompiler")
            .field("repo", &self.repo)
            .field("workflow_id", &self.workflow_id)
            .field("notifier", &self.notifier.as_ref().map(|_| "Some(Notifier)"))
            .finish()
    }
}

#[derive(Serialize)]
struct WorkflowDispatch {
    #[serde(rename = "ref")]
    branch: String,
    inputs: serde_json::Value,
}

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
    status: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
struct Artifact {
    #[allow(dead_code)]
    id: u64,
    name: String,
    archive_download_url: String,
}

#[derive(Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<Artifact>,
}

impl GithubCompiler {
    pub fn new(repo: String, token: String, notifier: Option<Arc<dyn Notifier>>) -> Self {
        Self {
            repo,
            token,
            workflow_id: "compiler.yml".to_string(),
            notifier,
        }
    }

    /// Trigger compilation and wait for result
    pub async fn compile(&self, skill_name: &str, source_code: &str) -> Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let url = format!("https://api.github.com/repos/{}/actions/workflows/{}/dispatches", self.repo, self.workflow_id);

        if let Some(notifier) = &self.notifier {
            let _ = notifier.notify(NotifyChannel::Log, &format!("GitHub: Starting compilation for {}", skill_name)).await;
        }

        info!("Triggering GitHub compilation for skill: {}", skill_name);

        // 1. Trigger workflow
        let response = client.post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "AIMAXXING-Agent")
            .json(&WorkflowDispatch {
                branch: "main".to_string(),
                inputs: serde_json::json!({
                    "source_code": source_code,
                    "skill_name": skill_name,
                }),
            })
            .send()
            .await
            .map_err(|e| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Failed to trigger workflow: {}", e) 
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("GitHub API error ({}): {}", status, body) 
            });
        }

        // 2. Poll for the latest run
        sleep(Duration::from_secs(5)).await; 
        
        let run_id = self.wait_for_run(&client, skill_name).await?;
        
        // 3. Wait for completion
        let result_run = self.wait_for_completion(&client, run_id).await?;
        
        if result_run.conclusion.as_deref() != Some("success") {
             return Err(Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Compilation failed on GitHub. Conclusion: {:?}", result_run.conclusion) 
            });
        }

        // 4. Download artifact
        self.download_artifact(&client, run_id, skill_name).await
    }

    async fn wait_for_run(&self, client: &reqwest::Client, _skill_name: &str) -> Result<u64> {
        let url = format!("https://api.github.com/repos/{}/actions/runs", self.repo);
        
        for _ in 0..12 { // Try for 1 minute
            let response = client.get(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("User-Agent", "AIMAXXING-Agent")
                .send()
                .await
                .map_err(|e| Error::ToolExecution { 
                    tool_name: "github_compiler".to_string(), 
                    message: format!("Failed to poll runs: {}", e) 
                })?;

            let body: WorkflowRunsResponse = response.json().await.map_err(|e| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Failed to parse runs: {}", e) 
            })?;

            if let Some(run) = body.workflow_runs.first() {
                // Return the first one for now, assuming it's ours if it was just created
                return Ok(run.id);
            }
            sleep(Duration::from_secs(5)).await;
        }

        Err(Error::ToolExecution { 
            tool_name: "github_compiler".to_string(), 
            message: "Timed out waiting for workflow job to start".to_string() 
        })
    }

    async fn wait_for_completion(&self, client: &reqwest::Client, run_id: u64) -> Result<WorkflowRun> {
        let url = format!("https://api.github.com/repos/{}/actions/runs/{}", self.repo, run_id);
        
        for _ in 0..60 { // Try for 10 minutes
            let response = client.get(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("User-Agent", "AIMAXXING-Agent")
                .send()
                .await
                .map_err(|e| Error::ToolExecution { 
                    tool_name: "github_compiler".to_string(), 
                    message: format!("Failed to check run status: {}", e) 
                })?;

            let run: WorkflowRun = response.json().await.map_err(|e| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Failed to parse run status: {}", e) 
            })?;

            if run.status == "completed" {
                return Ok(run);
            }
            
            if let Some(notifier) = &self.notifier {
                let _ = notifier.notify(NotifyChannel::Log, &format!("GitHub: Build in progress (Run {})", run_id)).await;
            }
            
            info!("Waiting for GitHub build (Run ID: {})...", run_id);
            sleep(Duration::from_secs(10)).await;
        }

        Err(Error::ToolExecution { 
            tool_name: "github_compiler".to_string(), 
            message: "Timed out waiting for workflow completion".to_string() 
        })
    }

    async fn download_artifact(&self, client: &reqwest::Client, run_id: u64, skill_name: &str) -> Result<Vec<u8>> {
        let url = format!("https://api.github.com/repos/{}/actions/runs/{}/artifacts", self.repo, run_id);
        
        let response = client.get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "AIMAXXING-Agent")
            .send()
            .await
            .map_err(|e| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Failed to list artifacts: {}", e) 
            })?;

        let body: ArtifactsResponse = response.json().await.map_err(|e| Error::ToolExecution { 
            tool_name: "github_compiler".to_string(), 
            message: format!("Failed to parse artifacts: {}", e) 
        })?;

        let artifact = body.artifacts.iter()
            .find(|a| a.name == skill_name)
            .ok_or_else(|| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Artifact '{}' not found in run {}", skill_name, run_id) 
            })?;

        info!("Downloading artifact: {} from {}", artifact.name, artifact.archive_download_url);

        let download_response = client.get(&artifact.archive_download_url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "AIMAXXING-Agent")
            .send()
            .await
            .map_err(|e| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Failed to download artifact zip: {}", e) 
            })?;

        let zip_data = download_response.bytes().await.map_err(|e| Error::ToolExecution { 
            tool_name: "github_compiler".to_string(), 
            message: format!("Failed to read artifact bytes: {}", e) 
        })?;

        use std::io::Read;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_data))
            .map_err(|e| Error::ToolExecution { 
                tool_name: "github_compiler".to_string(), 
                message: format!("Failed to open artifact zip: {}", e) 
            })?;

        let mut file = archive.by_index(0).map_err(|e| Error::ToolExecution { 
            tool_name: "github_compiler".to_string(), 
            message: format!("Failed to find file in zip: {}", e) 
        })?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| Error::ToolExecution { 
            tool_name: "github_compiler".to_string(), 
            message: format!("Failed to read file from zip: {}", e) 
        })?;

        Ok(buffer)
    }
}
