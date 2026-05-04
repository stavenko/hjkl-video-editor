use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::providers::ffmpeg::Ffmpeg;

pub struct WhisperProvider {
    ctx: Arc<Mutex<WhisperContext>>,
    ffmpeg: Ffmpeg,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionResult {
    pub segments: Vec<Segment>,
}

impl WhisperProvider {
    pub async fn new(model_path: &str, model_url: &str, ffmpeg: Ffmpeg) -> Result<Self, WhisperError> {
        let path = PathBuf::from(model_path);
        if !path.exists() {
            download_model(model_url, &path).await?;
        }
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| WhisperError::ModelLoad(format!("{e}")))?;
        tracing::info!("Whisper model loaded from {}", model_path);
        Ok(Self {
            ctx: Arc::new(Mutex::new(ctx)),
            ffmpeg,
        })
    }

    pub async fn transcribe(
        &self,
        audio_path: &Path,
        output_json: &Path,
    ) -> Result<(), WhisperError> {
        // Convert to 16kHz mono WAV via ffmpeg (whisper needs 16kHz f32 PCM)
        let tmp_wav = audio_path.with_extension("16k.wav");
        self.ffmpeg
            .convert_to_16k_mono(audio_path, &tmp_wav)
            .await
            .map_err(|e| WhisperError::AudioConvert(e.to_string()))?;

        // Read WAV and convert i16 PCM to f32
        let wav_bytes = tokio::fs::read(&tmp_wav)
            .await
            .map_err(|e| WhisperError::AudioConvert(e.to_string()))?;
        let _ = tokio::fs::remove_file(&tmp_wav).await;

        let samples = parse_wav_to_f32(&wav_bytes)?;

        // Run whisper (CPU/GPU bound — do in blocking task)
        let ctx = self.ctx.clone();
        let result = tokio::task::spawn_blocking(move || {
            let ctx = ctx.blocking_lock();
            let mut state = ctx
                .create_state()
                .map_err(|e| WhisperError::Transcribe(format!("{e}")))?;

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some("auto"));
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            // Split into word-level segments
            params.set_token_timestamps(true);
            params.set_max_len(1);
            params.set_split_on_word(true);

            state
                .full(params, &samples)
                .map_err(|e| WhisperError::Transcribe(format!("{e}")))?;

            let n = state.full_n_segments();
            let mut segments = Vec::with_capacity(n as usize);
            for i in 0..n {
                let seg = state.get_segment(i)
                    .ok_or_else(|| WhisperError::Transcribe(format!("segment {i} missing")))?;
                let text = seg.to_str()
                    .map_err(|e| WhisperError::Transcribe(format!("{e}")))?
                    .to_string();
                segments.push(Segment {
                    start_ms: seg.start_timestamp() * 10,
                    end_ms: seg.end_timestamp() * 10,
                    text,
                });
            }
            Ok::<_, WhisperError>(TranscriptionResult { segments })
        })
        .await
        .map_err(|e| WhisperError::Transcribe(format!("join error: {e}")))??;

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| WhisperError::Transcribe(e.to_string()))?;
        tokio::fs::write(output_json, json)
            .await
            .map_err(|e| WhisperError::Transcribe(e.to_string()))?;

        Ok(())
    }
}

async fn download_model(url: &str, dest: &Path) -> Result<(), WhisperError> {
    tracing::info!("Whisper model not found at {:?}, downloading from {}...", dest, url);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| WhisperError::ModelLoad(format!("create dir: {e}")))?;
    }

    let response = reqwest::get(url)
        .await
        .map_err(|e| WhisperError::ModelLoad(format!("download request: {e}")))?;

    if !response.status().is_success() {
        return Err(WhisperError::ModelLoad(format!(
            "download failed: HTTP {}",
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let tmp = dest.with_extension("bin.tmp");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| WhisperError::ModelLoad(format!("create file: {e}")))?;

    let mut downloaded: u64 = 0;
    let mut last_pct: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| WhisperError::ModelLoad(format!("download chunk: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| WhisperError::ModelLoad(format!("write: {e}")))?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = downloaded * 100 / total;
            if pct != last_pct {
                last_pct = pct;
                tracing::info!("Downloading whisper model: {}%", pct);
            }
        }
    }
    file.flush()
        .await
        .map_err(|e| WhisperError::ModelLoad(format!("flush: {e}")))?;
    drop(file);

    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| WhisperError::ModelLoad(format!("rename: {e}")))?;

    tracing::info!("Whisper model downloaded to {:?} ({} bytes)", dest, downloaded);
    Ok(())
}

fn parse_wav_to_f32(wav_bytes: &[u8]) -> Result<Vec<f32>, WhisperError> {
    // Minimal WAV parser: skip 44-byte header, read i16 LE samples
    if wav_bytes.len() < 44 {
        return Err(WhisperError::AudioConvert(
            "WAV file too short".to_string(),
        ));
    }
    let data = &wav_bytes[44..];
    let samples: Vec<f32> = data
        .chunks_exact(2)
        .map(|chunk| {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            sample as f32 / 32768.0
        })
        .collect();
    Ok(samples)
}

#[derive(thiserror::Error, Debug)]
pub enum WhisperError {
    #[error("Failed to load whisper model: {0}")]
    ModelLoad(String),
    #[error("Audio conversion failed: {0}")]
    AudioConvert(String),
    #[error("Transcription failed: {0}")]
    Transcribe(String),
}
