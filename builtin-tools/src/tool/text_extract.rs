  //! Text extraction tool — OCR via multiple backends with graceful degradation.
//!
//! Supports:
//! - Tesseract OCR (local, free, many languages)
//! - OpenAI Vision API (cloud fallback when local tools unavailable)
//! - Pure-Rust fallback: returns base64 of image for LLM multimodal input
//!
//! Degradation strategy:
//! 1. Try Tesseract if installed
//! 2. Fall back to OpenAI Vision API if configured
//! 3. Fall back to returning base64 for LLM multimodal processing

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use std::sync::Arc;
use brain::agent::provider::{Provider, ChatRequest};
use brain::agent::message::{Message, Role, Content, ContentPart, ImageSource};

use runtimes::WasmRuntime;
use engram::ensure_ocr_assets;

pub struct TextExtractTool {
    provider: Option<Arc<dyn Provider>>,
    model: Option<String>,
    wasm_runtime: Arc<WasmRuntime>,
}

impl TextExtractTool {
    /// Create a new TextExtractTool with optional provider and model for vision fallback
    pub fn new(provider: Option<Arc<dyn Provider>>, model: Option<String>) -> Self {
        Self { 
            provider, 
            model,
            wasm_runtime: Arc::new(WasmRuntime::new()),
        }
    }
}

#[derive(Deserialize)]
struct TextExtractArgs {
    action: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[async_trait]
impl Tool for TextExtractTool {
    fn name(&self) -> String { "text_extract".to_string() }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "text_extract".to_string(),
            description: "Extract text from images via OCR (Tesseract or Vision LLM). Auto-detects available backends.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["recognize", "info"], "description": "Action: 'recognize' to extract text, 'info' to check backends" },
                    "path": { "type": "string", "description": "Path to image file" },
                    "language": { "type": "string", "description": "OCR language (e.g., 'eng', 'chi_sim')" },
                    "backend": { "type": "string", "enum": ["tesseract", "api", "auto"], "description": "Backend preference (default: auto)" },
                    "model": { "type": "string", "description": "Specific Vision Model to use (overrides default agent model)" }
                },
                "required": ["action"]
            }),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this to extract text from images. Native Tesseract is preferred for local processing. Vision API is used as high-quality fallback.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: TextExtractArgs = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "text_extract".into(),
            message: e.to_string(),
        })?;

        let result = match args.action.as_str() {
            "info" => detect_backends(self.provider.is_some()).await,
            "recognize" => self.recognize(&args).await?,
            _ => json!({"error": format!("Unknown action: {}", args.action)}),
        };

        Ok(serde_json::to_string_pretty(&result)?)
    }
}

impl TextExtractTool {
    async fn recognize(&self, args: &TextExtractArgs) -> anyhow::Result<serde_json::Value> {
    if args.path.is_empty() {
        return Ok(json!({"error": "path is required"}));
    }
    if !tokio::fs::try_exists(&args.path).await.unwrap_or(false) {
        return Ok(json!({"error": format!("File not found: {}", args.path)}));
    }

    let backend = args.backend.as_deref().unwrap_or("auto");
    let lang = args.language.as_deref().unwrap_or("eng");

    // Strategy 1: WASM Tesseract (New Phase 3 - Prioritize built-in)
    if backend == "auto" || backend == "wasm" {
        // WASM currently only supports eng and chi_sim
        if lang == "eng" || lang == "chi_sim" || backend == "wasm" {
            if let Ok(text) = self.ocr_wasm(&args.path, lang).await {
                 return Ok(json!( {
                    "text": text,
                    "backend": "tesseract_wasm",
                    "language": lang,
                } ));
            }
        }
    }

    // Strategy 2: Tesseract (Native or Pixi)
    if backend == "auto" || backend == "tesseract" {
        match ocr_tesseract(&args.path, lang).await {
            Ok(text) => return Ok(json!( {
                "text": text,
                "backend": "tesseract",
                "language": lang,
            } )),
            Err(e) => {
                tracing::warn!("Tesseract (native/pixi) failed, trying fallback: {}", e);
            }
        }
    }

    // Strategy 2: LLM Vision API (via Provider)
    if backend == "auto" || backend == "api" {
        if let Some(p) = &self.provider {
            let model = args.model.as_ref().or(self.model.as_ref()).cloned().unwrap_or_else(|| "gpt-4o-mini".to_string());
            match ocr_via_provider(p.as_ref(), &args.path, &model).await {
                Ok(text) => return Ok(json!( {
                    "text": text,
                    "backend": format!("llm_vision ({})", p.name()),
                    "model": model,
                } )),
                Err(e) => {
                    tracing::warn!("LLM Vision via {} failed: {}", p.name(), e);
                }
            }
        }
    }

    // Strategy 3: Base64 fallback (always works)
    let bytes = tokio::fs::read(&args.path).await?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let preview = if b64.len() > 200 { format!("{}...({}B total)", &b64[..200], b64.len()) } else { b64.clone() };

    Ok(json!({
        "backend": "base64_fallback",
        "note": "No OCR backend available. Image encoded as base64 for LLM multimodal processing.",
        "base64_preview": preview,
        "base64_length": b64.len(),
        "degraded": true,
        "install_hint": "Install pixi/tesseract for other languages, or configure a Vision-enabled LLM Provider."
    }))
    }

