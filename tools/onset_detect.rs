///! Standalone onset detection tool
///! Usage: cargo run --example onset_detect -- <path_to_wav>
///!
///! Reads a WAV file (any sample rate, mono/stereo), computes RMS energy
///! in 10ms windows, finds noise floor from the first 200ms, then finds
///! the first sustained window above threshold.

use std::env;
use std::fs;

fn main() {
    let path = env::args().nth(1).expect("Usage: onset_detect <path.wav>");
    let data = fs::read(&path).expect("Failed to read file");

    // Parse WAV header (minimal: assumes PCM)
    assert!(data.len() > 44, "File too short for WAV");
    let channels = u16::from_le_bytes([data[22], data[23]]) as usize;
    let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;
    let bits_per_sample = u16::from_le_bytes([data[34], data[35]]) as usize;
    assert_eq!(bits_per_sample, 16, "Only 16-bit PCM supported");

    println!("WAV: {}Hz, {} ch, {} bits", sample_rate, channels, bits_per_sample);

    // Convert to mono f32
    let pcm_data = &data[44..];
    let samples_per_frame = channels;
    let frame_count = pcm_data.len() / (samples_per_frame * 2);
    let mut mono: Vec<f32> = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        let mut sum = 0.0f32;
        for ch in 0..channels {
            let offset = (i * channels + ch) * 2;
            let sample = i16::from_le_bytes([pcm_data[offset], pcm_data[offset + 1]]);
            sum += sample as f32 / 32768.0;
        }
        mono.push(sum / channels as f32);
    }

    println!("Total samples: {} ({:.2}s)", mono.len(), mono.len() as f64 / sample_rate as f64);

    // RMS in 10ms windows
    let window_samples = sample_rate / 100; // 10ms
    let window_count = mono.len() / window_samples;
    let mut rms: Vec<f64> = Vec::with_capacity(window_count);
    for w in 0..window_count {
        let start = w * window_samples;
        let end = start + window_samples;
        let sum_sq: f64 = mono[start..end].iter().map(|&s| (s as f64) * (s as f64)).sum();
        rms.push((sum_sq / window_samples as f64).sqrt());
    }

    // Noise floor: median of first 200ms (20 windows)
    let noise_windows = 20.min(rms.len());
    let mut noise_slice: Vec<f64> = rms[..noise_windows].to_vec();
    noise_slice.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let noise_floor = noise_slice[noise_slice.len() / 2];
    let noise_db = if noise_floor > 0.0 { 20.0 * noise_floor.log10() } else { -100.0 };
    println!("Noise floor: {:.6} ({:.1} dB)", noise_floor, noise_db);

    // Threshold: noise_floor * 8 (or at least -50dB absolute)
    let threshold = (noise_floor * 8.0).max(0.003);
    let threshold_db = 20.0 * threshold.log10();
    println!("Threshold: {:.6} ({:.1} dB)", threshold, threshold_db);

    // Find first sustained onset: 3+ consecutive windows above threshold
    let sustained = 3;
    let mut onset_window = None;
    for w in 0..rms.len().saturating_sub(sustained) {
        if rms[w..w + sustained].iter().all(|&r| r > threshold) {
            onset_window = Some(w);
            break;
        }
    }

    match onset_window {
        Some(w) => {
            let onset_ms = w as f64 * 10.0;
            let onset_s = onset_ms / 1000.0;
            println!("\n>>> Speech onset at window {}: {:.0}ms ({:.3}s)", w, onset_ms, onset_s);

            // Show energy around onset
            let from = w.saturating_sub(5);
            let to = (w + 10).min(rms.len());
            println!("\nEnergy around onset (10ms windows):");
            for i in from..to {
                let ms = i as f64 * 10.0;
                let db = if rms[i] > 0.0 { 20.0 * rms[i].log10() } else { -100.0 };
                let marker = if i == w { " <<< ONSET" } else { "" };
                println!("  {:6.0}ms: {:.6} ({:6.1} dB){}", ms, rms[i], db, marker);
            }
        }
        None => {
            println!("\n>>> No speech onset detected!");
        }
    }
}
