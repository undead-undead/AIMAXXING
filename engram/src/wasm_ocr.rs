use anyhow::Result;
use std::fs;
use std::path::PathBuf;

/// Embedded OCR assets
const TESSERACT_WASM: &[u8] = include_bytes!("../assets/ocr/tesseract.wasm");
const ENG_TRAINEDDATA: &[u8] = include_bytes!("../assets/ocr/eng.traineddata");
const CHI_SIM_TRAINEDDATA: &[u8] = include_bytes!("../assets/ocr/chi_sim.traineddata");

/// Ensures that the OCR assets are extracted to the standard models directory.
/// Returns the path to the extracted tesseract.wasm.
pub fn ensure_ocr_assets() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let models_dir = PathBuf::from(home).join(".aimaxxing").join("models");

    if !models_dir.exists() {
        fs::create_dir_all(&models_dir)?;
    }

    let wasm_path = models_dir.join("tesseract.wasm");
    let eng_path = models_dir.join("eng.traineddata");
    let chi_path = models_dir.join("chi_sim.traineddata");

    // Helper to write if missing
    let write_if_missing = |path: &PathBuf, data: &[u8], name: &str| -> Result<()> {
        if !path.exists() {
            tracing::info!("Extracting embedded OCR asset: {}", name);
            fs::write(path, data)?;
        }
        Ok(())
    };

    write_if_missing(&wasm_path, TESSERACT_WASM, "tesseract.wasm")?;
    write_if_missing(&eng_path, ENG_TRAINEDDATA, "eng.traineddata")?;
    write_if_missing(&chi_path, CHI_SIM_TRAINEDDATA, "chi_sim.traineddata")?;

    Ok(wasm_path)
}
