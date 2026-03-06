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
                "bash" | "sh" | "shell" => {
                    if cfg!(target_os = "windows") {
                        pixi_deps.push("m2-bash".to_string());
                        // Optional: pull in m2-coreutils or m2-grep to give a full environment
                        pixi_deps.push("m2-coreutils".to_string());
                        pixi_deps.push("m2-grep".to_string());
                        pixi_deps.push("m2-sed".to_string());
                        pixi_deps.push("m2-gawk".to_string());
                        pixi_deps.push("m2-curl".to_string());
                    } else {
                        // On Linux/Mac, rely on system bash implicitly, no conda pkg usually needed,
                        // but if they really requested bash via conda we can add `bash`
                        pixi_deps.push("bash".to_string());
                    }
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

        let pixi_bin = if let Ok(bin) = which::which("pixi") {
            bin.to_string_lossy().to_string()
        } else {
            // Priority: Local bin folder next to persistence layer
            let local_bin = self.base_storage.parent()
                .map(|p| p.join("bin"))
                .unwrap_or_else(|| self.base_storage.clone());
            
            let pixi_local = local_bin.join(if cfg!(target_os = "windows") { "pixi.exe" } else { "pixi" });
            
            if pixi_local.exists() {
                pixi_local.to_string_lossy().to_string()
            } else {
                // Fallback to standard install paths
                let home = dirs::home_dir().unwrap_or_default();
                let paths = if cfg!(target_os = "windows") {
                    vec![
                        home.join(".pixi").join("bin").join("pixi.exe"),
                        dirs::data_local_dir().unwrap_or_default().join("pixi").join("bin").join("pixi.exe"),
                    ]
                } else {
                    vec![home.join(".pixi").join("bin").join("pixi")]
                };

                paths.into_iter()
                    .find(|p| p.exists())
                    .map(|p| p.to_string_lossy().to_string())
                    .ok_or_else(|| {
                        Error::Internal("pixi binary not found. Please ensure it exists in the 'bin' folder or system PATH.".to_string())
                    })?
            }
        };

        let output = Command::new(&pixi_bin)
            .arg("install")
            .arg("--manifest-path")
            .arg(env_path.join("pixi.toml"))
            .output()
            .await
            .map_err(|e| Error::Internal(format!("Failed to run pixi install at {}: {}", pixi_bin, e)))?;

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

    /// Compute SHA256 checksum of a file natively using the sha2 crate.
    async fn compute_sha256(&self, path: &Path) -> Result<String> {
        use sha2::{Digest, Sha256};
        use tokio::fs::File;
        use tokio::io::AsyncReadExt;

        let mut file = File::open(path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to open file for checksum: {}", e)))?;
        
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .await
                .map_err(|e| Error::Internal(format!("Failed to read file for checksum: {}", e)))?;
            
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let result = hasher.finalize();
        Ok(format!("{:x}", result))
    }

    /// Check if there's enough disk space for a model download using sysinfo.
    async fn check_disk_space(&self, path: &Path, required_mb: u64) -> Result<()> {
        use sysinfo::Disks;

        let required_bytes = required_mb * 1024 * 1024;
        let required_with_buffer = required_bytes + (100 * 1024 * 1024); // 100MB buffer
        
        // sysinfo needs to enumerate disks
        let disks = Disks::new_with_refreshed_list();
        
        // Find the disk containing the path
        let mut target_disk = None;
        let mut longest_match = 0;
        
        let path_str = path.to_string_lossy().to_string();
        
        for disk in &disks {
            let mount_point = disk.mount_point().to_string_lossy().to_string();
            if path_str.starts_with(&mount_point) && mount_point.len() > longest_match {
                longest_match = mount_point.len();
                target_disk = Some(disk);
            }
        }
        
        if let Some(disk) = target_disk {
            let available_bytes = disk.available_space();
            if available_bytes < required_with_buffer {
                return Err(Error::Internal(format!(
                    "Insufficient disk space: {} MB available, {} MB required for model",
                    available_bytes / 1024 / 1024,
                    required_mb
                )));
            }
        } else {
            warn!("Could not determine mount point for {}, skipping free space check.", path.display());
        }

        Ok(())
    }

    /// Ensure `uv` is available, downloading it if necessary.
    pub async fn ensure_uv(&self) -> Result<PathBuf> {
        if let Ok(bin) = which::which("uv") {
            return Ok(bin);
        }

        let bin_dir = self.base_storage.parent()
            .map(|p| p.join("bin"))
            .unwrap_or_else(|| self.base_storage.clone());
        
        if !bin_dir.exists() {
            tokio::fs::create_dir_all(&bin_dir).await?;
        }

        let uv_bin = bin_dir.join(if cfg!(windows) { "uv.exe" } else { "uv" });
        if uv_bin.exists() {
            return Ok(uv_bin);
        }

        info!("uv not found. Downloading standalone uv binary...");
        
        let url = if cfg!(target_os = "windows") {
            "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip"
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "https://github.com/astral-sh/uv/releases/latest/download/uv-aarch64-apple-darwin.tar.gz"
            } else {
                "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-apple-darwin.tar.gz"
            }
        } else {
            "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-unknown-linux-gnu.tar.gz"
        };

        self.download_file(url, &uv_bin).await?;
        
        Ok(uv_bin)
    }

    /// Ensure `pixi` is available, downloading it if necessary.
    pub async fn ensure_pixi(&self) -> Result<PathBuf> {
        if let Ok(bin) = which::which("pixi") {
            return Ok(bin);
        }

        let bin_dir = self.base_storage.parent()
            .map(|p| p.join("bin"))
            .unwrap_or_else(|| self.base_storage.clone());

        let pixi_bin = bin_dir.join(if cfg!(windows) { "pixi.exe" } else { "pixi" });
        if pixi_bin.exists() {
            return Ok(pixi_bin);
        }

        info!("pixi not found. Downloading standalone pixi binary...");
        
        let url = if cfg!(target_os = "windows") {
            "https://github.com/prefix-dev/pixi/releases/latest/download/pixi-x86_64-pc-windows-msvc.exe"
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "https://github.com/prefix-dev/pixi/releases/latest/download/pixi-aarch64-apple-darwin"
            } else {
                "https://github.com/prefix-dev/pixi/releases/latest/download/pixi-x86_64-apple-darwin"
            }
        } else {
            "https://github.com/prefix-dev/pixi/releases/latest/download/pixi-x86_64-unknown-linux-musl"
        };

        self.download_file(url, &pixi_bin).await?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&pixi_bin).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&pixi_bin, perms).await?;
        }

        Ok(pixi_bin)
    }
}
