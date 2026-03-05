use std::path::{Path, PathBuf};
use std::fs;
use std::io::Read;
use tracing::{info, warn, error};
use zip::ZipArchive;

use crate::error::{Error, Result};
#[cfg(not(target_arch = "wasm32"))]
use crate::agent::evolution::auditor::{Auditor, AuditResult, ChangeType};

/// Security module for validating and inspecting imported .vessel packages.
pub struct VesselInspector {
    #[cfg(not(target_arch = "wasm32"))]
    auditor: Option<Auditor>,
}

impl VesselInspector {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            auditor: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_auditor(mut self, auditor: Auditor) -> Self {
        self.auditor = Some(auditor);
        self
    }

    /// Layer 1: Static Format Whitelist Extraction.
    /// Unpacks a .vessel (zip) file to a target directory, strictly rejecting
    /// any executable binaries, shell scripts, or unknown blobs.
    pub fn safe_extract(&self, vessel_path: &Path, extract_to: &Path) -> Result<()> {
// ... existing safe_extract logic ...
        if !vessel_path.exists() {
            return Err(Error::Internal(format!("Vessel file not found: {:?}", vessel_path)));
        }

        let file = fs::File::open(vessel_path)
            .map_err(|e| Error::Internal(format!("Failed to open vessel zip: {}", e)))?;
            
        let mut archive = ZipArchive::new(file)
            .map_err(|e| Error::Internal(format!("Invalid zip format: {}", e)))?;

        // 1. Blocklist of dangerous extensions
        let dangerous_extensions = [
            "exe", "sh", "bash", "bat", "cmd", "ps1", "vbs", "so", "dylib", "dll",
            "bin", "app", "msi", "jar", "pyc", "class"
        ];

        if !extract_to.exists() {
            fs::create_dir_all(extract_to)
                .map_err(|e| Error::Internal(format!("Failed to create extract dir: {}", e)))?;
        }

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| Error::Internal(format!("Failed to read zip index {}: {}", i, e)))?;
                
            let outpath = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => {
                    warn!("Skipping suspicious path in zip: {}", file.name());
                    continue;
                }
            };

            // SECURITY: Check for dangerous extensions
            if let Some(ext) = outpath.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if dangerous_extensions.contains(&ext_str.as_str()) {
                    let msg = format!("SECURITY VIOLATION: Malicious file type detected in vessel: {:?}", outpath);
                    error!("{}", msg);
                    // Proactively destroy the extraction dir
                    let _ = fs::remove_dir_all(extract_to);
                    return Err(Error::Security(msg));
                }
            }

            let full_outpath = extract_to.join(&outpath);

            if file.is_dir() {
                fs::create_dir_all(&full_outpath)
                    .map_err(|e| Error::Internal(format!("Failed to create dir {:?}: {}", full_outpath, e)))?;
            } else {
                if let Some(p) = full_outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)
                            .map_err(|e| Error::Internal(format!("Failed to create parent dir {:?}: {}", p, e)))?;
                    }
                }
                
                let mut outfile = fs::File::create(&full_outpath)
                    .map_err(|e| Error::Internal(format!("Failed to create file {:?}: {}", full_outpath, e)))?;
                    
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| Error::Internal(format!("Failed to write file {:?}: {}", full_outpath, e)))?;
            }
        }

        info!("Successfully & safely extracted vessel to {:?}", extract_to);
        Ok(())
    }

    /// Layer 2: Auditor LLM Inspection.
    /// Scans the unpacked SOUL.md and IDENTITY.md for prompt injections or malicious instructions.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn inspect_soul(&self, extract_to: &Path) -> Result<()> {
        let soul_path = extract_to.join("SOUL.md");
        
        if !soul_path.exists() {
            info!("No SOUL.md found in vessel, skipping AI inspection.");
            return Ok(());
        }

        let soul_content = fs::read_to_string(&soul_path)
            .map_err(|e| Error::Internal(format!("Failed to read SOUL.md: {}", e)))?;

        if let Some(auditor) = &self.auditor {
            info!("Initiating Auditor LLM scan on incoming SOUL.md...");
            
            // Re-use the existing Auditor infrastructure to evaluate the text
            let change_type = ChangeType::SoulModification { role: "imported_soul".to_string() };
            let result = auditor.audit(&change_type, &soul_content).await;
            
            if !matches!(result, AuditResult::Approved) {
                let msg = format!("SECURITY VIOLATION: Auditor LLM rejected the imported SOUL.md: {:?}", result);
                error!("{}", msg);
                // Proactively destroy the extraction dir
                let _ = fs::remove_dir_all(extract_to);
                return Err(Error::Security(msg));
            }
            info!("Auditor LLM scan passed: SOUL.md is safe.");
        } else {
            warn!("Auditor LLM not configured! Skipping Layer 2 inspection.");
        }

        Ok(())
    }
}
