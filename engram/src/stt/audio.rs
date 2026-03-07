use crate::error::Result;
use hound::WavReader;
use std::io::Cursor;

pub fn pcm_decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32)> {
    let mut reader = WavReader::new(Cursor::new(bytes))
        .map_err(|e| anyhow::anyhow!("Failed to read WAV: {}", e))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;

    let mut mono = Vec::new();
    if spec.sample_format == hound::SampleFormat::Int {
        if spec.bits_per_sample == 16 {
            let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap_or(0)).collect();
            for chunk in samples.chunks(channels) {
                let sum: f32 = chunk.iter().map(|&s| s as f32 / 32768.0).sum();
                mono.push(sum / channels as f32);
            }
        } else if spec.bits_per_sample == 32 {
            let samples: Vec<i32> = reader.samples::<i32>().map(|s| s.unwrap_or(0)).collect();
            for chunk in samples.chunks(channels) {
                let sum: f32 = chunk.iter().map(|&s| s as f32 / 2147483648.0).sum();
                mono.push(sum / channels as f32);
            }
        } else {
            return Err(anyhow::anyhow!("Unsupported bit depth: {}", spec.bits_per_sample).into());
        }
    } else if spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect();
        for chunk in samples.chunks(channels) {
            let sum: f32 = chunk.iter().sum();
            mono.push(sum / channels as f32);
        }
    } else {
        return Err(anyhow::anyhow!("Unsupported sample format").into());
    }

    if sample_rate != 16000 {
        use rubato::{
            Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
            WindowFunction,
        };
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let mut resampler =
            SincFixedIn::<f32>::new(16000.0 / sample_rate as f64, 2.0, params, mono.len(), 1)
                .map_err(|e| anyhow::anyhow!("Resampler init failed: {}", e))?;

        let mut waves_out = resampler
            .process(&[mono], None)
            .map_err(|e| anyhow::anyhow!("Resampling failed: {}", e))?;
        return Ok((waves_out.pop().unwrap(), 16000));
    }

    Ok((mono, sample_rate))
}
