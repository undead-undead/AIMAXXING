use crate::error::{Error, Result};
use crate::skills::runtime::SkillRuntime;
use crate::skills::SkillExecutionConfig;
use async_trait::async_trait;
use std::path::Path;
use std::process;
use tracing::debug;

/// In-process QuickJS runtime. Executes JavaScript directly inside the
/// Rust process via the `rquickjs` crate — no Node.js installation required.
///
/// Activated when a SKILL.md declares `runtime: js` or `runtime: javascript`.
#[derive(Clone)]
pub struct QuickJSRuntime;

impl QuickJSRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl Default for QuickJSRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillRuntime for QuickJSRuntime {
    async fn execute(
        &self,
        metadata: &crate::skills::SkillMetadata,
        arguments: &str,
        base_dir: &Path,
        config: &SkillExecutionConfig,
        _env_manager: Option<&std::sync::Arc<crate::env::EnvManager>>,
    ) -> Result<std::process::Output> {
        let script_file = metadata.script.as_ref().ok_or_else(|| {
            Error::ToolExecution {
                tool_name: metadata.name.clone(),
                message: "No script defined for this skill".to_string(),
            }
        })?;
        let script_path = base_dir.join("scripts").join(script_file);

        let script_code = tokio::fs::read_to_string(&script_path)
            .await
            .map_err(|e| Error::ToolExecution {
                tool_name: "QuickJSRuntime".into(),
                message: format!("Failed to read script {:?}: {}", script_path, e),
            })?;

        let args_owned = arguments.to_string();
        let timeout_secs = config.timeout_secs;

        #[cfg(feature = "quickjs")]
        {
            let result = tokio::task::spawn_blocking(move || {
                execute_js_blocking(&script_code, &args_owned, timeout_secs)
            })
            .await
            .map_err(|e| Error::ToolExecution {
                tool_name: "QuickJSRuntime".into(),
                message: format!("JS thread panicked: {}", e),
            })??;

            Ok(result)
        }

        #[cfg(not(feature = "quickjs"))]
        {
            let _ = (script_code, args_owned, timeout_secs);
            Err(Error::ToolExecution {
                tool_name: "QuickJSRuntime".into(),
                message: "QuickJS runtime is not compiled in. Enable the 'quickjs' feature.".to_string(),
            })
        }
    }
}

/// Synchronous JS execution inside QuickJS.
/// Sets up a context, injects `SKILL_ARGS` as a global JSON string, runs the script.
#[cfg(feature = "quickjs")]
fn execute_js_blocking(
    code: &str,
    args: &str,
    _timeout_secs: u64,
) -> Result<process::Output> {
    use rquickjs::{Context, Runtime};

    let rt = Runtime::new().map_err(|e| Error::ToolExecution {
        tool_name: "QuickJSRuntime".into(),
        message: format!("Failed to create QuickJS runtime: {}", e),
    })?;

    // Set memory limit (16MB per execution) to prevent DoS
    rt.set_memory_limit(16 * 1024 * 1024);
    // Max stack depth
    rt.set_max_stack_size(1024 * 1024);

    let ctx = Context::full(&rt).map_err(|e| Error::ToolExecution {
        tool_name: "QuickJSRuntime".into(),
        message: format!("Failed to create QuickJS context: {}", e),
    })?;

    let output = ctx.with(|ctx| -> std::result::Result<(String, String, bool), rquickjs::Error> {
        let wrapper = format!(
            r#"
(function() {{
    var __stdout = [];
    var __stderr = [];
    var console = {{
        log: function() {{ __stdout.push(Array.prototype.slice.call(arguments).join(' ')); }},
        error: function() {{ __stderr.push(Array.prototype.slice.call(arguments).join(' ')); }},
        warn: function() {{ __stderr.push('[WARN] ' + Array.prototype.slice.call(arguments).join(' ')); }},
    }};
    var SKILL_ARGS = {args_json};
    try {{
        {script}
    }} catch(e) {{
        __stderr.push('Error: ' + (e.stack || e.message || e));
        return JSON.stringify({{ stdout: __stdout.join('\n'), stderr: __stderr.join('\n'), ok: false }});
    }}
    return JSON.stringify({{ stdout: __stdout.join('\n'), stderr: __stderr.join('\n'), ok: true }});
}})()
"#,
            args_json = serde_json::to_string(args).unwrap_or_else(|_| format!("{:?}", args)),
            script = code,
        );

        let json_result: String = ctx.eval(wrapper.as_bytes())?;
        let parsed: serde_json::Value = serde_json::from_str(&json_result)
            .unwrap_or(serde_json::json!({"stdout": "", "stderr": "Internal JSON parse error of script result", "ok": false}));

        let stdout = parsed["stdout"].as_str().unwrap_or("").to_string();
        let stderr = parsed["stderr"].as_str().unwrap_or("").to_string();
        let ok = parsed["ok"].as_bool().unwrap_or(false);

        Ok((stdout, stderr, ok))
    });

    match output {
        Ok((stdout, stderr, ok)) => {
            debug!(
                stdout_len = stdout.len(),
                stderr_len = stderr.len(),
                ok = ok,
                "QuickJS execution complete"
            );
            let exit_code = if ok { 0i32 } else { 1i32 };
            Ok(process::Output {
                status: {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        process::ExitStatus::from_raw(exit_code << 8)
                    }
                    #[cfg(not(unix))]
                    {
                        std::process::Command::new(if ok { "true" } else { "false" })
                            .status()
                            .unwrap_or_else(|_| {
                                std::process::Command::new("cmd")
                                    .args(["/C", if ok { "exit 0" } else { "exit 1" }])
                                    .status()
                                    .expect("Failed to get exit status")
                            })
                    }
                },
                stdout: stdout.into_bytes(),
                stderr: stderr.into_bytes(),
            })
        }
        Err(e) => Err(Error::ToolExecution {
            tool_name: "QuickJSRuntime".into(),
            message: format!("QuickJS execution error: {}", e),
        }),
    }
}
