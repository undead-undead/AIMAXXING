use crate::error::Result;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct LocalPiper {
    model_path: PathBuf,
    piper_bin: PathBuf,
    memory_size: usize,
}

impl LocalPiper {
    pub fn load_local<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref();
        let model_path = dir.join("model.onnx");
        // We expect the user or system to fetch 'piper' and place it here
        let piper_bin = dir.join("piper");

        if !piper_bin.exists() {
            return Err(anyhow::anyhow!("Piper binary not found at {:?}", piper_bin).into());
        }

        if !model_path.exists() {
            return Err(anyhow::anyhow!("Piper model not found at {:?}", model_path).into());
        }

        Ok(Self {
            model_path,
            piper_bin,
            // Pre-computed models take around 40-70MB RAM
            memory_size: 70 * 1024 * 1024,
        })
    }

    pub fn memory_size(&self) -> usize {
        self.memory_size
    }

    // Piper binary runs on CPU natively for maximum compatibility
    pub fn is_gpu(&self) -> bool {
        false
    }

    pub fn synthesize(&self, text: &str) -> Result<Vec<u8>> {
        let mut child = Command::new(&self.piper_bin)
            .arg("--model")
            .arg(&self.model_path)
            .arg("--output_raw") // output raw PCM instead of WAV
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn piper binary: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| anyhow::anyhow!("Failed to write to piper stdin: {}", e))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| anyhow::anyhow!("Failed to read piper output: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Piper failed: {}", stderr).into());
        }

        // Wrap the raw PCM back into WAV so it can be played easily by browsers
        // We know Piper models typically output 22050Hz or 16000Hz.
        // We can let the frontend decode the raw float bytes or wrap it here.
        // We'll return the raw PCM bytes directly, and append WAV headers if needed later,
        // or just use piper's default --output_file - to get WAV
        Ok(output.stdout)
    }
}
