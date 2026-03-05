//! Llama.cpp provider implementation for local GGUF inference
//!
//! Fulfills Phase 14 of the CLAwv2 supplementary plan.
//! Supports offline execution using Metal/CUDA via llama-cpp-2.

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaModel, AddBos};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::sampling::LlamaSampler;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{Error, Result, StreamingChoice, StreamingResponse, Provider};

/// Local GGUF provider using llama.cpp
pub struct LlamaCpp {
    model: Arc<LlamaModel>,
    backend: Arc<LlamaBackend>,
}

impl LlamaCpp {
    /// Create from a GGUF model file path
    pub fn new(model_path: impl Into<PathBuf>) -> Result<Self> {
        let backend = Arc::new(LlamaBackend::init().map_err(|e| Error::Internal(format!("Failed to init llama-cpp: {}", e)))?);
        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, model_path.into(), &model_params)
            .map_err(|e| Error::Internal(format!("Failed to load GGUF model: {}", e)))?;

        Ok(Self {
            model: Arc::new(model),
            backend,
        })
    }
}

#[async_trait]
impl Provider for LlamaCpp {
    async fn stream_completion(
        &self,
        request: aimaxxing_core::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        let model = self.model.clone();
        let backend = self.backend.clone();
        let (tx, rx) = mpsc::channel(100);

        // Simple inference loop (non-optimized for Phase 14 prototype)
        tokio::task::spawn_blocking(move || {
            let ctx_params = LlamaContextParams::default();
            let mut ctx = model.new_context(&backend, ctx_params).expect("Failed to create context");
            let mut sampler = LlamaSampler::greedy();
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            
            // Format prompt from messages (simplified for prototype)
            let mut prompt = String::new();
            if let Some(sys) = &request.system_prompt {
                prompt.push_str(&format!("<|system|>\n{}<|end|>\n", sys));
            }
            for msg in request.messages {
                let role = match msg.role {
                    aimaxxing_core::agent::message::Role::User => "user",
                    aimaxxing_core::agent::message::Role::Assistant => "assistant",
                    _ => "user",
                };
                prompt.push_str(&format!("<|{}|>\n{}<|end|>\n", role, msg.text()));
            }
            prompt.push_str("<|assistant|>\n");

            // Tokenize and generate
            let tokens = model.str_to_token(&prompt, AddBos::Always).expect("Tokenization failed");
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(tokens.len(), 1);
            for (i, &token) in tokens.iter().enumerate() {
                let _ = batch.add(token, i as i32, &[0], i == tokens.len() - 1);
            }
            
            ctx.decode(&mut batch).expect("Initial decode failed");
            
            let mut n_cur = tokens.len();
            while n_cur < request.max_tokens.unwrap_or(512) as usize {
                let token = sampler.sample(&ctx, 0);
                if model.is_eog_token(token) {
                    break;
                }
                
                let piece = model.token_to_piece(token, &mut decoder, true, None).unwrap_or_default();
                if tx.blocking_send(Ok(StreamingChoice::Message(piece))).is_err() {
                    break;
                }
                
                batch.clear();
                let _ = batch.add(token, n_cur as i32, &[0], true);
                ctx.decode(&mut batch).expect("Decode failed");
                n_cur += 1;
            }
            
            let _ = tx.blocking_send(Ok(StreamingChoice::Done));
        });

        Ok(StreamingResponse::from_stream(ReceiverStream::new(rx)))
    }

    fn name(&self) -> &'static str {
        "llama_cpp"
    }

    fn metadata() -> aimaxxing_core::agent::provider::ProviderMetadata {
        aimaxxing_core::agent::provider::ProviderMetadata {
            id: "llama_cpp".to_string(),
            name: "Llama.cpp (Local GGUF)".to_string(),
            description: "Local inference using GGUF models via llama.cpp".to_string(),
            icon: "🦙".to_string(),
            fields: vec![aimaxxing_core::agent::provider::ProviderField {
                key: "llama_cpp_model_path".to_string(),
                label: "Model Path".to_string(),
                field_type: "text".to_string(),
                description: "Absolute path to your .gguf model file".to_string(),
                required: true,
                default: None,
            }],
            capabilities: vec![],
            preferred_models: vec!["local-model".to_string()],
        }
    }
}
