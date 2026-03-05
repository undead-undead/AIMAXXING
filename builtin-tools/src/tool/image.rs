//! DALL-E and Image Generation Tools
//!
//! Provides tools for generating and editing images using OpenAI's DALL-E API.

use async_trait::async_trait;
use serde::Deserialize;
use reqwest::Client;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use brain::skills::tool::{Tool, ToolDefinition};
use brain::error::{Error, Result};

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// Tool for generating images (DALL-E 3)
pub struct GenerateImageTool {
    api_key: String,
    base_url: String,
    client: Client,
    output_dir: PathBuf,
}

impl GenerateImageTool {
    /// Create a new GenerateImageTool
    pub fn new(api_key: impl Into<String>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: OPENAI_API_BASE.to_string(),
            client: Client::new(),
            output_dir: output_dir.into(),
        }
    }

    /// Create from environment
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| Error::ProviderAuth("OPENAI_API_KEY not set for GenerateImageTool".to_string()))?;
        let output_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Ok(Self::new(api_key, output_dir))
    }
}

#[derive(Deserialize)]
struct GenerateImageArgs {
    prompt: String,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default = "default_size")]
    size: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default = "default_style")]
    style: String,
    #[serde(default)]
    output_filename: Option<String>,
}

fn default_model() -> String { "dall-e-3".to_string() }
fn default_size() -> String { "1024x1024".to_string() }
fn default_quality() -> String { "standard".to_string() }
fn default_style() -> String { "vivid".to_string() }

#[async_trait]
impl Tool for GenerateImageTool {
    fn name(&self) -> String {
        "generate_image".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Generate an image based on a text prompt using DALL-E 3.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Detailed description of the image to generate"
                    },
                    "model": {
                        "type": "string",
                        "description": "Model to use (default: dall-e-3)"
                    },
                    "size": {
                        "type": "string",
                        "description": "Image size: '1024x1024', '1024x1792', '1792x1024' (defaults to 1024x1024)"
                    },
                    "quality": {
                        "type": "string",
                        "description": "Quality: 'standard' or 'hd' (default: standard)"
                    },
                    "style": {
                        "type": "string",
                        "description": "Style: 'vivid' (hyper-real/dramatic) or 'natural' (default: vivid)"
                    },
                    "output_filename": {
                        "type": "string",
                        "description": "Optional filename for saving (e.g. 'image.png')"
                    }
                },
                "required": ["prompt"]
            }),
            parameters_ts: Some("interface GenerateImageArgs { \n  prompt: string; \n  model?: 'dall-e-3'; \n  size?: '1024x1024'|'1024x1792'|'1792x1024'; \n  quality?: 'standard'|'hd'; \n  style?: 'vivid'|'natural'; \n  output_filename?: string; \n}".to_string()),
            is_binary: false,
            is_verified: false,
            usage_guidelines: Some("Use this to create new images from text descriptions. Returns the path to the saved image file on disk.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: GenerateImageArgs = serde_json::from_str(arguments)
            .map_err(|e| Error::ToolArguments { 
                tool_name: self.name(), 
                message: format!("Invalid arguments: {}", e) 
            })?;

        let json_body = serde_json::json!({
            "model": args.model,
            "prompt": args.prompt,
            "n": 1,
            "size": args.size,
            "quality": args.quality,
            "style": args.style,
            "response_format": "b64_json" 
        });

        let response = self.client.post(format!("{}/images/generations", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Image Generation API failed: {}", error_text));
        }

        #[derive(Deserialize)]
        struct ImageData {
            b64_json: String,
            revised_prompt: Option<String>,
        }

        #[derive(Deserialize)]
        struct ImageResponse {
            data: Vec<ImageData>,
        }

        let result: ImageResponse = response.json().await?;
        
        let image_data = result.data.first()
            .ok_or_else(|| anyhow::anyhow!("No image data returned"))?;

        // Decode Base64
        use base64::{Engine as _, engine::general_purpose};
        let image_bytes = general_purpose::STANDARD
            .decode(&image_data.b64_json)
            .map_err(|e| anyhow::anyhow!("Failed to decode base64 image: {}", e))?;

        // Save to file
        let output_filename = args.output_filename.unwrap_or_else(|| {
            format!("image_{}.png", chrono::Utc::now().timestamp())
        });
        
        let output_path = if Path::new(&output_filename).is_absolute() {
             PathBuf::from(output_filename)
        } else {
             self.output_dir.join(output_filename)
        };

        // Ensure parent dir exists
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file = File::create(&output_path).await?;
        file.write_all(&image_bytes).await?;

        let mut success_msg = format!("Image generated and saved to: {}", output_path.to_string_lossy());
        if let Some(revised) = &image_data.revised_prompt {
            success_msg.push_str(&format!("\nRevised Prompt used: {}", revised));
        }

        Ok(success_msg)
    }
}
