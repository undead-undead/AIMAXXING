use brain::capabilities::{Sidecar, SidecarManager, SidecarConfig};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = SidecarConfig::default();
    let manager = SidecarManager::new(config.clone());

    println!("Starting sidecar...");
    manager.start().await?;

    let address = format!("http://[::1]:{}", config.port);
    println!("Connecting to sidecar at {}...", address);
    
    let mut sidecar = Sidecar::connect(address).await?;

    println!("Executing Python code...");
    let response = sidecar.execute("print('Hello from Python Sidecar!')\nimport numpy as np\nprint(f'NumPy version: {np.__version__}')".to_string()).await?;

    println!("Stdout: {}", response.stdout);
    println!("Stderr: {}", response.stderr);
    
    if !response.stdout.contains("Hello from Python Sidecar!") {
        anyhow::bail!("Verification failed: unexpected stdout");
    }

    println!("Stopping sidecar...");
    manager.stop().await?;

    println!("Sidecar verification SUCCESS!");
    Ok(())
}
