use std::path::{Path, PathBuf};

use api_types::InputNodeKind;
use tokio::process::Command;

#[derive(Clone)]
pub struct Ffmpeg {
    binary: String,
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl Ffmpeg {
    pub fn new(binary: String) -> Self {
        Self { binary }
    }

    pub fn binary(&self) -> &str {
        &self.binary
    }

    fn probe_binary(&self) -> String {
        self.binary
            .replace("ffmpeg", "ffprobe")
    }

    pub async fn probe(&self, input: &Path) -> Result<ProbeResult, FfmpegError> {
        let output = Command::new(self.probe_binary())
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("-show_streams")
            .arg(input)
            .output()
            .await
            .map_err(|source| FfmpegError::Spawn {
                op: "probe",
                source,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(FfmpegError::NonZeroExit {
                op: "probe",
                input: input.to_path_buf(),
                output: PathBuf::new(),
                status: output.status.code(),
                stderr,
            });
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| FfmpegError::ProbeParseError(e.to_string()))?;

        let duration_secs = json["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok());

        let mut width = None;
        let mut height = None;
        if let Some(streams) = json["streams"].as_array() {
            for stream in streams {
                if stream["codec_type"].as_str() == Some("video") {
                    width = stream["width"].as_u64().map(|v| v as u32);
                    height = stream["height"].as_u64().map(|v| v as u32);
                    break;
                }
            }
        }

        Ok(ProbeResult {
            duration_secs,
            width,
            height,
        })
    }

    pub async fn make_thumbnail(
        &self,
        kind: InputNodeKind,
        input: &Path,
        output: &Path,
    ) -> Result<(), FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error");

        match kind {
            InputNodeKind::Video => {
                cmd.arg("-ss")
                    .arg("0.5")
                    .arg("-i")
                    .arg(input)
                    .arg("-frames:v")
                    .arg("1")
                    .arg("-vf")
                    .arg("scale=100:-2");
            }
            InputNodeKind::Image => {
                cmd.arg("-i")
                    .arg(input)
                    .arg("-frames:v")
                    .arg("1")
                    .arg("-vf")
                    .arg("scale=100:-2");
            }
            InputNodeKind::Audio | InputNodeKind::VideoArray => {
                return Err(FfmpegError::WrongKindForThumbnail(kind));
            }
        }

        cmd.arg(output);
        run(cmd, "thumbnail", input, output).await
    }

    pub async fn generate_frame_at(
        &self,
        input: &Path,
        t_secs: f64,
    ) -> Result<Vec<u8>, FfmpegError> {
        self.generate_frame_at_width(input, t_secs, 100).await
    }

    pub async fn generate_frame_at_width(
        &self,
        input: &Path,
        t_secs: f64,
        width: u32,
    ) -> Result<Vec<u8>, FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-ss")
            .arg(format!("{:.3}", t_secs))
            .arg("-i")
            .arg(input)
            .arg("-frames:v")
            .arg("1")
            .arg("-vf")
            .arg(format!("scale={}:-2", width))
            .arg("-f")
            .arg("image2pipe")
            .arg("-vcodec")
            .arg("png")
            .arg("pipe:1");

        let output = cmd.output().await.map_err(|source| FfmpegError::Spawn {
            op: "frame_at",
            source,
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(FfmpegError::NonZeroExit {
                op: "frame_at",
                input: input.to_path_buf(),
                output: PathBuf::from("pipe:1"),
                status: output.status.code(),
                stderr,
            });
        }

        Ok(output.stdout)
    }

    pub async fn extract_audio(&self, input: &Path, output: &Path) -> Result<(), FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-i")
            .arg(input)
            .arg("-vn")
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg("-ar")
            .arg("44100")
            .arg("-ac")
            .arg("2")
            .arg(output);
        run(cmd, "extract_audio", input, output).await
    }

    pub async fn detect_silence(
        &self,
        input: &Path,
        noise_db: f64,
    ) -> Result<Vec<SilenceSegment>, FfmpegError> {
        let filter = format!("silencedetect=noise={}dB:d=0.5", noise_db);
        let output = Command::new(&self.binary)
            .arg("-hide_banner")
            .arg("-i")
            .arg(input)
            .arg("-af")
            .arg(&filter)
            .arg("-f")
            .arg("null")
            .arg("-")
            .output()
            .await
            .map_err(|source| FfmpegError::Spawn {
                op: "detect_silence",
                source,
            })?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut segments = Vec::new();
        let mut current_start: Option<f64> = None;

        for line in stderr.lines() {
            if let Some(pos) = line.find("silence_start: ") {
                let s = &line[pos + "silence_start: ".len()..];
                if let Some(val) = s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())
                {
                    current_start = Some(val);
                }
            }
            if let Some(pos) = line.find("silence_end: ") {
                let s = &line[pos + "silence_end: ".len()..];
                if let Some(end) = s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())
                {
                    if let Some(start) = current_start.take() {
                        segments.push(SilenceSegment {
                            start,
                            end,
                            duration: end - start,
                        });
                    }
                }
            }
        }

