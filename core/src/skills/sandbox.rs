use crate::error::{Error, Result};
use crate::security::{LeakDetector, ShellFirewall};
use crate::skills::runtime::SkillRuntime;
use crate::skills::SkillExecutionConfig;
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::time::SystemTime;

pub static GLOBAL_DETECTOR: Lazy<crate::security::LeakDetector> = Lazy::new(|| crate::security::LeakDetector::new());

#[derive(Debug, Clone, Serialize)]
pub struct ActiveSandboxContext {
    pub pid: u32,
    pub tool_name: String,
    pub interpreter: String,
    pub started_at: SystemTime,
}

pub static ACTIVE_SANDBOXES: Lazy<DashMap<u32, ActiveSandboxContext>> = Lazy::new(DashMap::new);

#[cfg(target_os = "windows")]
mod windows_sandbox {
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::Foundation::*;
    use std::ptr::null;
    use std::mem::size_of;

    pub struct JobObject(HANDLE);

    impl JobObject {
        pub fn create(config: &crate::skills::SkillExecutionConfig) -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(null(), null());
                if handle == 0 { return None; }
                
                // 1. Set standard limits: Absolute kill on close, and optional memory limits
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = 
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | 
                    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
                
                if let Some(mem_mb) = config.max_memory_mb {
                    info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
                    info.JobMemoryLimit = mem_mb as usize * 1024 * 1024;
                }

                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );

                // 2. Set UI restrictions: No clipboard access, no desktop switching
                let mut ui_info: JOBOBJECT_BASIC_UI_RESTRICTIONS = std::mem::zeroed();
                ui_info.UIRestrictionsClass = 
                    JOB_OBJECT_UILIMIT_READCLIPBOARD | 
                    JOB_OBJECT_UILIMIT_WRITECLIPBOARD |
                    JOB_OBJECT_UILIMIT_HANDLES |
                    JOB_OBJECT_UILIMIT_GLOBALATOMS |
                    JOB_OBJECT_UILIMIT_EXITWINDOWS |
                    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS;

                SetInformationJobObject(
                    handle,
                    JobObjectBasicUIRestrictions,
                    &ui_info as *const _ as *const _,
                    size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
                );
                
                Some(Self(handle))
            }
        }

        pub fn assign(&self, process_handle: HANDLE) -> bool {
            unsafe { AssignProcessToJobObject(self.0, process_handle) != 0 }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0); }
        }
    }
}

pub struct NativeShellRuntime;

impl NativeShellRuntime {
    pub fn new() -> Self {
        Self
    }

    /// Layer 1 (Application): AIMAXXING shell firewall.
    /// Runs BEFORE the OS sandbox — rejects obviously dangerous commands.
    fn pre_flight_firewall(arguments: &str, interpreter: &str) -> Result<()> {
        // Build a combined string for checking (interpreter + args together)
        let combined = format!("{} {}", interpreter, arguments);
        ShellFirewall::enforce(&combined).map_err(|reason| {
            warn!(
                interpreter = %interpreter,
                reason = %reason,
                "Pre-flight firewall blocked execution"
            );
            Error::ToolExecution {
                tool_name: "NativeShellRuntime::Firewall".to_string(),
                message: reason,
            }
        })
    }

    /// Layer 1b: Secret-in-args guard.
    ///
    /// CLI arguments are visible to all users via `ps aux`. Secrets (API keys,
    /// tokens, private keys) must NEVER be passed as command-line arguments.
    /// They must always be injected via environment variables through the Vault.
    fn check_args_for_secrets(arguments: &str) -> Result<()> {
        let (_redacted, detections) = GLOBAL_DETECTOR.redact(arguments);

        use crate::security::leaks::LeakAction;
        let hard_violations: Vec<_> = detections
            .iter()
            .filter(|d| matches!(d.action, LeakAction::Redact | LeakAction::Block))
            .collect();

        if !hard_violations.is_empty() {
            let names: Vec<&str> = hard_violations.iter().map(|d| d.pattern_name.as_str()).collect();
            warn!(
                patterns = ?names,
                "SECURITY: Secret detected in skill CLI arguments — execution blocked. \
                 Use Vault env injection instead."
            );
            return Err(Error::ToolExecution {
                tool_name: "NativeShellRuntime::SecretGuard".to_string(),
                message: format!(
                    "Secrets must not be passed as CLI arguments (detected: {}). \
                     Inject secrets via environment variables through the Vault.",
                    names.join(", ")
                ),
            });
        }

        Ok(())
    }


