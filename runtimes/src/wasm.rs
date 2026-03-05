use brain::error::{Error, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::{debug, info};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::*;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtx, WasiCtxBuilder, WasiView};
use async_trait::async_trait;

/// Host state for Wasm execution
struct HostState {
    wasi_ctx: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

/// A high-performance Wasm runtime for agent skills.
/// Now uses Lazy Initialization to avoid startup cost.
#[derive(Clone)]
pub struct WasmRuntime {
    engine: Arc<OnceCell<Engine>>,
}

impl WasmRuntime {
    /// Create a new Wasm runtime handle.
    /// Does NOT initialize the engine yet (Lazy).
    pub fn new() -> Self {
        Self {
            engine: Arc::new(OnceCell::new()),
        }
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmRuntime {
    async fn get_engine(&self) -> Result<&Engine> {
        self.engine
            .get_or_try_init(|| async {
                info!("Initializing WASM Engine (Lazy Loading)...");
                let mut config = Config::new();
                config.wasm_component_model(true);
                config.async_support(false);

                // Security: Enable Fuel for CPU limiting
                config.consume_fuel(true);

                let engine = Engine::new(&config)
                    .map_err(|e| Error::Internal(format!("Failed to create Wasm engine: {}", e)))?;

                info!("WASM Engine initialized successfully.");
                Ok(engine)
            })
            .await
    }

    /// Execute a Wasm skill
    pub async fn call(&self, wasm_path: &Path, arguments: &str, base_dir: &Path) -> Result<std::process::Output> {
        let engine = self.get_engine().await?;
        let wasm_path = wasm_path.to_path_buf();
        let arguments = arguments.to_string();
        let base_dir = base_dir.to_path_buf();
        let engine = engine.clone();

        // Offload heavy Wasm execution to a blocking thread to avoid stalling the async runtime
        tokio::task::spawn_blocking(move || {
            Self::call_blocking(&engine, &wasm_path, &arguments, &base_dir)
        })
        .await
        .map_err(|e| Error::Internal(format!("Wasm execution join error: {}", e)))?
    }

    /// Blocking implementation of call
    fn call_blocking(
        engine: &Engine,
        wasm_path: &Path,
        arguments: &str,
        base_dir: &Path,
    ) -> Result<std::process::Output> {
        use wasmtime_wasi::pipe::MemoryOutputPipe;

        let component = Component::from_file(engine, wasm_path)
            .map_err(|e| Error::Internal(format!("Failed to load Wasm component: {}", e)))?;

        let stdout = MemoryOutputPipe::new(1024 * 1024 * 10); // 10MB limit
        let stderr = MemoryOutputPipe::new(1024 * 1024 * 5); // 5MB limit

        let mut wasi_builder = WasiCtxBuilder::new();
        wasi_builder.stdout(stdout.clone());
        wasi_builder.stderr(stderr.clone());

        // Security: WASI Directory Mapping
        wasi_builder
            .preopened_dir(base_dir, ".", DirPerms::all(), FilePerms::all())
            .map_err(|e| Error::Internal(format!("Failed to mount base dir: {}", e)))?;

        let wasi = wasi_builder.build();

        // Security: Memory Limits (128MB)
        let limits = StoreLimitsBuilder::new()
            .memory_size(128 * 1024 * 1024)
            .instances(1)
            .tables(1)
            .memories(1)
            .build();

        let mut store = Store::new(
            engine,
            HostState {
                wasi_ctx: wasi,
                table: ResourceTable::new(),
                limits,
            },
        );

        // Security: CPU Limits (Fuel)
        store
            .set_fuel(500_000_000)
            .map_err(|e| Error::Internal(format!("Failed to set fuel: {}", e)))?;

        // Enforce memory limits
        store.limiter(|state| &mut state.limits);

        let mut linker = Linker::new(engine);
        wasmtime_wasi::add_to_linker_sync(&mut linker)
            .map_err(|e| Error::Internal(format!("Failed to link WASI: {}", e)))?;

        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|e| Error::Internal(format!("Failed to instantiate Wasm component: {}", e)))?;

        // Try to call run(input: string) -> string
        let mut ok = true;
        if let Some(run_func) = instance.get_func(&mut store, "run") {
            use wasmtime::component::Val;

            let mut results = [Val::Bool(false)]; 
            let args = [Val::String(arguments.to_string())];

            if let Err(e) = run_func.call(&mut store, &args, &mut results) {
                // Fallback to parameterless run()
                if let Err(e2) = run_func.call(&mut store, &[], &mut []) {
                    debug!("Wasm execution failed: {}, fallback failed: {}", e, e2);
                    ok = false;
                }
            }
        } else {
             return Err(Error::Internal("Wasm component must export a 'run' function".to_string()));
        }

        // Drop store to flush pipes
        drop(store);

        let stdout_data = stdout.try_into_inner().unwrap_or_default().to_vec();
        let stderr_data = stderr.try_into_inner().unwrap_or_default().to_vec();

        Ok(std::process::Output {
            status: {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    std::process::ExitStatus::from_raw(if ok { 0 } else { 1 } << 8)
                }
                #[cfg(not(unix))]
                {
                    std::process::Command::new(if ok { "true" } else { "false" }).status().unwrap_or_else(|_| {
                        std::process::Command::new("cmd").args(["/C", if ok { "exit 0" } else { "exit 1" }]).status().unwrap()
                    })
                }
            },
            stdout: stdout_data,
            stderr: stderr_data,
        })
    }
}

#[async_trait]
impl crate::SkillRuntime for WasmRuntime {
    async fn execute(
        &self,
        metadata: &crate::SkillMetadata,
        arguments: &str,
        base_dir: &Path,
        _config: &crate::SkillExecutionConfig,
        _env_manager: Option<&Arc<brain::env::EnvManager>>,
    ) -> Result<std::process::Output> {
         let wasm_file = metadata.script.as_ref().ok_or_else(|| {
            Error::ToolExecution {
                tool_name: metadata.name.clone(),
                message: "No wasm file defined for this skill".to_string(),
            }
        })?;
        let wasm_path = base_dir.join("scripts").join(wasm_file);

        self.call(&wasm_path, arguments, base_dir).await
    }
}
