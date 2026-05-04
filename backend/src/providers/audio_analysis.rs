use std::path::Path;

use crate::providers::ffmpeg::Ffmpeg;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpeechBoundsResult {
    pub start_ms: f64,
    pub end_ms: f64,
    pub noise_floor_db: f64,
    pub threshold_db: f64,
}

pub async fn detect_speech_bounds(
    ffmpeg: &Ffmpeg,
    input: &Path,
    threshold_mul: f64,
    onset_windows: usize,
    offset_windows: usize,
    window_ms: u32,
) -> Result<SpeechBoundsResult, AudioAnalysisError> {
    // Convert to 16kHz mono WAV for consistent analysis
    let tmp_wav = input.with_extension("analysis.wav");
    ffmpeg
        .convert_to_16k_mono(input, &tmp_wav)
        .await
        .map_err(|e| AudioAnalysisError::Convert(e.to_string()))?;

    let wav_bytes = tokio::fs::read(&tmp_wav)
        .await
        .map_err(|e| AudioAnalysisError::Io(e.to_string()))?;
    let _ = tokio::fs::remove_file(&tmp_wav).await;

    let result =
        tokio::task::spawn_blocking(move || analyze_wav(&wav_bytes, threshold_mul, onset_windows, offset_windows, window_ms))
            .await
            .map_err(|e| AudioAnalysisError::Analysis(format!("join: {e}")))?;

    result
}

fn analyze_wav(
    wav_bytes: &[u8],
    threshold_mul: f64,
    onset_sustained: usize,
    offset_sustained: usize,
    window_ms: u32,
) -> Result<SpeechBoundsResult, AudioAnalysisError> {
    if wav_bytes.len() < 44 {
        return Err(AudioAnalysisError::Analysis("WAV too short".into()));
    }

    let channels = u16::from_le_bytes([wav_bytes[22], wav_bytes[23]]) as usize;
    let sample_rate = u32::from_le_bytes([wav_bytes[24], wav_bytes[25], wav_bytes[26], wav_bytes[27]]) as usize;
    let pcm = &wav_bytes[44..];

    // Convert to mono f32
    let frame_count = pcm.len() / (channels * 2);
    let mut mono = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        let mut sum = 0.0f32;
        for ch in 0..channels {
            let off = (i * channels + ch) * 2;
            if off + 1 < pcm.len() {
                let s = i16::from_le_bytes([pcm[off], pcm[off + 1]]);
                sum += s as f32 / 32768.0;
            }
        }
        mono.push(sum / channels as f32);
    }

    // RMS in windows
    let window_samples = (sample_rate as u32 * window_ms / 1000) as usize;
    if window_samples == 0 {
        return Err(AudioAnalysisError::Analysis("window too small".into()));
    }
    let window_count = mono.len() / window_samples;
    let mut rms = Vec::with_capacity(window_count);
    for w in 0..window_count {
        let start = w * window_samples;
        let end = start + window_samples;
        let sum_sq: f64 = mono[start..end]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        rms.push((sum_sq / window_samples as f64).sqrt());
    }

    // Noise floor: median of first 200ms
    let noise_window_count = (200 / window_ms as usize).max(1).min(rms.len());
    let mut noise_slice: Vec<f64> = rms[..noise_window_count].to_vec();
    noise_slice.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let noise_floor = noise_slice[noise_slice.len() / 2];
    let threshold = (noise_floor * threshold_mul).max(0.003);

    let noise_floor_db = if noise_floor > 0.0 {
        20.0 * noise_floor.log10()
    } else {
        -100.0
    };
    let threshold_db = 20.0 * threshold.log10();

    // Onset: first onset_sustained consecutive windows above threshold
    let start_ms = find_onset(&rms, threshold, onset_sustained, window_ms);

    // Offset: last offset_sustained consecutive windows above threshold
    let end_ms = find_offset(&rms, threshold, offset_sustained, window_ms);

    Ok(SpeechBoundsResult {
        start_ms: start_ms.unwrap_or(0.0),
        end_ms: end_ms.unwrap_or(mono.len() as f64 / sample_rate as f64 * 1000.0),
        noise_floor_db,
        threshold_db,
    })
}

fn find_onset(rms: &[f64], threshold: f64, sustained: usize, window_ms: u32) -> Option<f64> {
    for w in 0..rms.len().saturating_sub(sustained) {
        if rms[w..w + sustained].iter().all(|&r| r > threshold) {
            return Some(w as f64 * window_ms as f64);
        }
    }
    None
}

fn find_offset(rms: &[f64], threshold: f64, sustained: usize, window_ms: u32) -> Option<f64> {
    for w in (sustained..rms.len()).rev() {
        if rms[w - sustained..w].iter().all(|&r| r > threshold) {
            return Some(w as f64 * window_ms as f64);
        }
    }
    None
}

#[derive(thiserror::Error, Debug)]
pub enum AudioAnalysisError {
    #[error("Audio conversion failed: {0}")]
    Convert(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Analysis error: {0}")]
    Analysis(String),
}
