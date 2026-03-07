//! Differentiated quantization for AIMAXXING Engram.
//!
//! Provides multiple quantization levels to balance persona stability and memory efficiency:
//! - **Full (FP32)**: No loss, used for core "Soul" and meta-instructions.
//! - **Warm (U8)**: 8-bit scalar quantization for recent/relevant memory.
//! - **Cold (INT4)**: 4-bit scalar quantization for long-term/background knowledge.

use serde::{Deserialize, Serialize};

/// Quantization levels for AIMAXXING's differentiated memory strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QuantLevel {
    /// Level 0: Full precision (FP32). 4 bytes per dimension.
    Full = 0,
    /// Level 1: Warm memory (U8). 1 byte per dimension.
    Warm = 1,
    /// Level 2: Cold memory (INT4). 0.5 bytes per dimension (packed).
    Cold = 2,
    /// Level 3: Background memory (Q1.58). ~0.2 bytes per dimension (ternary).
    Background = 3,
}

/// Common trait for vector quantization.
pub trait Quantizer: Send + Sync {
    /// Encode a float vector into compact codes.
    fn encode(&self, vector: &[f32]) -> Vec<u8>;

    /// Decode compact codes back to an approximate float vector.
    fn decode(&self, codes: &[u8]) -> Vec<f32>;

    /// Get the quantization level.
    fn level(&self) -> QuantLevel;

    /// Get the vector dimensionality.
    fn dim(&self) -> usize;
}

/// Scalar Quantizer implementation for U8 and INT4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarQuantizer {
    pub level: QuantLevel,
    pub dim: usize,
    /// Per-dimension min values for scaling.
    pub min_vals: Vec<f32>,
    /// Per-dimension max values for scaling.
    pub max_vals: Vec<f32>,
}

impl ScalarQuantizer {
    /// Create a new scalar quantizer by training on a set of vectors.
    pub fn train(vectors: &[&[f32]], level: QuantLevel) -> Self {
        assert!(!vectors.is_empty());
        let dim = vectors[0].len();
        let mut min_vals = vec![f32::INFINITY; dim];
        let mut max_vals = vec![f32::NEG_INFINITY; dim];

        for v in vectors {
            for (d, &val) in v.iter().enumerate() {
                if val < min_vals[d] {
                    min_vals[d] = val;
                }
                if val > max_vals[d] {
                    max_vals[d] = val;
                }
            }
        }

        // Stability check
        for d in 0..dim {
            if (max_vals[d] - min_vals[d]).abs() < 1e-6 {
                max_vals[d] = min_vals[d] + 1.0;
            }
        }

        Self {
            level,
            dim,
            min_vals,
            max_vals,
        }
    }
}

impl Quantizer for ScalarQuantizer {
    fn encode(&self, vector: &[f32]) -> Vec<u8> {
        let mut codes = Vec::with_capacity(if self.level == QuantLevel::Cold {
            self.dim / 2 + self.dim % 2
        } else {
            self.dim
        });

        match self.level {
            QuantLevel::Full => {
                // Should not really be called for Full, but for completeness:
                let bytes: Vec<u8> = vector
                    .iter()
                    .flat_map(|&f| f.to_le_bytes().to_vec())
                    .collect();
                bytes
            }
            QuantLevel::Warm => {
                for (d, &val) in vector.iter().enumerate() {
                    let range = self.max_vals[d] - self.min_vals[d];
                    let normalized = ((val - self.min_vals[d]) / range).clamp(0.0, 1.0);
                    codes.push((normalized * 255.0).round() as u8);
                }
                codes
            }
            QuantLevel::Cold => {
                // 4-bit packing
                for d in (0..self.dim).step_by(2) {
                    let v1 = vector[d];
                    let r1 = self.max_vals[d] - self.min_vals[d];
                    let n1 = ((v1 - self.min_vals[d]) / r1).clamp(0.0, 1.0);
                    let q1 = (n1 * 15.0).round() as u8;

                    let q2 = if d + 1 < self.dim {
                        let v2 = vector[d + 1];
                        let r2 = self.max_vals[d + 1] - self.min_vals[d + 1];
                        let n2 = ((v2 - self.min_vals[d + 1]) / r2).clamp(0.0, 1.0);
                        (n2 * 15.0).round() as u8
                    } else {
                        0
                    };

                    codes.push((q2 << 4) | q1);
                }
                codes
            }
            QuantLevel::Background => {
                panic!("ScalarQuantizer does not support Background level (use TernaryQuantizer)");
            }
        }
    }

    fn decode(&self, codes: &[u8]) -> Vec<f32> {
        let mut vector = Vec::with_capacity(self.dim);

        match self.level {
            QuantLevel::Full => {
                for i in (0..codes.len()).step_by(4) {
                    let mut b = [0u8; 4];
                    b.copy_from_slice(&codes[i..i + 4]);
                    vector.push(f32::from_le_bytes(b));
                }
            }
            QuantLevel::Warm => {
                for (d, &code) in codes.iter().enumerate() {
                    let range = self.max_vals[d] - self.min_vals[d];
                    let val = (code as f32 / 255.0) * range + self.min_vals[d];
                    vector.push(val);
                }
            }
            QuantLevel::Cold => {
                for (i, &byte) in codes.iter().enumerate() {
                    let q1 = byte & 0x0F;
                    let q2 = (byte >> 4) & 0x0F;

                    let d1 = i * 2;
                    let range1 = self.max_vals[d1] - self.min_vals[d1];
                    vector.push((q1 as f32 / 15.0) * range1 + self.min_vals[d1]);

                    let d2 = d1 + 1;
                    if d2 < self.dim {
                        let range2 = self.max_vals[d2] - self.min_vals[d2];
                        vector.push((q2 as f32 / 15.0) * range2 + self.min_vals[d2]);
                    }
                }
            }
            QuantLevel::Background => {
                panic!("ScalarQuantizer does not support Background level (use TernaryQuantizer)");
            }
        }
        vector
    }

