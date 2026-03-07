use crate::error::{Error, Result};
use crate::skills::ModelSpec;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{error, info, warn, debug};
use tokio::sync::OnceCell;
use std::sync::Arc;

pub mod env_scanner;

pub struct EnvManager {
    base_storage: PathBuf,
    // Concurrency locks for tool downloads
    pixi_lock: Arc<OnceCell<PathBuf>>,
    uv_lock: Arc<OnceCell<PathBuf>>,
    bun_lock: Arc<OnceCell<PathBuf>>,
    git_lock: Arc<OnceCell<PathBuf>>,
    gcc_lock: Arc<OnceCell<PathBuf>>,
}

impl EnvManager {
    pub fn new(base_storage: PathBuf) -> Self {
        Self { 
            base_storage,
            pixi_lock: Arc::new(OnceCell::new()),
            uv_lock: Arc::new(OnceCell::new()),
            bun_lock: Arc::new(OnceCell::new()),
            git_lock: Arc::new(OnceCell::new()),
            gcc_lock: Arc::new(OnceCell::new()),
        }
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

        // Handle Portable Toolchain dependencies (Bun, Git, GCC)
        let mut pixi_deps = Vec::new();
        for d in &deps {
            match d.to_lowercase().as_str() {
                "python" | "python3" => pixi_deps.push("python".to_string()),
                "bun" | "js" | "node" => {
                    let _ = self.ensure_bun().await;
                },
                "git" => {
                    let _ = self.ensure_git().await;
                },
                "gcc" | "c" | "c++" | "cpp" => {
                    let _ = self.ensure_gcc().await;
                },
                "bash" | "sh" | "shell" => {
                    #[cfg(target_os = "windows")]
                    let _ = self.ensure_git().await; // Git (MinGit) provides bash on Windows
                },
                _ => {}
            }
        }

        // If no explicit python but use_browser is true, we still need python for playwright
        if pixi_deps.is_empty() && use_browser {
            pixi_deps.push("python".to_string());
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
                // Self-Healing: Trigger auto-download if missing (Lite version behavior)
                match self.ensure_pixi().await {
                    Ok(path) => path.to_string_lossy().to_string(),
                    Err(_) => {
                        // Fallback to standard install paths as a last resort
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
                                Error::Internal("pixi binary not found and auto-provisioning failed. Please ensure it exists in the 'bin' folder or system PATH.".to_string())
                            })?
                    }
                }
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

        // --- Phase 1.1: UV Integration (Fast Pip) ---
        // Install Python dependencies that weren't in pixi_deps using uv
        let pip_deps: Vec<String> = deps.iter()
            .filter(|d| !pixi_deps.contains(d))
            .filter(|d| !["bun", "git", "bash", "sh", "gcc"].contains(&d.to_lowercase().as_str()))
            .cloned()
            .collect();

