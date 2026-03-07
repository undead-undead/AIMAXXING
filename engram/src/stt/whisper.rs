use super::audio::pcm_decode_wav;
use crate::error::Result;
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self, audio, Config as WhisperConfig};
use std::path::Path;
use tokenizers::Tokenizer;

pub struct LocalWhisper {
    model: whisper::model::Whisper,
    tokenizer: Tokenizer,
    mel_filters: Vec<f32>,
    config: WhisperConfig,
    language: Option<String>,
    device: Device,
    is_gpu: bool,
    memory_size: usize,
}

impl LocalWhisper {
    pub fn load_local<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref();
        let config_path = dir.join("config.json");
        let tokenizer_path = dir.join("tokenizer.json");
        let model_path = dir.join("model.safetensors");

        let config: WhisperConfig =
            serde_json::from_str(&std::fs::read_to_string(&config_path)?)
                .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to parse tokenizer: {}", e))?;

        let device = if candle_core::utils::cuda_is_available() {
            Device::new_cuda(0).unwrap_or(Device::Cpu)
        } else if candle_core::utils::metal_is_available() {
            Device::new_metal(0).unwrap_or(Device::Cpu)
        } else {
            Device::Cpu
        };

        let is_gpu = device.is_cuda() || device.is_metal();

        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[model_path], whisper::DTYPE, &device)? };
        let model = whisper::model::Whisper::load(&vb, config.clone())?;

        // Load mel filters
        let mel_bytes = match config.num_mel_bins {
            80 => include_bytes!("../../assets/stt/melfilters.bytes").as_slice(),
            128 => {
                return Err(anyhow::anyhow!(
                    "128 mel bins not yet supported for local embedded bytes"
                )
                .into())
            }
            nmel => return Err(anyhow::anyhow!("unexpected num_mel_bins: {}", nmel).into()),
        };
        let mut mel_filters = vec![0f32; mel_bytes.len() / 4];
        <byteorder::LittleEndian as byteorder::ByteOrder>::read_f32_into(
            mel_bytes,
            &mut mel_filters,
        );

        Ok(Self {
            model,
            tokenizer,
            mel_filters,
            config,
            language: None,
            device,
            is_gpu,
            // Estimate size: model params (~400M) + context cache
            memory_size: 450 * 1024 * 1024,
        })
    }

    pub fn memory_size(&self) -> usize {
        self.memory_size
    }

    pub fn is_gpu(&self) -> bool {
        self.is_gpu
    }

    pub fn set_language(&mut self, lang: &str) {
        self.language = Some(lang.to_string());
    }

    fn token_id(&self, token: &str) -> Result<u32> {
        self.tokenizer
            .token_to_id(token)
            .ok_or_else(|| anyhow::anyhow!("no token-id for {}", token).into())
    }

    pub fn transcribe(&mut self, audio_bytes: &[u8]) -> Result<String> {
        let (pcm_data, _sample_rate) = pcm_decode_wav(audio_bytes)?;
        let mel = audio::pcm_to_mel(&self.config, &pcm_data, &self.mel_filters);
        let mel_len = mel.len();
        let mel_tensor = Tensor::from_vec(
            mel,
            (
                1,
                self.config.num_mel_bins,
                mel_len / self.config.num_mel_bins,
            ),
            &self.device,
        )?;

        // Decode loop
        let sot_token = self.token_id(whisper::SOT_TOKEN)?;
        let transcribe_token = self.token_id(whisper::TRANSCRIBE_TOKEN)?;
        let no_timestamps_token = self.token_id(whisper::NO_TIMESTAMPS_TOKEN)?;
        let eot_token = self.token_id(whisper::EOT_TOKEN)?;

        let mut tokens = vec![sot_token];

        if let Some(lang) = &self.language {
            if let Ok(lang_token) = self.token_id(&format!("<|{}|>", lang)) {
                tokens.push(lang_token);
            }
        }
        tokens.push(transcribe_token);
        tokens.push(no_timestamps_token);

        let audio_features = self.model.encoder.forward(&mel_tensor, true)?;

        let sample_len = self.config.max_target_positions / 2;

        // Build suppress mask
        let mut suppress_tokens = vec![0f32; self.config.vocab_size];
        for &t in &self.config.suppress_tokens {
            if (t as usize) < suppress_tokens.len() {
                suppress_tokens[t as usize] = f32::NEG_INFINITY;
            }
        }
        let suppress_tokens = Tensor::new(suppress_tokens.as_slice(), &self.device)?;

        for i in 0..sample_len {
            let tokens_tensor = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let ys = self
                .model
                .decoder
                .forward(&tokens_tensor, &audio_features, i == 0)?;

            let (_, seq_len, _) = ys.dims3()?;
            let logits = self
                .model
                .decoder
                .final_linear(&ys.i((..1, seq_len - 1..))?)?
                .i(0)?
                .i(0)?;

            // Suppress tokens
            let logits = logits.broadcast_add(&suppress_tokens)?;

            // Greedy
            let logits_v: Vec<f32> = logits.to_vec1()?;
            let next_token = logits_v
                .iter()
                .enumerate()
                .max_by(|(_, u), (_, v)| u.total_cmp(v))
                .map(|(i, _)| i as u32)
                .unwrap();

            if next_token == eot_token || tokens.len() > self.config.max_target_positions {
                break;
            }
            tokens.push(next_token);
        }

        let decoded = self
            .tokenizer
            .decode(&tokens, true)
            .map_err(|e| anyhow::anyhow!("Failed to decode output tokens: {}", e))?;
        Ok(decoded.trim().to_string())
    }
}
