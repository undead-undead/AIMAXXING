use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info, warn};
use brain::error::{Error, Result};

/// Returns the platform-specific Python binary name.
pub fn python_bin_name() -> &'static str {
    if cfg!(windows) { "python.exe" } else { "python3" }
}

/// Returns the platform-specific venv binary directory name.
pub fn venv_bin_dir() -> &'static str {
    if cfg!(windows) { "Scripts" } else { "bin" }
}

/// The standard directory where aimaxxing caches runtimes.
/// e.g. ~/.aimaxxing/runtimes/python/
pub fn aimaxxing_python_cache_dir() -> PathBuf {
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
                .join("aimaxxing")
        });
    base.join("runtimes").join("python")
}

/// Per-skill venv cache directory.
/// e.g. ~/.aimaxxing/venvs/<skill_name>/
pub fn skill_venv_dir(skill_name: &str) -> PathBuf {
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
                .join("aimaxxing")
        });
    base.join("venvs").join(skill_name)
}

/// Attempts to find python binary.
/// Priority:
///  1. aimaxxing's own managed python (uv-provisioned)
///  2. System python3 / python
pub async fn find_python() -> Option<PathBuf> {
    // 1. Check aimaxxing managed python
    let managed = aimaxxing_python_cache_dir().join(venv_bin_dir()).join(python_bin_name());
    if managed.exists() {
        debug!("Found aimaxxing-managed Python at {:?}", managed);
        return Some(managed);
    }

    // 2. Check system python
    let bin = python_bin_name();
    let cmd = if cfg!(windows) { "where" } else { "which" };
    
    if let Ok(out) = Command::new(cmd).arg(bin).output().await {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let path = stdout.trim().lines().next().unwrap_or("").to_string();
            if !path.is_empty() {
                debug!("Found system Python at {}", path);
                return Some(PathBuf::from(path));
            }
        }
    }

    // Fallback for Windows if 'python3' was used but it's just 'python'
    if !cfg!(windows) && bin == "python3" {
         if let Ok(out) = Command::new("which").arg("python").output().await {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }

    None
}

/// Check if `uv` is available on the system.
pub async fn is_uv_available() -> bool {
    if which::which("uv").is_ok() {
        return true;
    }
    
    // Check locally managed bin
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                .join("aimaxxing")
        });
    let managed = base.join("bin").join(if cfg!(windows) { "uv.exe" } else { "uv" });
    
    managed.exists()
}

/// Silently provision a standalone Python using `uv`.
/// This downloads and installs CPython into ~/.aimaxxing/runtimes/python/
pub async fn provision_python_via_uv() -> Result<PathBuf> {
    let cache_dir = aimaxxing_python_cache_dir();

    // Ensure uv is available
    if !is_uv_available().await {
        // Try to install uv first via its official installer
        info!("uv not found. Attempting to install uv via official installer...");
        let (cmd, arg) = if cfg!(windows) {
            ("powershell", "-ExecutionPolicy ByPass -c \"irm https://astral.sh/uv/install.ps1 | iex\"")
        } else {
            ("sh", "-c \"curl -LsSf https://astral.sh/uv/install.sh | sh\"")
        };

        let install_result = Command::new(cmd)
            .arg(arg)
            .output()
            .await;

        match install_result {
            Ok(out) if out.status.success() => {
                info!("uv installed successfully.");
            }
            _ => {
                return Err(Error::ToolExecution {
                    tool_name: "PythonUtils".into(),
                    message: "Python is not available and uv install failed. \
                              Please install Python (https://python.org) or uv (https://github.com/astral-sh/uv) manually."
                        .to_string(),
                });
            }
        }
    }

    info!(
        "Provisioning standalone Python via uv into {:?}...",
        cache_dir
    );

    let output = Command::new("uv")
        .arg("python")
        .arg("install")
        .arg("3.11")
        .arg("--install-dir")
        .arg(&cache_dir)
        .output()
        .await
        .map_err(|e| Error::ToolExecution {
            tool_name: "PythonUtils".into(),
            message: format!("Failed to run uv: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::ToolExecution {
            tool_name: "PythonUtils".into(),
            message: format!("uv python install failed: {}", stderr),
        });
    }

    find_python().await.ok_or_else(|| Error::ToolExecution {
        tool_name: "PythonUtils".into(),
        message: "uv provisioning succeeded but python binary not found.".to_string(),
    })
}

/// Ensure skill dependencies are installed in an isolated per-skill venv.
pub async fn ensure_venv(
    python_bin: &Path,
    venv_name: &str,
    dependencies: &[String],
) -> Result<PathBuf> {
    if dependencies.is_empty() {
        return Ok(python_bin.to_path_buf());
    }

    let venv_dir = skill_venv_dir(venv_name);
    let venv_python = venv_dir.join(venv_bin_dir()).join(python_bin_name());

    if !venv_dir.exists() {
        info!(name = %venv_name, venv = ?venv_dir, "Creating isolated venv");

        if is_uv_available().await {
            let out = Command::new("uv")
                .arg("venv")
                .arg(&venv_dir)
                .arg("--python")
                .arg(python_bin)
                .output()
                .await?;

            if !out.status.success() {
                return Err(Error::ToolExecution {
                    tool_name: "PythonUtils".into(),
                    message: format!("uv venv failed: {}", String::from_utf8_lossy(&out.stderr)),
                });
            }
        } else {
            let out = Command::new(python_bin)
                .arg("-m")
                .arg("venv")
                .arg(&venv_dir)
                .output()
                .await?;

            if !out.status.success() {
                return Err(Error::ToolExecution {
                    tool_name: "PythonUtils".into(),
                    message: format!("venv creation failed: {}", String::from_utf8_lossy(&out.stderr)),
                });
            }
        }
    }

    // Install dependencies
    if !dependencies.is_empty() {
        if is_uv_available().await {
            let out = Command::new("uv")
                .arg("pip")
                .arg("install")
                .arg("--python")
                .arg(&venv_python)
                .args(dependencies)
                .output()
                .await?;

            if !out.status.success() {
                return Err(Error::ToolExecution {
                    tool_name: "PythonUtils".into(),
                    message: format!("uv pip install failed: {}", String::from_utf8_lossy(&out.stderr)),
                });
            }
        } else {
            let pip_bin = venv_dir.join(venv_bin_dir()).join(if cfg!(windows) { "pip.exe" } else { "pip" });
            let out = Command::new(&pip_bin)
                .arg("install")
                .args(dependencies)
                .output()
                .await?;

            if !out.status.success() {
                return Err(Error::ToolExecution {
                    tool_name: "PythonUtils".into(),
                    message: format!("pip install failed: {}", String::from_utf8_lossy(&out.stderr)),
                });
            }
        }
    }

    Ok(venv_python)
}
