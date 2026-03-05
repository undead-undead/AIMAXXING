//! Voice interaction tools (STT and TTS)
//!
//! Provides tools for Speech-to-Text (Transcribe) and Text-to-Speech (Speak).
//! Currently supports OpenAI API compatible endpoints.

use async_trait::async_trait;
use serde::Deserialize;
use reqwest::{Client, multipart};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt; // For write_all

use brain::skills::tool::{Tool, ToolDefinition};
use brain::error::{Error, Result};

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// Tool for transcribing audio to text (STT)
pub struct TranscribeTool {
    api_key: String,
    base_url: String,
    client: Client,
}

impl TranscribeTool {
    /// Create a new TranscribeTool with optional API key and base URL
    pub fn new(api_key: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| OPENAI_API_BASE.to_string()),
            client: Client::new(),
        }
    }

    /// Create from environment variable
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| Error::ProviderAuth("OPENAI_API_KEY not set for TranscribeTool".to_string()))?;
        let base_url = std::env::var("OPENAI_API_BASE").ok();
        Ok(Self::new(api_key, base_url))
    }
}

#[derive(Deserialize)]
struct TranscribeArgs {
    file_path: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
}

#[async_trait]
impl Tool for TranscribeTool {
    fn name(&self) -> String {
        "transcribe_audio".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Transcribe audio file to text using Whisper model.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "Absolute path to the audio file to transcribe"
                    },
                    "language": {
                        "type": "string",
                        "description": "Optional ISO-639-1 language code (e.g. 'en', 'zh')"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Optional prompt to guide the model's style or terminology"
                    }
                },
                "required": ["file_path"]
            }),
            parameters_ts: Some("interface TranscribeArgs { \n  file_path: string; // Absolute path to audio file\n  language?: string; // e.g. 'en', 'zh'\n  prompt?: string; // Context or spelling guide\n}".to_string()),
            is_binary: false,
            is_verified: false,
            usage_guidelines: Some("Use this to convert audio files (mp3, wav, etc.) to text.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: TranscribeArgs = serde_json::from_str(arguments)
            .map_err(|e| Error::ToolArguments { 
                tool_name: self.name(), 
                message: format!("Invalid arguments: {}", e) 
            })?;

        let path = Path::new(&args.file_path);
        if !path.exists() {
            return Err(anyhow::anyhow!("Audio file not found: {}", args.file_path));
        }

        // Read file content
        let file_content = tokio::fs::read(path).await
            .map_err(|e| anyhow::anyhow!("Failed to read audio file: {}", e))?;
        
        // Prepare multipart form
        let filename = path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();

        let file_part = multipart::Part::bytes(file_content)
            .file_name(filename);

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", "whisper-1");

        if let Some(lang) = args.language {
            form = form.text("language", lang);
        }
        if let Some(prompt) = args.prompt {
            form = form.text("prompt", prompt);
        }

        let response = self.client.post(format!("{}/audio/transcriptions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Transcription API failed: {}", error_text));
        }

        #[derive(Deserialize)]
        struct TranscriptionResponse {
            text: String,
        }

        let result: TranscriptionResponse = response.json().await?;
        Ok(result.text)
    }
}

/// Tool for converting text to speech (TTS)
pub struct SpeakTool {
    api_key: String,
    base_url: String,
    client: Client,
    output_dir: PathBuf,
}

impl SpeakTool {
    /// Create a new SpeakTool with optional API key and base URL
    pub fn new(api_key: impl Into<String>, base_url: Option<String>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| OPENAI_API_BASE.to_string()),
            client: Client::new(),
            output_dir: output_dir.into(),
        }
    }

    /// Create from environment
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| Error::ProviderAuth("OPENAI_API_KEY not set for SpeakTool".to_string()))?;
        let base_url = std::env::var("OPENAI_API_BASE").ok();
        let output_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Ok(Self::new(api_key, base_url, output_dir))
    }

    /// Set output directory
    pub fn with_output_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.output_dir = path.into();
        self
    }
}

fn default_voice() -> String {
    "alloy".to_string()
}

fn default_model() -> String {
    "tts-1".to_string()
}

#[derive(Deserialize)]
struct SpeakArgs {
    text: String,
    #[serde(default = "default_voice")]
    voice: String,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default)]
    speed: Option<f32>,
    #[serde(default)]
    output_filename: Option<String>,
}

#[async_trait]
impl Tool for SpeakTool {
    fn name(&self) -> String {
        "text_to_speech".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Convert text to speech audio file.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to convert to speech"
                    },
                    "voice": {
                        "type": "string",
                        "description": "Voice to use (alloy, echo, fable, onyx, nova, shimmer)",
                        "default": "alloy"
                    },
                    "model": {
                        "type": "string",
                        "description": "TTS model to use (tts-1, tts-1-hd)",
                        "default": "tts-1"
                    },
                    "speed": {
                        "type": "number",
                        "description": "Speed of the speech (0.25 to 4.0)",
                        "default": 1.0
                    },
                    "output_filename": {
                        "type": "string",
                        "description": "Optional custom filename for the output mp3"
                    }
                },
                "required": ["text"]
            }),
            parameters_ts: Some("interface SpeakArgs { \n  text: string; // Text to convert\n  voice?: 'alloy' | 'echo' | 'fable' | 'onyx' | 'nova' | 'shimmer';\n  model?: 'tts-1' | 'tts-1-hd';\n  speed?: number; // 0.25 to 4.0\n  output_filename?: string;\n}".to_string()),
            is_binary: false,
            is_verified: false,
            usage_guidelines: Some("Use this to generate audio files from text descriptions or agent responses.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: SpeakArgs = serde_json::from_str(arguments)
            .map_err(|e| Error::ToolArguments { 
                tool_name: self.name(), 
                message: format!("Invalid arguments: {}", e) 
            })?;

        let filename = args.output_filename.clone().unwrap_or_else(|| {
            format!("speech_{}.mp3", uuid::Uuid::new_v4().to_string().split('-').next().unwrap())
        });
        let output_path = self.output_dir.join(filename);

        let voice = if args.voice == default_voice() {
            std::env::var("VOICE_TTS_VOICE").unwrap_or(args.voice)
        } else {
            args.voice
        };

        let model = if args.model == default_model() {
            std::env::var("VOICE_TTS_MODEL").unwrap_or(args.model)
        } else {
            args.model
        };

        let local_enabled = std::env::var("VOICE_LOCAL_TTS_ENABLED").map(|v| v == "true").unwrap_or(false);
        if local_enabled {
            let local_path = std::env::var("VOICE_LOCAL_TTS_PATH").unwrap_or_default();
            if !local_path.is_empty() {
                tracing::info!("Local TTS enabled with path: {}. (Real local synthesis logic pending implementation in core-runtime)", local_path);
            }
        }

        let json_body = serde_json::json!({
            "model": model,
            "input": args.text,
            "voice": voice,
            "speed": args.speed.unwrap_or(1.0)
        });

        let response = self.client.post(format!("{}/audio/speech", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("TTS API failed: {}", error_text));
        }

        let bytes = response.bytes().await?;
        let mut file = File::create(&output_path).await?;
        file.write_all(&bytes).await?;

        Ok(format!("Audio saved to: {}", output_path.to_string_lossy()))
    }
}