    /// Layer 3 (Output): Secret leak scanner.
    /// Strips API keys / tokens from stdout/stderr before returning to caller.
    fn sanitize_output(stdout: Vec<u8>, stderr: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
        let stdout_str = String::from_utf8_lossy(&stdout);
        let (clean_stdout, stdout_detections) = GLOBAL_DETECTOR.redact(&stdout_str);

        let stderr_str = String::from_utf8_lossy(&stderr);
        let (clean_stderr, stderr_detections) = GLOBAL_DETECTOR.redact(&stderr_str);

        let total = stdout_detections.len() + stderr_detections.len();
        if total > 0 {
            warn!(
                count = total,
                "Secret leak scanner redacted {} potential secret(s) from skill output",
                total
            );
        }

        (clean_stdout.into_bytes(), clean_stderr.into_bytes())
    }

    /// Layer 2 (Kernel): Build the OS-native sandboxed subprocess command.
    fn build_os_sandboxed_command(
        &self,
        interpreter: &str,
        script_path: &Path,
        base_dir: &Path,
        arguments: &str,
        config: &SkillExecutionConfig,
    ) -> Command {
        #[cfg(target_os = "linux")]
        {
            let unsafe_override = std::env::var("AIMAXXING_UNSAFE_SKILL_EXEC")
                .map(|v| v == "true")
                .unwrap_or(false);

            if unsafe_override {
                warn!("UNSAFE EXECUTION: bwrap bypassed via AIMAXXING_UNSAFE_SKILL_EXEC.");
                let mut c = Command::new(interpreter);
                c.arg(script_path).arg(arguments);
                return c;
            }

            // bwrap: read-only root, writable /tmp, network isolated unless explicitly allowed
            // As per Phase 4.2: Mount the base_dir (workspace) as Read-Write.
            let mut c = Command::new("bwrap");
            c.arg("--ro-bind").arg("/").arg("/");
            c.arg("--dev").arg("/dev");
            c.arg("--proc").arg("/proc");
            c.arg("--tmpfs").arg("/tmp");
            
            // Expose the workspace (base_dir) as Read-Write
            let abs_base = std::fs::canonicalize(base_dir).unwrap_or_else(|_| base_dir.to_path_buf());
            c.arg("--bind").arg(&abs_base).arg(&abs_base);
            
            if !config.allow_network {
                c.arg("--unshare-net");
            }
            // Die with parent to prevent zombie sandboxes
            c.arg("--die-with-parent");
            c.arg("--").arg(interpreter).arg(script_path).arg(arguments);
            c
        }

        #[cfg(target_os = "macos")]
        {
            let unsafe_override = std::env::var("AIMAXXING_UNSAFE_SKILL_EXEC")
                .map(|v| v == "true")
                .unwrap_or(false);

            if unsafe_override {
                warn!("UNSAFE EXECUTION: sandbox-exec bypassed via AIMAXXING_UNSAFE_SKILL_EXEC.");
                let mut c = Command::new(interpreter);
                c.arg(script_path).arg(arguments);
                return c;
            }

            // macOS Seatbelt: deny network if not allowed, deny raw filesystem writes
            // As per Phase 4.2: We should ideally allow write ONLY to base_dir.
            let abs_base = std::fs::canonicalize(base_dir).unwrap_or_else(|_| base_dir.to_path_buf());
            let base_str = abs_base.to_string_lossy();
            
            let network_policy = if config.allow_network {
                "(allow network*)"
            } else {
                "(deny network*)"
            };

            let profile = format!(
                r#"(version 1)
                   (allow default)
                   {network_policy}
                   (deny file-write*)
                   (allow file-write* (subpath "{base_str}"))
                   (allow file-write* (subpath "/tmp"))"#,
                network_policy = network_policy,
                base_str = base_str
            );

            let mut c = Command::new("sandbox-exec");
            c.arg("-p").arg(profile);
            c.arg(interpreter).arg(script_path).arg(arguments);
            c
        }

        #[cfg(target_os = "windows")]
        {
            // Windows Job Object limit construction
            let mut c = Command::new(interpreter);
            c.arg(script_path).arg(arguments);
            c
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            warn!("Unsupported OS. Running without OS sandboxing (firewall only).");
            let mut c = Command::new(interpreter);
            c.arg(script_path).arg(arguments);
            c
        }
    }

