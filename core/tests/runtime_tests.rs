/// Tests for the QuickJSRuntime (Phase 3) and MicroPythonRuntime discovery (Phase 4).
/// Run with: cargo test --test runtime_tests
use brain::skills::{SkillMetadata, SkillExecutionConfig};
use brain::skills::runtime::{QuickJSRuntime, MicroPythonRuntime, SkillRuntime};
use std::path::Path;
use tempfile::TempDir;

fn default_metadata(name: &str, script: &str, runtime: &str) -> SkillMetadata {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "description": "Test skill",
        "runtime": runtime,
        "script": script,
    })).unwrap()
}

fn default_config() -> SkillExecutionConfig {
    SkillExecutionConfig {
        timeout_secs: 10,
        ..Default::default()
    }
}

// ── Phase 3: QuickJS ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_quickjs_hello_world() {
    let tmp = TempDir::new().unwrap();
    let scripts_dir = tmp.path().join("scripts");
    std::fs::create_dir(&scripts_dir).unwrap();
    let script_file = "skill.js";
    std::fs::write(scripts_dir.join(script_file), r#"console.log("Hello from QuickJS!");"#).unwrap();

    let meta = default_metadata("hello", script_file, "js");
    let rt = QuickJSRuntime::new();
    let output = rt
        .execute(&meta, "{}", tmp.path(), &default_config(), None)
        .await
        .expect("QuickJS execution should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Exit status should be success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Hello from QuickJS!"),
        "stdout should contain greeting, got: {}",
        stdout
    );
}

#[tokio::test]
async fn test_quickjs_skill_args_available() {
    let tmp = TempDir::new().unwrap();
    let scripts_dir = tmp.path().join("scripts");
    std::fs::create_dir(&scripts_dir).unwrap();
    let script_file = "args.js";
    std::fs::write(
        scripts_dir.join(script_file),
        r#"
var parsed = JSON.parse(SKILL_ARGS);
console.log("city=" + parsed.city);
"#,
    )
    .unwrap();

    let meta = default_metadata("args", script_file, "js");
    let rt = QuickJSRuntime::new();
    let output = rt
        .execute(
            &meta,
            r#"{"city":"Tokyo"}"#,
            tmp.path(),
            &default_config(),
            None,
        )
        .await
        .expect("QuickJS execution should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("city=Tokyo"),
        "Should parse SKILL_ARGS, got: {}",
        stdout
    );
}

#[tokio::test]
async fn test_quickjs_math_computation() {
    let tmp = TempDir::new().unwrap();
    let scripts_dir = tmp.path().join("scripts");
    std::fs::create_dir(&scripts_dir).unwrap();
    let script_file = "math.js";
    std::fs::write(
        scripts_dir.join(script_file),
        r#"
// Compute fibonacci without any external library
function fib(n) { return n <= 1 ? n : fib(n-1) + fib(n-2); }
console.log(fib(10));
"#,
    )
    .unwrap();

    let meta = default_metadata("math", script_file, "js");
    let rt = QuickJSRuntime::new();
    let output = rt
        .execute(&meta, "{}", tmp.path(), &default_config(), None)
        .await
        .expect("QuickJS math should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.trim() == "55",
        "fib(10) should be 55, got: {}",
        stdout.trim()
    );
}

#[tokio::test]
async fn test_quickjs_runtime_error_captured() {
    let tmp = TempDir::new().unwrap();
    let scripts_dir = tmp.path().join("scripts");
    std::fs::create_dir(&scripts_dir).unwrap();
    let script_file = "error.js";
    std::fs::write(scripts_dir.join(script_file), r#"throw new Error("intentional error");"#).unwrap();

    let meta = default_metadata("error", script_file, "js");
    let rt = QuickJSRuntime::new();
    let output = rt
        .execute(&meta, "{}", tmp.path(), &default_config(), None)
        .await
        .expect("QuickJS should return output even on script error");

    // Script-level errors: exit code should be non-zero
    assert!(
        !output.status.success(),
        "Script that throws should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error:"),
        "Stderr should contain the error, got: {}",
        stderr
    );
}

// ── Phase 4: MicroPython discovery ─────────────────────────────────────────

#[tokio::test]
async fn test_micropython_detects_system_python() {
    // This test only verifies that MicroPythonRuntime can actually RUN a simple
    // Python script using the system Python (if available).
    // If Python is not found AND uv is not installed, the test is skipped.
    let tmp = TempDir::new().unwrap();
    let scripts_dir = tmp.path().join("scripts");
    std::fs::create_dir(&scripts_dir).unwrap();
    let script_file = "hello.py";
    std::fs::write(scripts_dir.join(script_file), r#"print("MicroPython: Hello from Python!")"#).unwrap();

    std::env::set_var("AIMAXXING_UNSAFE_SKILL_EXEC", "true");

    let meta = default_metadata("hello-py", script_file, "python3");
    let rt = MicroPythonRuntime::with_skill_context("hello-py", vec![]);
    match rt
        .execute(
            &meta,
            "{}",
            tmp.path(),
            &default_config(),
            None,
        )
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "Python script should succeed, stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                stdout.contains("MicroPython: Hello from Python!"),
                "stdout should contain greeting, got: {}",
                stdout
            );
            println!("✅ Python found and working.");
        }
        Err(e) => {
            // If Python + uv are not available in the test environment, skip
            eprintln!("⚠️  Skipping Python test (Python/uv not available): {}", e);
        }
    }
}

// ── Phase 5: Native & Sandbox features ─────────────────────────────────────

#[tokio::test]
async fn test_native_sandbox_registration() {
    use brain::skills::sandbox::{NativeShellRuntime, ACTIVE_SANDBOXES};
    
    let tmp = TempDir::new().unwrap();
    let scripts_dir = tmp.path().join("scripts");
    std::fs::create_dir(&scripts_dir).unwrap();
    let script_file = "sleep.sh";
    
    #[cfg(not(target_os = "windows"))]
    let script_content = "#!/bin/bash\nsleep 2\necho done";
    #[cfg(target_os = "windows")]
    let script_content = "timeout /t 2 /nobreak > NUL\necho done";
    
    std::fs::write(scripts_dir.join(script_file), script_content).unwrap();
    
    std::env::set_var("AIMAXXING_UNSAFE_SKILL_EXEC", "true");
    
    #[cfg(not(target_os = "windows"))]
    let meta = default_metadata("sleep_skill", script_file, "bash");
    #[cfg(target_os = "windows")]
    let meta = default_metadata("sleep_skill", script_file, "cmd");
    
    let rt = NativeShellRuntime::new();
    let tmp_path = tmp.path().to_path_buf();
    
    let handle = tokio::spawn(async move {
        let config = default_config();
        rt.execute(&meta, "{}", &tmp_path, &config, None).await
    });
    
    // Wait for the process to spawn and register its PID
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let found = ACTIVE_SANDBOXES.iter().any(|entry| entry.value().tool_name == "sleep_skill");
    assert!(found, "The running sandbox should be registered in ACTIVE_SANDBOXES");
    
    // Wait for execution to finish
    let output = handle.await.expect("task panicked").expect("execution failed");
    assert!(output.status.success(), "Skill execution should succeed");
    
    let found_after = ACTIVE_SANDBOXES.iter().any(|entry| entry.value().tool_name == "sleep_skill");
    assert!(!found_after, "The sandbox should be unregistered from ACTIVE_SANDBOXES after completion");
}
