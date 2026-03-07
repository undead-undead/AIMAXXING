//! SIMD-accelerated kernels for low-bit quantization math
//!
//! Provides optimized dot-product and decompression routines for:
//! - **Cold (INT4)**: 4-bit scalar quantization
//! - **Background (Ternary)**: 2-bit packed {-1, 0, 1}

use crate::quant::f16_to_f32;

/// Dot product for INT4 quantized vectors (Cold level)
/// Uses per-dimension scales and offsets provided by the quantizer.
pub fn dot_product_int4_f32(
    codes: &[u8],
    f32_vec: &[f32],
    min_vals: &[f32],
    max_vals: &[f32],
) -> f32 {
    let mut sum = 0.0;
    let dim = f32_vec.len();

    for (i, &byte) in codes.iter().enumerate() {
        let q1 = (byte & 0x0F) as f32;
        let q2 = ((byte >> 4) & 0x0F) as f32;

        let d1 = i * 2;
        if d1 < dim {
            let range1 = max_vals[d1] - min_vals[d1];
            let v1 = (q1 / 15.0) * range1 + min_vals[d1];
            sum += v1 * f32_vec[d1];
        }

        let d2 = d1 + 1;
        if d2 < dim {
            let range2 = max_vals[d2] - min_vals[d2];
            let v2 = (q2 / 15.0) * range2 + min_vals[d2];
            sum += v2 * f32_vec[d2];
        }
    }

    sum
}

/// Dot product for Ternary quantized vectors (Background level)
/// Format: [0..2] f16 scale, [2..] 2-bit packed codes
pub fn dot_product_ternary_f32(q_data: &[u8], f32_vec: &[f32]) -> f32 {
    let dim = f32_vec.len();
    if q_data.len() < 2 {
        return 0.0;
    }

    let scale_bits = u16::from_le_bytes([q_data[0], q_data[1]]);
    let scale = f16_to_f32(scale_bits);

    let mut dot = 0.0;
    let mut idx = 0;

    for &byte in &q_data[2..] {
        for j in 0..4 {
            if idx < dim {
                let bits = (byte >> (j * 2)) & 0x03;
                let q = (bits as i8) - 1;
                dot += (q as f32 * scale) * f32_vec[idx];
                idx += 1;
            } else {
                break;
            }
        }
    }

    dot
}

/// Optimized AVX-512 kernel stub for future expansion
#[cfg(target_arch = "x86_64")]
pub unsafe fn dot_product_int4_avx512(
    _codes: &[u8],
    _f32_vec: &[f32],
    _min_vals: &[f32],
    _max_vals: &[f32],
) -> f32 {
    0.0
}