    fn inject_env_vars(
        config: &SkillExecutionConfig,
        cmd: &mut Command,
        env_prefix: Option<&Path>,
        models_path: Option<&Path>,
    ) {
        let mut final_env = config.env_vars.clone();

        if let Some(prefix) = env_prefix {
            let bin = prefix.join("bin").to_string_lossy().to_string();
            let old_path = std::env::var("PATH").unwrap_or_default();
            final_env.insert("PATH".to_string(), format!("{}:{}", bin, old_path));
            final_env.insert("CONDA_PREFIX".to_string(), prefix.to_string_lossy().to_string());
        }

        if let Some(mp) = models_path {
            final_env.insert("AIMAXXING_MODELS_PATH".to_string(), mp.to_string_lossy().to_string());
        }

        for (key, value) in &final_env {
            cmd.env(key, value);
        }
    }
}

#[async_trait]
impl SkillRuntime for NativeShellRuntime {
    async fn execute(
        &self,
        metadata: &crate::skills::SkillMetadata,
        arguments: &str,
        base_dir: &Path,
        config: &SkillExecutionConfig,
        env_manager: Option<&std::sync::Arc<crate::env::EnvManager>>,
    ) -> Result<std::process::Output> {
        let interpreter = metadata.runtime.as_deref().unwrap_or("bash");
        
        // Resolve script path
        let script_file = metadata.script.as_ref().ok_or_else(|| {
            Error::ToolExecution {
                tool_name: metadata.name.clone(),
                message: "No script defined for this skill".to_string(),
            }
        })?;
        let script_path = base_dir.join("scripts").join(script_file);

        // ─── Layer 0: Environment Provisioning (Pixi) ─────────────────────
        let mut env_prefix = None;
        let mut models_path = None;
        if let Some(em) = env_manager {
            if !metadata.dependencies.is_empty() || metadata.use_browser {
                env_prefix = Some(em.provision(&metadata.name, &metadata.dependencies, metadata.use_browser).await?);
            }
            if !metadata.models.is_empty() {
                models_path = Some(em.provision_models(&metadata.name, &metadata.models).await?);
            }
        }

        // ─── Layer 1: Application firewall (AIMAXXING) ───────────────────────
        Self::pre_flight_firewall(arguments, interpreter)?;

        // ─── Layer 1b: Secret-in-args guard ──────────────────────────────────
        Self::check_args_for_secrets(arguments)?;

        // ─── Layer 2: Kernel sandbox ─────────────────────────────────────────
        let mut cmd = self.build_os_sandboxed_command(interpreter, &script_path, base_dir, arguments, config);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.kill_on_drop(true);
        Self::inject_env_vars(config, &mut cmd, env_prefix.as_deref(), models_path.as_deref());

        debug!(
            interpreter = %interpreter,
            script = ?script_path,
            "NativeShellRuntime: spawning sandboxed process"
        );

        let mut child = cmd.spawn().map_err(|e| Error::ToolExecution {
            tool_name: "NativeShellRuntime".to_string(),
            message: format!("Failed to spawn process: {}", e),
        })?;

        #[cfg(target_os = "windows")]
        {
            if let Some(job) = windows_sandbox::JobObject::create(config) {
                use std::os::windows::io::AsRawHandle;
                let handle = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
                if !job.assign(handle) {
                    warn!("Failed to assign process to Job Object sandbox. Running with partial isolation.");
                } else {
                    debug!("Process successfully locked in Windows Job Object with UI and resource limits.");
                }
                // The job will be kept alive by the handle inside the struct until it drops
                // But since we want to keep it alive for the child's lifetime:
                let _keep_alive = job; 
            }
        }

        let child_id = child.id();
        let pid = child_id.unwrap_or(0);
        
        if pid > 0 {
            ACTIVE_SANDBOXES.insert(pid, ActiveSandboxContext {
                pid,
                tool_name: metadata.name.clone(),
                interpreter: interpreter.to_string(),
                started_at: SystemTime::now(),
            });
        }

        let timeout_duration = std::time::Duration::from_secs(config.timeout_secs);
        let wait_res = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;
        
        // Ensure cleanup and killing on failure/timeout
        if pid > 0 {
            ACTIVE_SANDBOXES.remove(&pid);
        }

        let raw_output = match wait_res {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(Error::ToolExecution {
                    tool_name: "NativeShellRuntime".to_string(),
                    message: format!("Process IO error: {}", e),
                });
            }
            Err(_) => {
                return Err(Error::ToolExecution {
                    tool_name: "NativeShellRuntime".to_string(),
                    message: format!("Execution timed out after {}s", config.timeout_secs),
                });
            }
        };

        // ─── Layer 3: Secret leak sanitizer ──────────────────────────────────
        let (clean_stdout, clean_stderr) =
            Self::sanitize_output(raw_output.stdout, raw_output.stderr);

        Ok(std::process::Output {
            status: raw_output.status,
            stdout: clean_stdout,
            stderr: clean_stderr,
        })
    }
}
