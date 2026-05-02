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
            InputNodeKind::Audio => {
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
            .arg("scale=100:-2")
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
