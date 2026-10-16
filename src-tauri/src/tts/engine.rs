//! Kokoro's ONNX graph on the CPU through `ort`, voice packs, and WAV bytes.
//!
//! Inputs: `input_ids` int64 [1, n+2] (pad id 0 at both ends), `style` f32
//! [1, 256], `speed` f32 [1]. Output: `waveform` f32 [1, samples] at 24 kHz.
//! A voice pack is a raw little-endian f32 array [510, 1, 256]; row `n`
//! styles an utterance of `n` tokens.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use super::vocab::{MAX_TOKENS, PAD_ID};

pub const SAMPLE_RATE: u32 = 24_000;
const STYLE_DIM: usize = 256;

/// One voice's style vectors, one row per utterance length.
#[derive(Clone)]
pub struct VoicePack {
    rows: Vec<f32>,
}

impl VoicePack {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() % (STYLE_DIM * 4) != 0 {
            return Err(format!(
                "voice pack is {} bytes, not a whole number of {STYLE_DIM}-value rows",
                bytes.len()
            ));
        }
        let rows = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect::<Vec<_>>();
        if rows.iter().any(|v| !v.is_finite()) {
            return Err("voice pack holds a value that is not a number".to_string());
        }
        Ok(Self { rows })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    pub fn len(&self) -> usize {
        self.rows.len() / STYLE_DIM
    }

    /// The style row for an utterance of `tokens` ids, clamped to the pack.
    pub fn style(&self, tokens: usize) -> &[f32] {
        let row = tokens.min(self.len().saturating_sub(1));
        &self.rows[row * STYLE_DIM..(row + 1) * STYLE_DIM]
    }
}

pub struct Engine {
    session: Session,
}

impl Engine {
    pub fn load(model: &Path, threads: usize) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| format!("onnx session builder: {e}"))?
            .with_intra_threads(threads.max(1))
            .map_err(|e| format!("onnx threads: {e}"))?
            .commit_from_file(model)
            .map_err(|e| format!("load {}: {e}", model.display()))?;
        let inputs: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        for required in ["input_ids", "style", "speed"] {
            if !inputs.iter().any(|name| name == required) {
                return Err(format!(
                    "{} is not a Kokoro model: no '{required}' input",
                    model.display()
                ));
            }
        }
        Ok(Self { session })
    }

    /// Audio for at most `MAX_TOKENS` ids, as 24 kHz float samples.
    pub fn synthesize(
        &mut self,
        ids: &[i64],
        voice: &VoicePack,
        speed: f32,
    ) -> Result<Vec<f32>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        if ids.len() > MAX_TOKENS {
            return Err(format!(
                "{} tokens is over the model's {MAX_TOKENS}",
                ids.len()
            ));
        }
        let mut padded = Vec::with_capacity(ids.len() + 2);
        padded.push(PAD_ID);
        padded.extend_from_slice(ids);
        padded.push(PAD_ID);
        let input_ids = Tensor::from_array(([1usize, padded.len()], padded))
            .map_err(|e| format!("input_ids tensor: {e}"))?;
        let style = Tensor::from_array(([1usize, STYLE_DIM], voice.style(ids.len()).to_vec()))
            .map_err(|e| format!("style tensor: {e}"))?;
        let speed = Tensor::from_array(([1usize], vec![speed.clamp(0.5, 2.0)]))
            .map_err(|e| format!("speed tensor: {e}"))?;
        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "style" => style,
                "speed" => speed,
            ])
            .map_err(|e| format!("kokoro inference: {e}"))?;
        let output = outputs
            .get("waveform")
            .ok_or("kokoro returned no 'waveform' output")?;
        let (_, samples) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("kokoro audio tensor: {e}"))?;
        Ok(samples.to_vec())
    }
}

/// Mono 16-bit PCM WAV at `SAMPLE_RATE`. Samples are clamped to [-1, 1].
pub fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

pub fn duration_ms(samples: usize) -> u64 {
    (samples as u64 * 1000) / SAMPLE_RATE as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_bytes(rows: usize) -> Vec<u8> {
        (0..rows * STYLE_DIM)
            .flat_map(|i| (i as f32 / 1000.0).to_le_bytes())
            .collect()
    }

    #[test]
    fn voice_pack_picks_the_row_for_the_token_count() {
        let pack = VoicePack::from_bytes(&pack_bytes(510)).unwrap();
        assert_eq!(pack.len(), 510);
        assert_eq!(pack.style(3)[0], (3 * STYLE_DIM) as f32 / 1000.0);
        assert_eq!(pack.style(9999).len(), STYLE_DIM);
        assert_eq!(pack.style(9999)[0], (509 * STYLE_DIM) as f32 / 1000.0);
    }

    #[test]
    fn voice_pack_rejects_truncated_or_corrupt_files() {
        assert!(VoicePack::from_bytes(&[]).is_err());
        assert!(VoicePack::from_bytes(&[0u8; 1000]).is_err());
        let nan: Vec<u8> = (0..STYLE_DIM)
            .flat_map(|_| f32::NAN.to_le_bytes())
            .collect();
        assert!(VoicePack::from_bytes(&nan).is_err());
    }

    #[test]
    fn wav_header_describes_24k_mono_pcm() {
        let wav = wav_bytes(&[0.0, 0.5, -1.5, 1.0]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 24_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(wav.len(), 52);
        assert_eq!(i16::from_le_bytes([wav[46], wav[47]]), 16384);
        assert_eq!(i16::from_le_bytes([wav[48], wav[49]]), -i16::MAX);
        assert_eq!(duration_ms(24_000), 1000);
    }
}
