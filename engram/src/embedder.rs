//! Text embedding generation using ONNX Runtime
//!
//! Converts text to dense vector representations using pre-trained models.
//! Supports both local ONNX models and provides mean pooling for sentence embeddings.

use crate::error::{EngramError, Result};
use crate::storage::Storage;
use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokenizers::{PaddingParams, Tokenizer};

/// Configuration for the embedder
#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub config_path: PathBuf,
    pub normalize: bool,
    /// Device to use (cpu, cuda, metal, or auto). Default: auto
    pub device: Option<String>,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/model.safetensors"),
            tokenizer_path: PathBuf::from("models/tokenizer.json"),
            config_path: PathBuf::from("models/config.json"),
            normalize: true,
            device: None, // Auto-detect
        }
    }
}

pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    config: EmbedderConfig,
    device: Device,
    dimension: usize,
    storage: Option<Arc<dyn Storage>>,
}

impl Embedder {
    /// Check if all required model files exist
    pub fn check_files(config: &EmbedderConfig) -> bool {
        config.model_path.exists() && config.tokenizer_path.exists() && config.config_path.exists()
    }

    pub fn new() -> Result<Self> {
        Self::with_config(EmbedderConfig::default(), None)
    }

    pub fn with_config(config: EmbedderConfig, storage: Option<Arc<dyn Storage>>) -> Result<Self> {
        let device = match config.device.as_deref() {
            Some("cpu") => Device::Cpu,
            Some("cuda") => Device::new_cuda(0)
                .map_err(|e| EngramError::Custom(format!("CUDA error: {}", e)))?,
            Some("metal") => Device::new_metal(0)
                .map_err(|e| EngramError::Custom(format!("Metal error: {}", e)))?,
            Some("auto") | None => {
                if candle_core::utils::cuda_is_available() {
                    tracing::info!("Auto-detected CUDA, using GPU");
                    Device::new_cuda(0)
                        .map_err(|e| EngramError::Custom(format!("CUDA error: {}", e)))?
                } else if candle_core::utils::metal_is_available() {
                    tracing::info!("Auto-detected Metal, using GPU");
                    Device::new_metal(0)
                        .map_err(|e| EngramError::Custom(format!("Metal error: {}", e)))?
                } else {
                    tracing::info!("Using CPU");
                    Device::Cpu
                }
            }
            Some(d) => return Err(EngramError::Custom(format!("Unknown device: {}", d))),
        };

        let config_content = std::fs::read_to_string(&config.config_path)
            .map_err(|e| EngramError::Custom(format!("Failed to read config file: {}", e)))?;
        let bert_config: Config = serde_json::from_str(&config_content)
            .map_err(|e| EngramError::Custom(format!("Failed to parse config: {}", e)))?;

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[config.model_path.clone()],
                candle_core::DType::F32,
                &device,
            )
        }
        .map_err(|e| EngramError::Custom(format!("Failed to load safetensors: {}", e)))?;

        let model = BertModel::load(vb, &bert_config)
            .map_err(|e| EngramError::Custom(format!("Failed to load BertModel: {}", e)))?;

        let mut tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| EngramError::Custom(format!("Failed to load tokenizer: {}", e)))?;

        if let Some(pp) = tokenizer.get_padding_mut() {
            pp.strategy = tokenizers::PaddingStrategy::BatchLongest;
        } else {
            let pp = PaddingParams {
                strategy: tokenizers::PaddingStrategy::BatchLongest,
                ..Default::default()
            };
            tokenizer.with_padding(Some(pp));
        }

        let dimension = bert_config.hidden_size;

        Ok(Self {
            model,
            tokenizer,
            config,
            device,
            dimension,
            storage,
        })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Err(EngramError::Custom("Cannot embed empty text".to_string()));
        }
        self.embed_batch(&[text])?
            .into_iter()
            .next()
            .ok_or_else(|| EngramError::Internal("No embedding returned".to_string()))
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = vec![vec![0.0; self.dimension]; texts.len()];
        let mut to_embed = Vec::new();
        let mut to_embed_indices = Vec::new();

        for (i, &text) in texts.iter().enumerate() {
            if let Some(storage) = &self.storage {
                let hash = self.hash_text(text);
                if let Ok(Some(cached)) = storage.get_embedding_cache(&hash) {
                    if cached.len() == self.dimension {
                        results[i] = cached;
                        continue;
                    }
                }
            }
            to_embed.push(text);
            to_embed_indices.push(i);
        }

        if to_embed.is_empty() {
            return Ok(results);
        }

        // Tokenization for remaining
        let tokens = self
            .tokenizer
            .encode_batch(to_embed.clone(), true)
            .map_err(|e| EngramError::Custom(format!("Tokenization failed: {}", e)))?;

        // ... truncated (replacing the actual inference logic block) ...
        // Forward pass
        let token_ids = tokens
            .iter()
            .map(|t| {
                let ids = t.get_ids().to_vec();
                Tensor::new(ids.as_slice(), &self.device)
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| EngramError::Custom(format!("Tensor creation failed: {}", e)))?;

        let token_ids = Tensor::stack(&token_ids, 0)
            .map_err(|e| EngramError::Custom(format!("Stack failed: {}", e)))?;

        let token_type_ids = token_ids
            .zeros_like()
            .map_err(|e| EngramError::Custom(format!("Zeros like failed: {}", e)))?;

        let attention_mask = token_ids
            .ne(0u32)
            .map_err(|e| EngramError::Custom(format!("Mask failed: {}", e)))?
            .to_dtype(candle_core::DType::U32)
            .map_err(|e| EngramError::Custom(format!("Mask cast failed: {}", e)))?;

        let embeddings = self
            .model
            .forward(&token_ids, &token_type_ids, Some(&attention_mask))
            .map_err(|e| EngramError::Custom(format!("Model forward failed: {}", e)))?;

        // Mean pooling logic (same as before but applied to 'embeddings')
        let mask_float = attention_mask
            .to_dtype(candle_core::DType::F32)
            .map_err(|e| EngramError::Custom(format!("Mask cast f32 failed: {}", e)))?;

        let mask_broadcast = mask_float
            .unsqueeze(2)
            .map_err(|e| EngramError::Custom(format!("Unsqueeze failed: {}", e)))?
            .broadcast_as(embeddings.shape())
            .map_err(|e| EngramError::Custom(format!("Broadcast failed: {}", e)))?;

        let masked_embeddings = embeddings
            .mul(&mask_broadcast)
            .map_err(|e| EngramError::Custom(format!("Mul failed: {}", e)))?;

        let sum_embeddings = masked_embeddings
            .sum(1)
            .map_err(|e| EngramError::Custom(format!("Sum failed: {}", e)))?;
        let sum_mask = mask_float
            .sum(1)
            .map_err(|e| EngramError::Custom(format!("Mask sum failed: {}", e)))?;

        let sum_mask = sum_mask
            .clamp(1e-9, f32::MAX)
            .map_err(|e| EngramError::Custom(format!("Clamp failed: {}", e)))?;
        let sum_mask = sum_mask
            .unsqueeze(1)
            .map_err(|e| EngramError::Custom(format!("Unsqueeze mask failed: {}", e)))?
            .broadcast_as(sum_embeddings.shape())
            .map_err(|e| EngramError::Custom(format!("Broadcast mask failed: {}", e)))?;

        let pooled = sum_embeddings
            .div(&sum_mask)
            .map_err(|e| EngramError::Custom(format!("Div failed: {}", e)))?;

        let pooled = if self.config.normalize {
            let norm = pooled
                .sqr()
                .map_err(|e| EngramError::Custom(format!("Sqr failed: {}", e)))?
                .sum_keepdim(1)
                .map_err(|e| EngramError::Custom(format!("Sum keepdim failed: {}", e)))?
                .sqrt()
                .map_err(|e| EngramError::Custom(format!("Sqrt failed: {}", e)))?;

            let norm_broadcast = norm
                .broadcast_as(pooled.shape())
                .map_err(|e| EngramError::Custom(format!("Norm broadcast failed: {}", e)))?;

            pooled
                .div(&norm_broadcast)
                .map_err(|e| EngramError::Custom(format!("Norm div failed: {}", e)))?
        } else {
            pooled
        };

        let new_embeddings = pooled
            .to_vec2::<f32>()
            .map_err(|e| EngramError::Custom(format!("To vec2 failed: {}", e)))?;

        // Cache and merge
        for (i, emb) in new_embeddings.into_iter().enumerate() {
            let idx = to_embed_indices[i];
            let text = to_embed[i];
            results[idx] = emb.clone();
            if let Some(storage) = &self.storage {
                let hash = self.hash_text(text);
                let _ = storage.put_embedding_cache(&hash, &emb);
            }
        }

        Ok(results)
    }

    fn hash_text(&self, text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text);
        format!("{:x}", hasher.finalize())
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// L2 normalize a vector (helper for tests)
    #[allow(dead_code)]
    fn normalize_vector(vec: &[f32]) -> Vec<f32> {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm == 0.0 {
            vec.to_vec()
        } else {
            vec.iter().map(|x| x / norm).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require the ONNX model to be present
    // Run with: cargo test --features vector -- --ignored

    #[test]
    #[ignore] // Requires model file
    fn test_embed_single_text() {
        let config = EmbedderConfig::default();
        let embedder = Embedder::with_config(config, None).unwrap();

        let text = "This is a test sentence for embedding.";
        let embedding = embedder.embed(text).unwrap();

        assert_eq!(embedding.len(), 384);

        // Check normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "Embedding should be normalized");
    }

    #[test]
    #[ignore] // Requires model file
    fn test_embed_batch() {
        let embedder = Embedder::new().unwrap();

        let texts = vec!["First sentence", "Second sentence with more words", "Third"];

        let embeddings = embedder.embed_batch(&texts).unwrap();

        assert_eq!(embeddings.len(), 3);
        for emb in embeddings {
            assert_eq!(emb.len(), 384);
        }
    }

    #[test]
    fn test_normalize_vector() {
        let vec = vec![3.0, 4.0]; // Length 5
        let normalized = Embedder::normalize_vector(&vec);

        assert_eq!(normalized.len(), 2);
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);

        // Check unit length
        let norm: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_zero_vector() {
        let vec = vec![0.0, 0.0, 0.0];
        let normalized = Embedder::normalize_vector(&vec);

        assert_eq!(normalized, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    #[ignore]
    fn test_embed_empty_text() {
        let embedder = Embedder::new().unwrap();
        let result = embedder.embed("");

        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_semantic_similarity() {
        let embedder = Embedder::new().unwrap();

        let text1 = "The cat sits on the mat";
        let text2 = "A cat is sitting on a mat";
        let text3 = "The dog runs in the park";

        let emb1 = embedder.embed(text1).unwrap();
        let emb2 = embedder.embed(text2).unwrap();
        let emb3 = embedder.embed(text3).unwrap();

        // Cosine similarity
        let sim_1_2: f32 = emb1.iter().zip(&emb2).map(|(a, b)| a * b).sum();
        let sim_1_3: f32 = emb1.iter().zip(&emb3).map(|(a, b)| a * b).sum();

        // text1 and text2 should be more similar than text1 and text3
        assert!(
            sim_1_2 > sim_1_3,
            "Semantic similarity failed: {} <= {}",
            sim_1_2,
            sim_1_3
        );
    }

    #[test]
    fn test_embedding_cache_storage() {
        use crate::storage::redb_impl::EngramKV;
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_cache.db");
        let kv = EngramKV::open(db_path).unwrap();

        let hash = "abc-123-hash";
        let vector = vec![0.1, 0.2, 0.3, 0.4];

        kv.put_embedding_cache(hash, &vector).unwrap();
        let cached = kv.get_embedding_cache(hash).unwrap();
        assert_eq!(cached.unwrap(), vector);

        // Verify mismatch (different hash)
        let none = kv.get_embedding_cache("other").unwrap();
        assert!(none.is_none());
    }
}