    /// Run OCR via a WASM-compiled Tesseract component
    async fn ocr_wasm(&self, path: &str, lang: &str) -> anyhow::Result<String> {
        let wasm_path = engram::ensure_ocr_assets()?;
        let models_dir = wasm_path.parent().ok_or_else(|| anyhow::anyhow!("Invalid WASM path"))?;

        let args_json = json!({
            "image_path": path,
            "lang": lang
        });

        // We mount the models_dir as base_dir so the WASM can find .traineddata at ./eng.traineddata
        let output = self.wasm_runtime.call(
            &wasm_path, 
            &args_json.to_string(), 
            &models_dir.to_path_buf()
        ).await?;

        if !output.status.success() {
            anyhow::bail!("WASM OCR failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if result.is_empty() {
            anyhow::bail!("WASM OCR returned empty text");
        }
        Ok(result)
    }
}

/// Detect available OCR backends
async fn detect_backends(has_provider: bool) -> serde_json::Value {
    let tesseract_native = cmd_available("tesseract").await;
    let tesseract_pixi = cmd_available("pixi").await;
    let has_tesseract = tesseract_native || tesseract_pixi;

    let mut backends = Vec::new();
    if has_tesseract { backends.push("tesseract"); }
    if has_provider { backends.push("llm_vision"); }
    backends.push("base64_fallback"); // Always available

    let languages = if tesseract_native {
        get_tesseract_languages().await.unwrap_or_default()
    } else if tesseract_pixi {
        vec!["eng".to_string(), "chi_sim".to_string(), "(Auto-provision via Pixi: 125+ languages)".to_string()]
    } else {
        Vec::new()
    };

    json!( {
        "available_backends": backends,
        "tesseract_native": tesseract_native,
        "tesseract_ready": has_tesseract,
        "llm_vision_available": has_provider,
        "base64_fallback": true,
        "tesseract_languages": languages,
        "degradation_note": if !has_tesseract && !has_provider {
            "No OCR backend available. Install pixi/tesseract or configure an LLM provider with vision support. Base64 fallback will be used."
        } else { "" }
    } )
}

async fn ensure_tesseract_via_pixi() -> anyhow::Result<std::path::PathBuf> {
    let tesseract_env_dir = std::env::temp_dir().join("aimaxxing_tesseract_env");
    let pixi_toml = tesseract_env_dir.join("pixi.toml");
    
    if !tesseract_env_dir.exists() {
        tokio::fs::create_dir_all(&tesseract_env_dir).await?;
    }
    
    if !cmd_available("pixi").await {
        anyhow::bail!("pixi is not installed, cannot auto-provision Tesseract");
    }

    if !pixi_toml.exists() {
        tracing::info!("Auto-provisioning Tesseract via Pixi in {:?}...", tesseract_env_dir);
        let init_out = tokio::process::Command::new("pixi")
            .arg("init")
            .current_dir(&tesseract_env_dir)
            .output()
            .await?;
            
        if !init_out.status.success() {
            anyhow::bail!("Failed to init pixi env: {}", String::from_utf8_lossy(&init_out.stderr));
        }
        
        let add_out = tokio::process::Command::new("pixi")
            .arg("add")
            .arg("tesseract")
            .current_dir(&tesseract_env_dir)
            .output()
            .await?;
            
        if !add_out.status.success() {
            anyhow::bail!("Failed to add tesseract to pixi env: {}", String::from_utf8_lossy(&add_out.stderr));
        }
        tracing::info!("Successfully provisioned Tesseract via Pixi");
    }
    
    Ok(tesseract_env_dir)
}

async fn ocr_tesseract(path: &str, lang: &str) -> anyhow::Result<String> {
    let (mut cmd, args) = if cmd_available("tesseract").await {
        (tokio::process::Command::new("tesseract"), vec![path.to_string(), "stdout".to_string(), "-l".to_string(), lang.to_string()])
    } else {
        let env_dir = ensure_tesseract_via_pixi().await?;
        let mut c = tokio::process::Command::new("pixi");
        c.current_dir(env_dir);
        (c, vec!["run".to_string(), "tesseract".to_string(), path.to_string(), "stdout".to_string(), "-l".to_string(), lang.to_string()])
    };

    let output = cmd.args(&args).output().await?;

    if !output.status.success() {
        anyhow::bail!("Tesseract error: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn ocr_via_provider(provider: &dyn Provider, path: &str, model: &str) -> anyhow::Result<String> {
    let bytes = tokio::fs::read(path).await?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let media_type = "image/png"; // Default for most OCR screenshots
    
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![Message::user(Content::parts(vec![
            ContentPart::Text { text: "Extract ALL text from this image. Return only the extracted text, nothing else.".to_string() },
            ContentPart::Image { source: ImageSource::Base64 { media_type: media_type.to_string(), data: b64 } }
        ]))],
        max_tokens: Some(2000),
        ..Default::default()
    };

    let mut stream = provider.stream_completion(request).await?;
    let mut full_text = String::new();
    
    use futures::StreamExt;
    while let Some(choice) = stream.next().await {
        match choice? {
            brain::agent::streaming::StreamingChoice::Message(text) => full_text.push_str(&text),
            brain::agent::streaming::StreamingChoice::Done => break,
            _ => {}
        }
    }

    if full_text.is_empty() {
        anyhow::bail!("LLM Vision provider returned empty text");
    }
    Ok(full_text.trim().to_string())
}

async fn cmd_available(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

async fn get_tesseract_languages() -> anyhow::Result<Vec<String>> {
    let output = tokio::process::Command::new("tesseract")
        .args(["--list-langs"])
        .output()
        .await?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().skip(1).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_definition() {
        let tool = TextExtractTool::new(None, None);
        let def = tool.definition().await;
        assert_eq!(def.name, "text_extract");
    }

    #[tokio::test]
    async fn test_info_always_returns() {
        let tool = TextExtractTool::new(None, None);
        let result = tool.call(r#"{"action": "info"}"#).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["base64_fallback"], true); // Always available
    }
}
