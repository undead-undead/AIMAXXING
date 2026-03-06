//! Local Candle Reranker
//!
//! Provides precision reranking using lightweight cross-encoder models (e.g., BGE-Reranker).
//! These models take `(query, document)` pairs and output a relevance score, which is
//! far more accurate than standard dot-product similarity or BM25, but more computationally expensive.

#[cfg(feature = "vector")]
use crate::error::{EngramError, Result};
#[cfg(feature = "vector")]
use crate::hybrid_search::HybridSearchResult;
#[cfg(feature = "vector")]
use crate::reranker::Reranker;
#[cfg(feature = "vector")]
use candle_core::{Device, Tensor};
#[cfg(feature = "vector")]
use candle_nn::VarBuilder;
#[cfg(feature = "vector")]
use candle_transformers::models::xlm_roberta::{Config, XLMRobertaForSequenceClassification};
#[cfg(feature = "vector")]
use std::path::Path;
#[cfg(feature = "vector")]
use std::sync::Mutex;
#[cfg(feature = "vector")]
use tokenizers::Tokenizer;
#[cfg(feature = "vector")]
use tracing::{debug, info, warn};

#[cfg(feature = "vector")]
pub struct LocalCandleReranker {
    model: Mutex<XLMRobertaForSequenceClassification>,
    tokenizer: Mutex<Tokenizer>,
    device: Device,
}

#[cfg(feature = "vector")]
impl LocalCandleReranker {
    /// Load a BGE-M3 (or similar XLM-Roberta) cross-encoder model from local paths.
    ///
    /// The model directory must contain:
    /// - `model.safetensors`
    /// - `config.json`
    /// - `tokenizer.json`
    pub fn load_local(model_dir: impl AsRef<Path>) -> Result<Self> {
        let model_dir = model_dir.as_ref();
        let safetensors_path = model_dir.join("model.safetensors");
        let config_path = model_dir.join("config.json");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !safetensors_path.exists() || !config_path.exists() || !tokenizer_path.exists() {
            return Err(EngramError::InvalidInput(format!(
                "Model directory {} is missing required files (model.safetensors, config.json, tokenizer.json)",
                model_dir.display()
            )));
        }

        info!("Loading Local Reranker from: {}", model_dir.display());

        // Determine device (CUDA/Metal/CPU)
        let device = Device::cuda_if_available(0)
            .or_else(|_| Device::new_metal(0))
            .unwrap_or_else(|_| Device::Cpu);

        debug!("Reranker using device: {:?}", device);

        // Load Tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EngramError::ModelLoad(format!("Failed to load tokenizer: {}", e)))?;

        // Load Config
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| EngramError::ModelLoad(format!("Failed to read config: {}", e)))?;
        let config: Config = serde_json::from_str(&config_str)
            .map_err(|e| EngramError::ModelLoad(format!("Failed to parse config: {}", e)))?;

        // Load model weights
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[safetensors_path],
                candle_core::DType::F32,
                &device,
            )
        }
        .map_err(|e| EngramError::ModelLoad(format!("Failed to load weights: {}", e)))?;

        // Cross encoders have 1 label (a single float score)
        let model = XLMRobertaForSequenceClassification::new(1, &config, vb)
            .map_err(|e| EngramError::ModelLoad(format!("Failed to build model: {}", e)))?;

        info!("Local Reranker loaded successfully.");

        Ok(Self {
            model: Mutex::new(model),
            tokenizer: Mutex::new(tokenizer),
            device,
        })
    }

    /// Score a single query-document pair
    fn score(&self, query: &str, document_text: &str) -> Result<f32> {
        let mut tokenizer = self.tokenizer.lock().unwrap();

        // Truncate document text if too long (e.g. 512 total tokens minus query)
        // For BGE-Reranker, the format is generally: <s> query </s></s> document </s>
        let encoding = tokenizer
            .encode(
                (query, document_text), // encode pair
                true,
            )
            .map_err(|e| EngramError::ModelLoad(format!("Tokenization failed: {}", e)))?;

        let tokens = encoding.get_ids().to_vec();
        // Cross-encoders can be sensitive to length limit, BGE usually limit 512
        let tokens = if tokens.len() > 512 {
            tokens[..512].to_vec()
        } else {
            tokens
        };

        let token_tensor = Tensor::new(tokens.as_slice(), &self.device)
            .map_err(|e| EngramError::ModelLoad(format!("Tensor creation failed: {}", e)))?
            .unsqueeze(0) // Add batch dimension
            .map_err(|e| EngramError::ModelLoad(format!("Tensor shape error: {}", e)))?;

        let model = self.model.lock().unwrap();

        // Forward pass requires input_ids and optionally token_type_ids. We pass None for type IDs.
        // Some implementations of XLMRoberta just take `&Tensor` without type ids depending on the version.
        // Let's check `candle_transformers` 0.8 method signature which we saw was `pub fn forward(&self, token_ids: &Tensor, type_ids: &Tensor) -> Result<Tensor>`
        // For standard inputs without type ids, we can construct a zero tensor of the same shape.
        let type_ids = token_tensor
            .zeros_like()
            .map_err(|e| EngramError::ModelLoad(format!("Tensor creation failed: {}", e)))?;

        let mask_ids = token_tensor
            .ones_like()
            .map_err(|e| EngramError::ModelLoad(format!("Tensor creation failed: {}", e)))?;

        let logits = model
            .forward(&token_tensor, &type_ids, &mask_ids)
            .map_err(|e| EngramError::ModelLoad(format!("Model forward failed: {}", e)))?;

        // Extract the score.
        // BGE cross-encoders usually output a single float (logit) for relevance.
        // We often apply sigmoid to normalize between 0 and 1, though raw logits sort just fine.
        let raw_score = logits
            .to_vec2::<f32>()
            .map_err(|_| EngramError::ModelLoad("Failed to extract logits".to_string()))?[0][0];

        // Sigmoid normalization
        let score = 1.0 / (1.0 + (-raw_score).exp());

        Ok(score)
    }
}

#[cfg(feature = "vector")]
impl Reranker for LocalCandleReranker {
    fn rerank(
        &self,
        query: &str,
        documents: Vec<HybridSearchResult>,
    ) -> Result<Vec<HybridSearchResult>> {
        if documents.is_empty() {
            return Ok(documents);
        }

        let mut scored_docs = Vec::with_capacity(documents.len());

        for mut doc_result in documents {
            let content = match &doc_result.document.body {
                Some(b) => b.as_str(),
                None => continue,
            };

            // Call model inference
            match self.score(query, content) {
                Ok(cross_score) => {
                    // Overwrite the rrf score with the cross-encoder score since precision is much higher
                    doc_result.rrf_score = cross_score as f64;
                    scored_docs.push(doc_result);
                }
                Err(e) => {
                    warn!(
                        "Reranking failed for document {}, falling back: {}",
                        doc_result.document.path, e
                    );
                    scored_docs.push(doc_result);
                }
            }
        }

        // Re-sort strictly by the new cross-encoder score
        scored_docs.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(scored_docs)
    }
}
