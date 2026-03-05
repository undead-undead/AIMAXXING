//! Hook Discovery
//!
//! Auto-discovers hook scripts from a directory.
//! Convention: `hooks/{timing}/{name}.sh`
//! e.g., `hooks/before_llm/content-filter.sh`

use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

use super::engine::{Hook, HookTiming, ShellHook};

/// Discovers and loads hook scripts from a directory.
///
/// Expected directory structure:
/// ```text
/// hooks/
///   before_llm/
///     filter.sh
///   after_llm/
///     log.sh
///   before_tool_call/
///     safety.sh
///   on_error/
///     notify.sh
/// ```
pub fn discover_hooks(hooks_dir: &Path) -> Vec<Arc<dyn Hook>> {
    let mut hooks = Vec::new();

    if !hooks_dir.is_dir() {
        info!(dir = ?hooks_dir, "No hooks directory found, skipping discovery");
        return hooks;
    }

    let timing_dirs = [
        ("before_llm", HookTiming::BeforeLlm),
        ("after_llm", HookTiming::AfterLlm),
        ("before_tool_call", HookTiming::BeforeToolCall),
        ("after_tool_call", HookTiming::AfterToolCall),
        ("on_error", HookTiming::OnError),
        ("before_response", HookTiming::BeforeResponse),
    ];

    for (dir_name, timing) in timing_dirs {
        let dir = hooks_dir.join(dir_name);
        if !dir.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(dir = ?dir, error = %e, "Failed to read hook directory");
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Only .sh files
            if path.extension().and_then(|e| e.to_str()) != Some("sh") {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let command = path.to_string_lossy().to_string();

            info!(
                name = %name,
                timing = %timing,
                path = %command,
                "Discovered hook script"
            );

            let hook = ShellHook::new(
                format!("{}:{}", dir_name, name),
                format!("bash {}", command),
                vec![timing],
            );

            hooks.push(Arc::new(hook) as Arc<dyn Hook>);
        }
    }

    info!(count = hooks.len(), "Hooks discovery complete");
    hooks
}