        Ok(segments)
    }

    pub async fn trim_audio(
        &self,
        input: &Path,
        output: &Path,
        start_secs: f64,
        duration_secs: f64,
    ) -> Result<(), FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-ss")
            .arg(format!("{:.3}", start_secs))
            .arg("-t")
            .arg(format!("{:.3}", duration_secs))
            .arg("-i")
            .arg(input)
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg(output);
        run(cmd, "trim_audio", input, output).await
    }

    pub async fn trim_video(
        &self,
        input: &Path,
        output: &Path,
        start_secs: f64,
        duration_secs: f64,
    ) -> Result<(), FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-ss")
            .arg(format!("{:.3}", start_secs))
            .arg("-t")
            .arg(format!("{:.3}", duration_secs))
            .arg("-i")
            .arg(input)
            .arg("-c:v")
            .arg("libx264")
            .arg("-c:a")
            .arg("aac")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(output);
        run(cmd, "trim_video", input, output).await
    }

    pub async fn encode_png_sequence(
        &self,
        frames_dir: &Path,
        fps: u32,
        output: &Path,
    ) -> Result<(), FfmpegError> {
        let pattern = frames_dir.join("frame_%06d.png");
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-framerate")
            .arg(fps.to_string())
            .arg("-i")
            .arg(&pattern)
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-preset")
            .arg("fast")
            .arg(output);
        run(cmd, "encode_png_sequence", &pattern, output).await
    }

    /// Spawn an ffmpeg process that decodes video to raw RGBA frames via stdout pipe.
    /// Returns (child, width, height). Read `frame_size = w*h*4` bytes per frame from stdout.
    pub fn spawn_frame_reader(
        &self,
        input: &Path,
        fps: u32,
        width: u32,
        height: u32,
    ) -> Result<std::process::Child, FfmpegError> {
        let child = std::process::Command::new(&self.binary)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-i")
            .arg(input)
            .arg("-vf")
            .arg(format!("fps={},scale={}:{}", fps, width, height))
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-f")
            .arg("rawvideo")
            .arg("pipe:1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|source| FfmpegError::Spawn {
                op: "spawn_frame_reader",
                source,
            })?;
        Ok(child)
    }

    /// Spawn an ffmpeg process that encodes raw RGBA frames from stdin to mp4.
    pub fn spawn_frame_writer(
        &self,
        width: u32,
        height: u32,
        fps: u32,
        output: &Path,
    ) -> Result<std::process::Child, FfmpegError> {
        let child = std::process::Command::new(&self.binary)
            .arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-s")
            .arg(format!("{}x{}", width, height))
            .arg("-r")
            .arg(fps.to_string())
            .arg("-i")
            .arg("pipe:0")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-preset")
            .arg("fast")
            .arg("-crf")
            .arg("23")
            .arg(output)
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|source| FfmpegError::Spawn {
                op: "spawn_frame_writer",
                source,
            })?;
        Ok(child)
    }

    pub async fn convert_to_16k_mono(&self, input: &Path, output: &Path) -> Result<(), FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-i")
            .arg(input)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg(output);
        run(cmd, "convert_16k_mono", input, output).await
    }

    pub async fn make_waveform(&self, input: &Path, output: &Path) -> Result<(), FfmpegError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-y")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-i")
            .arg(input)
            .arg("-filter_complex")
            .arg("showwavespic=s=900x300:colors=#4dabf7|#74c0fc")
            .arg("-frames:v")
            .arg("1")
            .arg(output);
        run(cmd, "waveform", input, output).await
    }
}

async fn run(
    mut cmd: Command,
    op: &'static str,
    input: &Path,
    output: &Path,
) -> Result<(), FfmpegError> {
    let output_result = cmd.output().await.map_err(|source| FfmpegError::Spawn {
        op,
        source,
    })?;
    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr).to_string();
        return Err(FfmpegError::NonZeroExit {
            op,
            input: input.to_path_buf(),
            output: output.to_path_buf(),
            status: output_result.status.code(),
            stderr,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SilenceSegment {
    pub start: f64,
    pub end: f64,
    pub duration: f64,
}

#[derive(thiserror::Error, Debug)]
pub enum FfmpegError {
    #[error("Cannot make thumbnail for input kind {0:?}")]
    WrongKindForThumbnail(InputNodeKind),
    #[error("Failed to spawn ffmpeg for {op}: {source}")]
    Spawn {
        op: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "ffmpeg {op} failed (status {status:?}) for input {input:?} -> {output:?}: {stderr}"
    )]
    NonZeroExit {
        op: &'static str,
        input: PathBuf,
        output: PathBuf,
        status: Option<i32>,
        stderr: String,
    },
    #[error("Failed to parse ffprobe output: {0}")]
    ProbeParseError(String),
}
