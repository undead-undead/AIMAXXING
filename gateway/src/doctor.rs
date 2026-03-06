use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Command;

pub async fn run_doctor() -> anyhow::Result<()> {
    println!("{}", "Running AIMAXXING Doctor...".bold().blue());
    println!("{}", "Checking system requirements and configuration...".dimmed());

    let mut checks = 0;
    let mut failures = 0;

    let steps: Vec<(&str, fn() -> anyhow::Result<()>)> = vec![
        ("Native Sandbox", check_sandbox as fn() -> anyhow::Result<()>),
        ("Vector DB Path", check_vectordb as fn() -> anyhow::Result<()>),
        ("RAG Engine Mode", check_rag_mode as fn() -> anyhow::Result<()>),
        ("Pixi Environment", check_pixi as fn() -> anyhow::Result<()>),
        ("JS Runtime (Node/Bun)", check_node as fn() -> anyhow::Result<()>),
        ("Smithery ENV", check_env as fn() -> anyhow::Result<()>),
    ];

    let pb = ProgressBar::new(steps.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    for (name, check_fn) in steps {
        pb.set_message(format!("Checking {}", name));
        checks += 1;
        match check_fn() {
            Ok(_) => {
                pb.println(format!("{} {}", "✓".green(), name));
            },
            Err(e) => {
                failures += 1;
                pb.println(format!("{} {} - {}", "✗".red(), name, e));
            }
        }
        pb.inc(1);
    }

    pb.finish_and_clear();

    println!("\n{}", "Diagnostic Summary".bold().underline());
    println!("Total Checks: {}", checks);
    println!("Passed:       {}", (checks - failures).to_string().green());
    println!("Failed:       {}", failures.to_string().red());

    if failures > 0 {
        println!("\n{}", "Recommendations:".bold().yellow());
        if check_sandbox().is_err() {
            #[cfg(target_os = "linux")]
            println!("- Install 'bubblewrap' (bwrap) for Linux process isolation.");
            #[cfg(target_os = "macos")]
            println!("- Ensure 'sandbox-exec' is available (default on macOS).");
            #[cfg(target_os = "windows")]
            println!("- Running on Windows. Job Objects will be used for isolation.");
        }
        if check_env().is_err() {
            println!("- Create a .env file or config.toml with required API keys.");
            println!("  Run `aimaxxing-gateway onboard` to generate one.");
        }
        if check_pixi().is_err() {
            #[cfg(target_os = "windows")]
            println!("- Pixi missing. Install with: `iwr -useb https://pixi.sh/install.ps1 | iex` (PowerShell)");
            #[cfg(not(target_os = "windows"))]
            println!("- Pixi missing. Install with: `curl -fsSL https://pixi.sh/install.sh | bash` (Bash)");
        }
        if check_node().is_err() {
            println!("- JS Runtime missing. We recommend installing Bun for native performance: `powershell -c \"irm https://bun.sh/install.ps1 | iex\"` (Windows)");
        }
    } else {
        println!("\n{}", "System is ready for takeoff! 🚀".bold().green());
    }

    Ok(())
}

fn check_rag_mode() -> anyhow::Result<()> {
    // Engram uses provider APIs for embeddings, no local models needed
    // Check if the data directory and DB are accessible
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let db_path = base.join("data").join("engram.db");
    if db_path.exists() {
        Ok(())
    } else {
        // First run - OK, will be created on startup
        Ok(())
    }
}

fn check_sandbox() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("bwrap")
            .arg("--version")
            .output()
            .map_err(|_| anyhow::anyhow!("'bwrap' (bubblewrap) not found. Required for Linux sandboxing."))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("'bwrap' found but failed to execute."));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("sandbox-exec")
            .arg("-n")
            .arg("default")
            .arg("true")
            .output()
            .map_err(|_| anyhow::anyhow!("'sandbox-exec' not found or inaccessible."))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("'sandbox-exec' failed to execute basic profile."));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Job objects are built-in, no external tool to check usually.
    }

    Ok(())
}

fn check_vectordb() -> anyhow::Result<()> {
    // Check if data directory is writable
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let data_dir = base.join("data");
    
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
    }

    let meta = std::fs::metadata(&data_dir)?;
    if meta.permissions().readonly() {
        return Err(anyhow::anyhow!("Data directory is read-only"));
    }

    Ok(())
}

fn check_env() -> anyhow::Result<()> {
    use std::env;
    let _ = dotenv::dotenv(); // Try verify .env loading

    // Heuristic: Check for at least ONE provider key
    let keys = [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "DEEPSEEK_API_KEY",
        "GEMINI_API_KEY",
        "MINIMAX_API_KEY",
        "OLLAMA_BASE_URL",
    ];

    for key in keys {
        if env::var(key).is_ok() {
            return Ok(());
        }
    }

    // Also check config file
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let config_path = base.join("aimaxxing.yaml");
    if config_path.exists() {
         return Ok(());
    }

    Err(anyhow::anyhow!("No API keys found in ENV or aimaxxing.yaml"))
}

fn check_pixi() -> anyhow::Result<()> {
    if which::which("pixi").is_ok() {
        return Ok(());
    }
    
    // Check locally managed bin
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                .join("aimaxxing")
        });
    let managed = base.join("bin").join(if cfg!(windows) { "pixi.exe" } else { "pixi" });
        
    if managed.exists() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("pixi binary not found in PATH or standard locations"))
    }
}

fn check_node() -> anyhow::Result<()> {
    if which::which("bun").is_ok() || which::which("node").is_ok() {
        return Ok(());
    }
    
    // Check locally managed bin
    let base = std::env::var("AIMAXXING_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_local_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
                .join("aimaxxing")
        });
    let managed_bun = base.join("bin").join(if cfg!(windows) { "bun.exe" } else { "bun" });

    if managed_bun.exists() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Neither node nor bun found in PATH"))
    }
}