        if !pip_deps.is_empty() {
            info!(skill = %skill_id, "Installing pip dependencies via UV...");
            let uv_bin = self.ensure_uv().await?;
            let mut uv_cmd = Command::new(&pixi_bin);
            uv_cmd.arg("run").arg(&uv_bin).arg("pip").arg("install");
            for d in &pip_deps {
                uv_cmd.arg(d);
            }
            uv_cmd.arg("--manifest-path").arg(env_path.join("pixi.toml"));
            
            let uv_out = uv_cmd.output().await.map_err(|e| Error::Internal(format!("Failed to run uv: {}", e)))?;
            if !uv_out.status.success() {
                warn!("UV pip install warning: {}", String::from_utf8_lossy(&uv_out.stderr));
            }
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

    pub fn get_infra_bin_dir(&self) -> PathBuf {
        self.base_storage.parent()
            .map(|p| p.join("infra").join("bin"))
            .unwrap_or_else(|| self.base_storage.clone())
    }

    /// Returns the directory where tools might be bundled with the application EXE
    pub fn get_bundled_bin_dir(&self) -> Option<PathBuf> {
        std::env::current_exe().ok()?
            .parent()?
            .join("infra").join("bin")
            .into()
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
        
        let disks = Disks::new_with_refreshed_list();
        let mut target_disk = None;
        let mut longest_match = 0;
        
        let path_str = path.to_string_lossy().to_string();
        
        for disk in &disks {
            let mount_point = disk.mount_point().to_string_lossy().to_string();
            // Match the mount point that is a prefix of our path
            if path_str.starts_with(&mount_point) && mount_point.len() >= longest_match {
                longest_match = mount_point.len();
                target_disk = Some(disk);
            }
        }
        
        if let Some(disk) = target_disk {
            let available_bytes = disk.available_space();
            if available_bytes < required_with_buffer {
                error!("Disk space check failed: {} MB available, {} MB required", available_bytes / 1024 / 1024, required_mb);
                return Err(Error::Internal(format!(
                    "Insufficient disk space: {} MB available, {} MB required for model",
                    available_bytes / 1024 / 1024,
                    required_mb
                )));
            }
        }

        Ok(())
    }

    /// Ensure directory is writable, fallback to temp if not.
    pub async fn ensure_writable_dir(&self, dir: &Path) -> Result<PathBuf> {
        if !dir.exists() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        
        // Simple write test
        let test_file = dir.join(".aimaxxing_write_test");
        match tokio::fs::write(&test_file, b"test").await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(&test_file).await;
                Ok(dir.to_path_buf())
            },
            Err(_) => {
                let temp_dir = std::env::temp_dir().join("aimaxxing-infra-fallback");
                warn!("Directory {:?} not writable, falling back to {:?}", dir, temp_dir);
                tokio::fs::create_dir_all(&temp_dir).await.map_err(|e| Error::Internal(format!("Failed to create fallback temp dir: {}", e)))?;
                Ok(temp_dir)
            }
        }
    }

    /// Ensure `uv` is available, downloading it if necessary.
    pub async fn ensure_uv(&self) -> Result<PathBuf> {
        self.uv_lock.get_or_init(|| async {
            // 1. Check if bundled with installer (Offline-First)
            if let Some(bundled_dir) = self.get_bundled_bin_dir() {
                let uv_bundled = bundled_dir.join(if cfg!(windows) { "uv.exe" } else { "uv" });
                if uv_bundled.exists() {
                    debug!("Using bundled uv: {:?}", uv_bundled);
                    return uv_bundled;
                }
            }

            // 2. Check if already in system PATH
            if let Ok(bin) = which::which("uv") {
                return bin;
            }

            // 3. Check if already provisioned in infra/bin
            let bin_dir = self.ensure_writable_dir(&self.get_infra_bin_dir()).await.unwrap_or_else(|_| self.get_infra_bin_dir());
            let uv_bin = bin_dir.join(if cfg!(windows) { "uv.exe" } else { "uv" });
            if uv_bin.exists() {
                return uv_bin;
            }

            // 4. Fallback: Download from GitHub (Lite version behavior)
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

            let _ = self.download_file(url, &uv_bin).await;
            uv_bin
        }).await;

        let uv_bin = self.get_infra_bin_dir().join(if cfg!(windows) { "uv.exe" } else { "uv" });
        // Final verification: check infra/bin OR bundled OR system
        if uv_bin.exists() { return Ok(uv_bin); }
        if let Some(bundled) = self.get_bundled_bin_dir().map(|d| d.join(if cfg!(windows) { "uv.exe" } else { "uv" })) {
            if bundled.exists() { return Ok(bundled); }
        }
        which::which("uv").map_err(|_| Error::Internal("UV failed to initialize".into()))
    }

    /// Ensure `pixi` is available, downloading it if necessary.
    pub async fn ensure_pixi(&self) -> Result<PathBuf> {
        self.pixi_lock.get_or_init(|| async {
            // 1. Check bundled
            if let Some(bundled_dir) = self.get_bundled_bin_dir() {
                let pixi_bundled = bundled_dir.join(if cfg!(windows) { "pixi.exe" } else { "pixi" });
                if pixi_bundled.exists() { return pixi_bundled; }
            }

            // 2. Check system
            if let Ok(bin) = which::which("pixi") {
                return bin;
            }

            // 3. Check provisioned
            let bin_dir = self.ensure_writable_dir(&self.get_infra_bin_dir()).await.unwrap_or_else(|_| self.get_infra_bin_dir());
            let pixi_bin = bin_dir.join(if cfg!(windows) { "pixi.exe" } else { "pixi" });
            if pixi_bin.exists() {
                return pixi_bin;
            }

            // 4. Download
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

            let _ = self.download_file(url, &pixi_bin).await;
            
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&pixi_bin) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&pixi_bin, perms);
                }
            }
            pixi_bin
        }).await;

        let pixi_bin = self.get_infra_bin_dir().join(if cfg!(windows) { "pixi.exe" } else { "pixi" });
        if pixi_bin.exists() { Ok(pixi_bin) } else {
             if let Some(bundled) = self.get_bundled_bin_dir().map(|d| d.join(if cfg!(windows) { "pixi.exe" } else { "pixi" })) {
                if bundled.exists() { return Ok(bundled); }
            }
            which::which("pixi").map_err(|_| Error::Internal("Pixi failed to initialize".into()))
        }
    }

    /// Ensure `bun` is available.
    pub async fn ensure_bun(&self) -> Result<PathBuf> {
        self.bun_lock.get_or_init(|| async {
            // 1. Check bundled
            if let Some(bundled_dir) = self.get_bundled_bin_dir() {
                let bun_bundled = bundled_dir.join(if cfg!(windows) { "bun.exe" } else { "bun" });
                if bun_bundled.exists() { return bun_bundled; }
            }

            // 2. Check provisioned
            let bin_dir = self.ensure_writable_dir(&self.get_infra_bin_dir()).await.unwrap_or_else(|_| self.get_infra_bin_dir());
            let bun_name = if cfg!(windows) { "bun.exe" } else { "bun" };
            let bun_bin = bin_dir.join(bun_name);
            if bun_bin.exists() { return bun_bin; }

            // 3. Check system
            if let Ok(path) = which::which("bun") { return path; }

            // 4. Download
            info!("bun not found. Downloading portable bun...");
            let url = if cfg!(target_os = "windows") {
                "https://github.com/oven-sh/bun/releases/download/bun-v1.2.4/bun-windows-x64.zip"
            } else if cfg!(target_os = "macos") {
                if cfg!(target_arch = "aarch64") {
                    "https://github.com/oven-sh/bun/releases/download/bun-v1.2.4/bun-darwin-aarch64.zip"
                } else {
                    "https://github.com/oven-sh/bun/releases/download/bun-v1.2.4/bun-darwin-x64.zip"
                }
            } else {
                "https://github.com/oven-sh/bun/releases/download/bun-v1.2.4/bun-linux-x64.zip"
            };

            let zip_path = bin_dir.join("bun.zip");
            let _ = self.download_file(url, &zip_path).await;
            let _ = self.extract_and_cleanup(&zip_path, &bin_dir).await;

            bun_bin
        }).await;

        let bun_bin = self.get_infra_bin_dir().join(if cfg!(windows) { "bun.exe" } else { "bun" });
        if bun_bin.exists() { Ok(bun_bin) } else {
            if let Some(bundled) = self.get_bundled_bin_dir().map(|d| d.join(if cfg!(windows) { "bun.exe" } else { "bun" })) {
                if bundled.exists() { return Ok(bundled); }
            }
            which::which("bun").map_err(|_| Error::Internal("Bun failed to initialize".into()))
        }
    }

    /// Ensure `git` and `mini-bash` are available (Windows focused).
    pub async fn ensure_git(&self) -> Result<PathBuf> {
        self.git_lock.get_or_init(|| async {
            // 1. Check bundled (Offline-First)
            if let Some(bundled_dir) = self.get_bundled_bin_dir() {
                #[cfg(target_os = "windows")]
                let git_bundled = bundled_dir.join("git-bash").join("bin").join("git.exe");
                #[cfg(not(target_os = "windows"))]
                let git_bundled = bundled_dir.join("git");
                
                if git_bundled.exists() { return git_bundled; }
            }

            #[cfg(target_os = "windows")]
            {
                let bin_dir = self.ensure_writable_dir(&self.get_infra_bin_dir()).await.unwrap_or_else(|_| self.get_infra_bin_dir());
                let git_bin = bin_dir.join("git-bash").join("bin").join("git.exe");
                if git_bin.exists() { return git_bin; }
                if let Ok(path) = which::which("git") { return path; }

                info!("Portable Git (MinGit) not found. Downloading 20MB thumb version...");
                let url = "https://github.com/git-for-windows/git/releases/download/v2.53.0.windows.1/MinGit-2.53.0-64-bit.zip";
                let zip_path = bin_dir.join("mingit.zip");
                let extract_to = bin_dir.join("git-bash");
                
                let _ = self.download_file(url, &zip_path).await;
                let _ = self.extract_and_cleanup(&zip_path, &extract_to).await;
                
                git_bin
            }

            #[cfg(not(target_os = "windows"))]
            {
                 which::which("git").unwrap_or_else(|_| PathBuf::from("/usr/bin/git"))
            }
        }).await;

        #[cfg(target_os = "windows")]
        {
            let git_bin = self.get_infra_bin_dir().join("git-bash").join("bin").join("git.exe");
            if git_bin.exists() { return Ok(git_bin); }
             if let Some(bundled) = self.get_bundled_bin_dir().map(|d| d.join("git-bash").join("bin").join("git.exe")) {
                if bundled.exists() { return Ok(bundled); }
            }
            which::which("git").map_err(|_| Error::Internal("Git failed to initialize".into()))
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(path) = which::which("git") { Ok(path) } else { Err(Error::Internal("Git not found".into())) }
        }
    }

    /// Ensure `gcc` (MinGW) is available on Windows.
    pub async fn ensure_gcc(&self) -> Result<PathBuf> {
        self.gcc_lock.get_or_init(|| async {
            #[cfg(target_os = "windows")]
            {
                let bin_dir = self.ensure_writable_dir(&self.get_infra_bin_dir()).await.unwrap_or_else(|_| self.get_infra_bin_dir());
                let gcc_bin = bin_dir.join("mingw").join("bin").join("gcc.exe");
                if gcc_bin.exists() { return gcc_bin; }
                if let Ok(path) = which::which("gcc") { return path; }

                info!("Portable GCC (w64devkit) not found. Downloading lightweight toolchain...");
                // Version locked to 1.21.0 for stability
                let url = "https://github.com/skeeto/w64devkit/releases/download/v1.21.0/w64devkit-1.21.0.zip";
                let zip_path = bin_dir.join("mingw.zip");
                let extract_to = bin_dir.join("mingw_temp");
                
                let _ = self.download_file(url, &zip_path).await;
                let _ = self.extract_and_cleanup(&zip_path, &extract_to).await;
                
                // Move logic for subfolders
                let actual_dir = extract_to.join("w64devkit");
                if actual_dir.exists() {
                    let _ = tokio::fs::rename(actual_dir, bin_dir.join("mingw")).await;
                    let _ = tokio::fs::remove_dir_all(extract_to).await;
                } else {
                     let _ = tokio::fs::rename(extract_to, bin_dir.join("mingw")).await;
                }
                
                gcc_bin
            }

            #[cfg(not(target_os = "windows"))]
            {
                which::which("gcc").unwrap_or_else(|_| PathBuf::from("/usr/bin/gcc"))
            }
        }).await;

        #[cfg(target_os = "windows")]
        {
            let gcc_bin = self.get_infra_bin_dir().join("mingw").join("bin").join("gcc.exe");
            if gcc_bin.exists() { Ok(gcc_bin) } else { Err(Error::Internal("GCC failed to initialize".into())) }
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(path) = which::which("gcc") { Ok(path) } else { Err(Error::Internal("GCC not found".into())) }
        }
    }

    /// Internal helper to extract zip files using PowerShell (Windows) or unzip (Unix)
    /// to avoid adding extra Rust dependencies for binary size.
    async fn extract_and_cleanup(&self, zip_path: &Path, dest: &Path) -> Result<()> {
        if !dest.exists() {
            tokio::fs::create_dir_all(dest).await?;
        }

        info!("Extracting {} to {}...", zip_path.display(), dest.display());

        let status = if cfg!(target_os = "windows") {
            // Enhanced PowerShell command with WindowStyle Hidden and explicit error checking
            Command::new("powershell")
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-WindowStyle").arg("Hidden")
                .arg("-Command")
                .arg(format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force; if (!$?) {{ exit 1 }}",
                    zip_path.display(),
                    dest.display()
                ))
                .status()
                .await
        } else {
            Command::new("unzip")
                .arg("-o")
                .arg(zip_path)
                .arg("-d")
                .arg(dest)
                .status()
                .await
        };

        match status {
            Ok(s) if s.success() => {
                let _ = tokio::fs::remove_file(zip_path).await;
                Ok(())
            },
            _ => Err(Error::Internal(format!("Failed to extract {}", zip_path.display()))),
        }
    }
}
