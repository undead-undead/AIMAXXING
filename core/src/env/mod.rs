use crate::error::{Error, Result};
use crate::skills::ModelSpec;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{error, info, warn};

pub mod env_scanner;

pub struct EnvManager {
    base_storage: PathBuf,
}

impl EnvManager {
    pub fn new(base_storage: PathBuf) -> Self {
        Self { base_storage }
    }

    /// Returns the models directory for a given skill.
    pub fn models_path(&self, skill_id: &str) -> PathBuf {
        self.base_storage.join(skill_id).join(".models")
    }

    /// Materialize an environment for a skill based on its requirements
    pub async fn provision(
        &self,
        skill_id: &str,
        dependencies: &[String],
        use_browser: bool,
    ) -> Result<PathBuf> {
        let env_path = self.base_storage.join(skill_id);

        let mut deps = dependencies.to_vec();
        if use_browser {
            deps.push("playwright-python".to_string());
            deps.push("python".to_string());
        }

        if env_path.exists() {
            // TODO: Incremental update check
            return Ok(env_path);
        }

        tokio::fs::create_dir_all(&env_path).await?;

        info!(skill = %skill_id, "Provisioning new environment via Pixi...");

        // Detect platform for pixi.toml
        let platform = if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "osx-arm64"
            } else {
                "osx-64"
            }
        } else if cfg!(target_os = "windows") {
            "win-64"
        } else if cfg!(target_arch = "aarch64") {
           "linux-aarch64"
        } else {
            "linux-64"
        };

        // Map standard dependency names to conda-forge counterparts
        let mut pixi_deps = Vec::new();
        for d in &deps {
            match d.to_lowercase().as_str() {
                "python" | "python3" => pixi_deps.push("python".to_string()),
                "js" | "node" | "nodejs" | "ts" | "typescript" | "bun" => pixi_deps.push("bun".to_string()),
                "c" | "gcc" | "cxx" => {
                    pixi_deps.push("gcc_linux-64".to_string());
                    pixi_deps.push("gxx_linux-64".to_string());
                },
                _ => pixi_deps.push(d.clone()),
            }
        }

        // Create a minimal pixi.toml (modern workspace format)
        let pixi_toml = format!(
            r#"[project]
name = "{}"
channels = ["conda-forge"]
platforms = ["{}"]

[dependencies]
{}
"#,
            skill_id,
            platform,
            pixi_deps.iter()
                .map(|d| format!("{} = \"*\"", d))
                .collect::<Vec<_>>()
                .join("\n")
        );

        tokio::fs::write(env_path.join("pixi.toml"), pixi_toml).await?;

        let pixi_bin = if which::which("pixi").is_ok() {
            "pixi".to_string()
        } else {
            let home = std::env::var("HOME").unwrap_or_default();
            let path = format!("{}/.pixi/bin/pixi", home);
            if std::path::Path::new(&path).exists() {
                path
            } else {
                return Err(Error::Internal(
                    "pixi binary not found. Please install it first.".to_string(),
                ));
            }
        };

        let output = Command::new(pixi_bin.clone())
            .arg("install")
            .arg("--manifest-path")
            .arg(env_path.join("pixi.toml"))
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run pixi install: {}", e)))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            error!("Pixi install failed: {}", err);
            return Err(Error::Internal(format!("Pixi install failed: {}", err)));
        }

        // Browser installation (Playwright)
        if use_browser {
            info!(skill = %skill_id, "Installing Chromium browser via Playwright...");
            let mut playwright_cmd = Command::new(pixi_bin);
            playwright_cmd
                .arg("run")
                .arg("--manifest-path")
                .arg(env_path.join("pixi.toml"))
                .arg("playwright")
                .arg("install")
                .arg("chromium");

            // Critical: Install browser INTO the env_path (not prefix yet, but top-level env_path)
            let browsers_path = env_path.join(".browsers");
            playwright_cmd.env("PLAYWRIGHT_BROWSERS_PATH", &browsers_path);

            let p_output = playwright_cmd
                .output()
                .await
                .map_err(|e| Error::Internal(format!("Failed to run playwright install: {}", e)))?;

            if !p_output.status.success() {
                let stderr = String::from_utf8_lossy(&p_output.stderr);
                warn!("Playwright install warning: {}", stderr);
            }
        }

        // Return the path to the prefix
        Ok(env_path.join(".pixi/envs/default"))
    }

    /// Provision model files for a skill.
    ///
    /// Downloads models from their source URLs, verifies checksums if available,
    /// and stores them in `.models/` under the skill's environment directory.
    /// Models are cached and not re-downloaded if they already exist.
    pub async fn provision_models(&self, skill_id: &str, models: &[ModelSpec]) -> Result<PathBuf> {
        let models_dir = self.models_path(skill_id);
        tokio::fs::create_dir_all(&models_dir).await?;

        for model in models {
            let model_file = models_dir.join(&model.name);

            // Skip if already downloaded
            if model_file.exists() {
                info!(
                    skill = %skill_id,
                    model = %model.name,
                    "Model already cached, skipping download"
                );
                continue;
            }

            // Check disk space if size hint is available
            if let Some(size_mb) = model.size_mb {
                self.check_disk_space(&models_dir, size_mb).await?;
            }

            info!(
                skill = %skill_id,
                model = %model.name,
                source = %model.source,
                format = %model.format,
                size_mb = ?model.size_mb,
                "Downloading model..."
            );

            // Resolve source URL (support Hugging Face shorthand)
            let download_url = self.resolve_model_url(&model.source);

            // Download the model
            self.download_file(&download_url, &model_file).await?;

            // Verify checksum if provided
            if let Some(ref expected_sha256) = model.sha256 {
                info!(model = %model.name, "Verifying SHA256 checksum...");
                let actual_sha256 = self.compute_sha256(&model_file).await?;
                if actual_sha256 != *expected_sha256 {
                    // Remove the corrupted file
                    let _ = tokio::fs::remove_file(&model_file).await;
                    return Err(Error::Internal(format!(
                        "Checksum mismatch for model '{}': expected {}, got {}",
                        model.name, expected_sha256, actual_sha256
                    )));
                }
                info!(model = %model.name, "Checksum verified ✓");
            }

            info!(
                skill = %skill_id,
                model = %model.name,
                path = %model_file.display(),
                "Model provisioned successfully"
            );
        }

        Ok(models_dir)
    }

    /// Resolve a model source URL. Supports:
    /// - Full HTTPS URLs (passed through)
    /// - Hugging Face shorthand: "hf://org/repo/file.onnx"
    /// - Local file paths (passed through)
    fn resolve_model_url(&self, source: &str) -> String {
        if let Some(path) = source.strip_prefix("hf://") {
            // Convert hf://org/repo/file.ext -> https://huggingface.co/org/repo/resolve/main/file.ext
            let parts: Vec<&str> = path.splitn(3, '/').collect();
            if parts.len() == 3 {
                format!(
                    "https://huggingface.co/{}/{}/resolve/main/{}",
                    parts[0], parts[1], parts[2]
                )
            } else {
                // Fallback: treat as raw URL
                source.to_string()
            }
        } else {
            source.to_string()
        }
    }

    /// Download a file from a URL to a local path using curl (available on both Linux and macOS).
    async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        // Use a temporary file to avoid partial downloads
        let tmp_dest = dest.with_extension("downloading");

        let output = Command::new("curl")
            .arg("-fSL") // fail silently on HTTP errors, show errors, follow redirects
            .arg("--progress-bar")
            .arg("-o")
            .arg(&tmp_dest)
            .arg(url)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run curl: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = tokio::fs::remove_file(&tmp_dest).await;
            return Err(Error::Internal(format!(
                "Failed to download model from '{}': {}",
                url, stderr
            )));
        }

        // Atomically rename to final path
        tokio::fs::rename(&tmp_dest, dest).await?;

        Ok(())
    }

    /// Compute SHA256 checksum of a file using the system's sha256sum / shasum.
    async fn compute_sha256(&self, path: &Path) -> Result<String> {
        // Try sha256sum (Linux) first, then shasum (macOS)
        let (program, args): (&str, Vec<&str>) = if which::which("sha256sum").is_ok() {
            ("sha256sum", vec![])
        } else if which::which("shasum").is_ok() {
            ("shasum", vec!["-a", "256"])
        } else {
            return Err(Error::Internal(
                "Neither sha256sum nor shasum found on system".to_string(),
            ));
        };

        let mut cmd = Command::new(program);
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.arg(path);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to compute checksum: {}", e)))?;

        if !output.status.success() {
            return Err(Error::Internal("sha256 computation failed".to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output format: "hash  filename\n"
        let hash = stdout.split_whitespace().next().unwrap_or("").to_string();

        Ok(hash)
    }

    /// Check if there's enough disk space for a model download.
    async fn check_disk_space(&self, path: &Path, required_mb: u64) -> Result<()> {
        // Use df to check available space
        let output = Command::new("df")
            .arg("-m") // megabytes
            .arg(path)
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to check disk space: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse the 'Available' column (4th field of the 2nd line)
            if let Some(line) = stdout.lines().nth(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 4 {
                    if let Ok(available_mb) = fields[3].parse::<u64>() {
                        if available_mb < required_mb + 100 {
                            // 100MB buffer
                            return Err(Error::Internal(format!(
                                "Insufficient disk space: {} MB available, {} MB required for model",
                                available_mb, required_mb
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