    fn level(&self) -> QuantLevel {
        self.level
    }
    fn dim(&self) -> usize {
        self.dim
    }
}

/// Ternary Quantizer (Q1.58) for ultimate background memory compression.
/// Maps floats to {-1, 0, 1} using a symmetric scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TernaryQuantizer {
    pub dim: usize,
}

impl TernaryQuantizer {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Quantizer for TernaryQuantizer {
    fn encode(&self, vector: &[f32]) -> Vec<u8> {
        // 1.58-bit packing: 5 elements per u8 (3^5 = 243 < 256)
        // For simplicity in this prototype, we'll use 2-bit packing (4 elements per u8)
        let mut codes = Vec::with_capacity(self.dim / 4 + 1);

        // Compute per-vector scale
        let mut abs_sum = 0.0;
        for &v in vector {
            abs_sum += v.abs();
        }
        let scale = abs_sum / self.dim as f32;

        // Store scale as f16 (2 bytes) at the beginning
        let scale_bits = f32_to_f16(scale);
        codes.extend_from_slice(&scale_bits.to_le_bytes());

        for i in (0..self.dim).step_by(4) {
            let mut byte = 0u8;
            for j in 0..4 {
                if i + j < self.dim {
                    let val = vector[i + j];
                    let q = if scale > 0.0 {
                        (val / scale).round().clamp(-1.0, 1.0) as i8
                    } else {
                        0
                    };
                    // Map {-1, 0, 1} to {0, 1, 2}
                    let bits = (q + 1) as u8;
                    byte |= bits << (j * 2);
                }
            }
            codes.push(byte);
        }
        codes
    }

    fn decode(&self, codes: &[u8]) -> Vec<f32> {
        if codes.len() < 2 {
            return vec![0.0; self.dim];
        }

        let scale_bits = u16::from_le_bytes([codes[0], codes[1]]);
        let scale = f16_to_f32(scale_bits);

        let mut vector = Vec::with_capacity(self.dim);
        for &byte in &codes[2..] {
            for j in 0..4 {
                if vector.len() < self.dim {
                    let bits = (byte >> (j * 2)) & 0x03;
                    let q = (bits as i8) - 1;
                    vector.push(q as f32 * scale);
                }
            }
        }
        vector
    }

    fn level(&self) -> QuantLevel {
        QuantLevel::Background
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// Helper: f32 to f16 conversion (simplified)
pub(crate) fn f32_to_f16(val: f32) -> u16 {
    let bits = val.to_bits();
    let sign = (bits >> 31) & 1;
    let exp = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x7FFFFF;

    let mut res_exp = exp - 127 + 15;
    let mut res_mant = mant >> 13;

    if res_exp <= 0 {
        res_exp = 0;
        res_mant = 0;
    } else if res_exp >= 31 {
        res_exp = 31;
        res_mant = 0;
    }

    ((sign << 15) | ((res_exp as u32) << 10) | (res_mant as u32)) as u16
}

/// Helper: f16 to f32 conversion (re-implemented here for standalone use)
pub(crate) fn f16_to_f32(bits: u16) -> f32 {
    let sign = (bits >> 15) & 1;
    let exp = (bits >> 10) & 0x1F;
    let mant = bits & 0x03FF;

    if exp == 0 {
        if mant == 0 {
            return if sign == 0 { 0.0 } else { -0.0 };
        }
        return (if sign == 0 { 1.0 } else { -1.0 }) * (mant as f32) * f32::powi(2.0, -14 - 10);
    } else if exp == 0x1F {
        return if mant == 0 { f32::INFINITY } else { f32::NAN };
    }

    (if sign == 0 { 1.0 } else { -1.0 })
        * ((1 << 10 | mant) as f32)
        * f32::powi(2.0, exp as i32 - 15 - 10)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for (va, vb) in a.iter().zip(b.iter()) {
            dot += va * vb;
            norm_a += va * va;
            norm_b += vb * vb;
        }
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }

    #[test]
    fn test_scalar_quantization_warm() {
        let v1 = vec![0.1, 0.5, -0.3, 0.8];
        let v2 = vec![0.2, 0.4, -0.2, 0.7];
        let quant = ScalarQuantizer::train(&[&v1, &v2], QuantLevel::Warm);

        let encoded = quant.encode(&v1);
        let decoded = quant.decode(&encoded);

        let sim = cosine_similarity(&v1, &decoded);
        assert!(sim > 0.999); // U8 Should be extremely accurate
    }

    #[test]
    fn test_scalar_quantization_cold() {
        let v1 = vec![0.1, 0.5, -0.3, 0.8];
        let v2 = vec![0.2, 0.4, -0.2, 0.7];
        let quant = ScalarQuantizer::train(&[&v1, &v2], QuantLevel::Cold);

        let encoded = quant.encode(&v1);
        let decoded = quant.decode(&encoded);

        let sim = cosine_similarity(&v1, &decoded);
        assert!(sim > 0.95); // INT4 Should be decent
    }

    #[test]
    fn test_ternary_quantization() {
        let v = vec![0.1, 0.5, -0.3, 0.8, -0.1, 0.0, 0.4, -0.9];
        let quant = TernaryQuantizer::new(8);

        let encoded = quant.encode(&v);
        let decoded = quant.decode(&encoded);

        let sim = cosine_similarity(&v, &decoded);
        // Ternary is rough, but should maintain general direction
        assert!(sim > 0.7);
    }
}
