use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONTENT_TYPE: &str = "application/x-postcard";

fn default_one() -> f64 { 1.0 }
fn default_preview_width() -> u32 { 320 }

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(value)
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}

// ─── Node kinds ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputNodeKind {
    Video,
    Audio,
    Image,
    VideoArray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessNodeKind {
    ExtractAudio,
    DetectSilence,
    DetectSubtitles,
    SpeechBounds,
    TrimAudio,
    TrimVideo,
    Scalar,
    Spline,
    Clip,
    Mux,
    MathAdd,
    MathSubtract,
    MathMultiply,
    MathDivide,
    Map,
    SubgraphInput,
    SubgraphOutput,
    Reduce,
    AssBuilder,
    #[serde(alias = "PhraseSelector", alias = "SpellCheck")]
    SubtitlePiece,
    Overlay,
    RemoveBackground,
    ResizeImage,
    AddBorder,
    SubtitleTrack,
    NamedInput,
    NamedOutput,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReduceOp {
    ConcatVideo,
    Sum,
    Collect,
}

// ─── Spline types (shared) ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Interpolation {
    #[default]
    Linear,
    CatmullRom,
    Step,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplineKeyframe {
    pub t: f64,
    pub value: f64,
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Input(InputNodeKind),
    Process(ProcessNodeKind),
    Reference { source: Uuid },
}

impl InputNodeKind {
    pub fn url_slug(&self) -> &'static str {
        match self {
            InputNodeKind::Video => "video-input",
            InputNodeKind::Audio => "audio-input",
            InputNodeKind::Image => "image-input",
            InputNodeKind::VideoArray => "video-array",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "video-input" => Some(InputNodeKind::Video),
            "audio-input" => Some(InputNodeKind::Audio),
            "image-input" => Some(InputNodeKind::Image),
            "video-array" => Some(InputNodeKind::VideoArray),
            _ => None,
        }
    }
}

impl ProcessNodeKind {
    pub fn url_slug(&self) -> &'static str {
        match self {
            ProcessNodeKind::ExtractAudio => "extract-audio",
            ProcessNodeKind::DetectSilence => "detect-silence",
            ProcessNodeKind::DetectSubtitles => "detect-subtitles",
            ProcessNodeKind::SpeechBounds => "speech-bounds",
            ProcessNodeKind::TrimAudio => "trim-audio",
            ProcessNodeKind::TrimVideo => "trim-video",
            ProcessNodeKind::Scalar => "scalar",
            ProcessNodeKind::Spline => "spline",
            ProcessNodeKind::Clip => "clip",
            ProcessNodeKind::Mux => "mux",
            ProcessNodeKind::MathAdd => "math-add",
            ProcessNodeKind::MathSubtract => "math-sub",
            ProcessNodeKind::MathMultiply => "math-mul",
            ProcessNodeKind::MathDivide => "math-div",
            ProcessNodeKind::Map => "map",
            ProcessNodeKind::SubgraphInput => "subgraph-input",
            ProcessNodeKind::SubgraphOutput => "subgraph-output",
            ProcessNodeKind::Reduce => "reduce",
            ProcessNodeKind::AssBuilder => "ass-builder",
            ProcessNodeKind::SubtitlePiece => "subtitle-piece",
            ProcessNodeKind::Overlay => "overlay",
            ProcessNodeKind::RemoveBackground => "remove-background",
            ProcessNodeKind::ResizeImage => "resize-image",
            ProcessNodeKind::AddBorder => "add-border",
            ProcessNodeKind::SubtitleTrack => "subtitle-track",
            ProcessNodeKind::NamedInput => "named-input",
            ProcessNodeKind::NamedOutput => "named-output",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "extract-audio" => Some(ProcessNodeKind::ExtractAudio),
            "detect-silence" => Some(ProcessNodeKind::DetectSilence),
            "detect-subtitles" => Some(ProcessNodeKind::DetectSubtitles),
            "speech-bounds" => Some(ProcessNodeKind::SpeechBounds),
            "trim-audio" => Some(ProcessNodeKind::TrimAudio),
            "trim-video" => Some(ProcessNodeKind::TrimVideo),
            "scalar" => Some(ProcessNodeKind::Scalar),
            "spline" => Some(ProcessNodeKind::Spline),
            "clip" => Some(ProcessNodeKind::Clip),
            "mux" => Some(ProcessNodeKind::Mux),
            "math-add" => Some(ProcessNodeKind::MathAdd),
            "math-sub" => Some(ProcessNodeKind::MathSubtract),
            "math-mul" => Some(ProcessNodeKind::MathMultiply),
            "math-div" => Some(ProcessNodeKind::MathDivide),
            "map" => Some(ProcessNodeKind::Map),
            "subgraph-input" => Some(ProcessNodeKind::SubgraphInput),
            "subgraph-output" => Some(ProcessNodeKind::SubgraphOutput),
            "reduce" => Some(ProcessNodeKind::Reduce),
            "ass-builder" => Some(ProcessNodeKind::AssBuilder),
            "subtitle-piece" => Some(ProcessNodeKind::SubtitlePiece),
            "overlay" => Some(ProcessNodeKind::Overlay),
            "remove-background" => Some(ProcessNodeKind::RemoveBackground),
            "resize-image" => Some(ProcessNodeKind::ResizeImage),
            "add-border" => Some(ProcessNodeKind::AddBorder),
            "subtitle-track" => Some(ProcessNodeKind::SubtitleTrack),
            "named-input" => Some(ProcessNodeKind::NamedInput),
            "named-output" => Some(ProcessNodeKind::NamedOutput),
            _ => None,
        }
    }

    pub fn accepted_input(&self) -> PortType {
        match self {
            ProcessNodeKind::ExtractAudio => PortType::Video,
            ProcessNodeKind::DetectSilence => PortType::Audio,
            ProcessNodeKind::DetectSubtitles => PortType::Audio,
            ProcessNodeKind::SpeechBounds => PortType::Audio,
            ProcessNodeKind::TrimAudio => PortType::Audio,
            ProcessNodeKind::TrimVideo => PortType::Video,
            ProcessNodeKind::Scalar => PortType::Number,
            ProcessNodeKind::Spline => PortType::Number,
            ProcessNodeKind::Clip => PortType::Video,
            ProcessNodeKind::Mux => PortType::Video,
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => PortType::Number,
            ProcessNodeKind::Map => PortType::Number,
            ProcessNodeKind::SubgraphInput => PortType::Number,
            ProcessNodeKind::SubgraphOutput => PortType::Number,
            ProcessNodeKind::Reduce => PortType::Number,
            ProcessNodeKind::AssBuilder => PortType::SubtitleSegments,
            ProcessNodeKind::SubtitlePiece => PortType::SubtitleSegments,
            ProcessNodeKind::Overlay => PortType::Image,
            ProcessNodeKind::RemoveBackground | ProcessNodeKind::ResizeImage
            | ProcessNodeKind::AddBorder => PortType::Image,
            ProcessNodeKind::SubtitleTrack => PortType::SubtitleSegments,
            ProcessNodeKind::NamedInput | ProcessNodeKind::NamedOutput => PortType::Number,
        }
    }

    pub fn produced_output(&self) -> PortType {
        match self {
            ProcessNodeKind::ExtractAudio => PortType::Audio,
            ProcessNodeKind::DetectSilence => PortType::Number, // silence intervals
            ProcessNodeKind::DetectSubtitles => PortType::SubtitleSegments,
            ProcessNodeKind::SpeechBounds => PortType::Number,
            ProcessNodeKind::TrimAudio => PortType::Audio,
            ProcessNodeKind::TrimVideo => PortType::Video,
            ProcessNodeKind::Scalar => PortType::Number,
            ProcessNodeKind::Spline => PortType::Number,
            ProcessNodeKind::Clip => PortType::ClipDescriptor,
            ProcessNodeKind::Mux => PortType::Video,
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => PortType::Number,
            ProcessNodeKind::Map => PortType::Number,
            ProcessNodeKind::SubgraphInput => PortType::Number,
            ProcessNodeKind::SubgraphOutput => PortType::Number,
            ProcessNodeKind::Reduce => PortType::Number,
            ProcessNodeKind::AssBuilder => PortType::AssSubtitles,
            ProcessNodeKind::SubtitlePiece => PortType::SubtitleSegments,
            ProcessNodeKind::Overlay => PortType::ClipDescriptor,
            ProcessNodeKind::RemoveBackground | ProcessNodeKind::ResizeImage
            | ProcessNodeKind::AddBorder => PortType::Image,
            ProcessNodeKind::SubtitleTrack => PortType::AssSubtitles,
            ProcessNodeKind::NamedInput | ProcessNodeKind::NamedOutput => PortType::Number,
        }
    }

    pub fn has_inputs(&self) -> bool {
        !matches!(self, ProcessNodeKind::Scalar | ProcessNodeKind::Spline | ProcessNodeKind::SubgraphInput | ProcessNodeKind::NamedOutput)
    }

    pub fn allows_multi_connect(&self, port: &str) -> bool {
        matches!((self, port),
            (ProcessNodeKind::Overlay, "times") |
            (ProcessNodeKind::Clip, "times") |
            (ProcessNodeKind::Mux, "clips") |
            (ProcessNodeKind::SubtitleTrack, "subs")
        )
    }
}

/// Per-node-type settings that affect processing output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeSettings {
    ExtractAudio,
    DetectSilence { noise_db: f64 },
    DetectSubtitles { model: String, #[serde(default)] corrected_content: String },
    SpeechBounds {
        threshold_mul: f64,
        onset_windows: u32,
        offset_windows: u32,
        window_ms: u32,
    },
    TrimAudio,
    TrimVideo,
    Scalar { value: f64 },
    Spline { keyframes: Vec<SplineKeyframe> },
    Clip {
        trim_start_ms: f64,
        trim_end_ms: f64,
        #[serde(default)]
        time_in: f64,
        #[serde(default = "default_one")]
        time_out: f64,
        #[serde(default = "default_preview_width")]
        preview_width: u32,
        #[serde(default)]
        keyframes: Vec<OverlayKeyframe>,
    },
    Mux { num_clips: u32, fps: u32 },
    MathOp,
    Map,
    SubgraphInput { output_kind: NodeOutputKind },
    SubgraphOutput { name: String },
    Reduce { operation: ReduceOp },
    AssBuilder { titles: Vec<AssTitle> },
    #[serde(alias = "PhraseSelector")]
    SubtitlePiece { phrase: String, occurrence: u32 },
    /// Legacy — kept only for deserializing old graph.toml files.
    SpellCheck { content: String },
    Overlay { keyframes: Vec<OverlayKeyframe> },
    RemoveBackground { prompt: String },
    ResizeImage { width: u32, height: u32 },
    AddBorder { color: String, border_width: u32 },
    SubtitleTrack {
        styles: Vec<SubtitleStyle>,
        #[serde(default)]
        segments: Vec<SubtitleSegment>,
        #[serde(default = "default_res")]
        resolution_x: u32,
        #[serde(default = "default_res")]
        resolution_y: u32,
        #[serde(default = "default_fps")]
        fps: u32,
    },
    NamedInput { name: String },
    NamedOutput { names: Vec<String> },
}

fn default_res() -> u32 { 1920 }
fn default_fps() -> u32 { 30 }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssTitle {
    pub text: String,
    pub font: String,
    pub size: u32,
    pub color: String,
    pub time_in_ms: f64,
    pub time_out_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayKeyframe {
    pub t_ms: f64,
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_one")]
    pub scale: f64,
    #[serde(default = "default_one")]
    pub alpha: f64,
    #[serde(default)]
    pub corner_radius: f64,
    #[serde(default)]
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleStyle {
    pub name: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_sub_size")]
    pub size: u32,
    #[serde(default = "default_white")]
    pub color: String,
    #[serde(default = "default_black")]
    pub outline_color: String,
    #[serde(default = "default_outline_w")]
    pub outline_width: u32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default = "default_alignment")]
    pub alignment: u8,
    #[serde(default = "default_margin_v")]
    pub margin_v: u32,
}

fn default_font() -> String { "Arial".to_string() }
fn default_sub_size() -> u32 { 48 }
fn default_white() -> String { "#FFFFFF".to_string() }
fn default_black() -> String { "#000000".to_string() }
fn default_outline_w() -> u32 { 2 }
fn default_alignment() -> u8 { 2 }
fn default_margin_v() -> u32 { 30 }

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            font: default_font(), size: default_sub_size(),
            color: default_white(), outline_color: default_black(),
            outline_width: default_outline_w(),
            bold: false, italic: false,
            alignment: default_alignment(), margin_v: default_margin_v(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleSegment {
    pub text: String,
    pub start_ms: f64,
    pub end_ms: f64,
    #[serde(default)]
    pub track: u32,
    #[serde(default)]
    pub style_name: Option<String>,
    #[serde(default = "default_pos_x")]
    pub pos_x: f64,
    #[serde(default = "default_pos_y")]
    pub pos_y: f64,
}

fn default_pos_x() -> f64 { 0.5 }
fn default_pos_y() -> f64 { 0.9 }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleSegmentOverride {
    pub index: usize,
    #[serde(default)]
    pub start_ms: Option<f64>,
    #[serde(default)]
    pub end_ms: Option<f64>,
    #[serde(default)]
    pub style_name: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub pos_x: Option<f64>,
    #[serde(default)]
    pub pos_y: Option<f64>,
    #[serde(default)]
    pub track: u32,
}

impl NodeSettings {
    pub fn default_for(kind: ProcessNodeKind) -> Self {
        match kind {
            ProcessNodeKind::ExtractAudio => NodeSettings::ExtractAudio,
            ProcessNodeKind::DetectSilence => NodeSettings::DetectSilence { noise_db: -30.0 },
            ProcessNodeKind::DetectSubtitles => NodeSettings::DetectSubtitles {
                model: "small".to_string(),
                corrected_content: String::new(),
            },
            ProcessNodeKind::SpeechBounds => NodeSettings::SpeechBounds {
                threshold_mul: 8.0,
                onset_windows: 3,
                offset_windows: 15, // 150ms — filters out clicks
                window_ms: 10,
            },
            ProcessNodeKind::TrimAudio => NodeSettings::TrimAudio,
            ProcessNodeKind::TrimVideo => NodeSettings::TrimVideo,
            ProcessNodeKind::Scalar => NodeSettings::Scalar { value: 0.0 },
            ProcessNodeKind::Spline => NodeSettings::Spline {
                keyframes: vec![
                    SplineKeyframe { t: 0.0, value: 0.0, interpolation: Interpolation::Linear },
                    SplineKeyframe { t: 1.0, value: 1.0, interpolation: Interpolation::Linear },
                ],
            },
            ProcessNodeKind::Clip => NodeSettings::Clip {
                trim_start_ms: 0.0,
                trim_end_ms: 0.0,
                time_in: 0.0,
                time_out: 1.0,
                preview_width: 320,
                keyframes: Vec::new(),
            },
            ProcessNodeKind::Mux => NodeSettings::Mux { num_clips: 1, fps: 30 },
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => NodeSettings::MathOp,
            ProcessNodeKind::Map => NodeSettings::Map,
            ProcessNodeKind::SubgraphInput => NodeSettings::SubgraphInput { output_kind: NodeOutputKind::Video },
            ProcessNodeKind::SubgraphOutput => NodeSettings::SubgraphOutput { name: "output".to_string() },
            ProcessNodeKind::Reduce => NodeSettings::Reduce { operation: ReduceOp::Collect },
            ProcessNodeKind::AssBuilder => NodeSettings::AssBuilder { titles: Vec::new() },
            ProcessNodeKind::SubtitlePiece => NodeSettings::SubtitlePiece {
                phrase: String::new(),
                occurrence: 0,
            },
            ProcessNodeKind::Overlay => NodeSettings::Overlay { keyframes: Vec::new() },
            ProcessNodeKind::RemoveBackground => NodeSettings::RemoveBackground { prompt: String::new() },
            ProcessNodeKind::ResizeImage => NodeSettings::ResizeImage { width: 1920, height: 1080 },
            ProcessNodeKind::AddBorder => NodeSettings::AddBorder { color: "#FFFFFF".to_string(), border_width: 5 },
            ProcessNodeKind::NamedInput => NodeSettings::NamedInput { name: "default".to_string() },
            ProcessNodeKind::NamedOutput => NodeSettings::NamedOutput { names: Vec::new() },
            ProcessNodeKind::SubtitleTrack => NodeSettings::SubtitleTrack {
                styles: vec![SubtitleStyle::default()],
                segments: Vec::new(),
                resolution_x: 1920,
                resolution_y: 1080,
                fps: 30,
            },
        }
    }

    pub fn cache_fingerprint(&self) -> String {
        match self {
            NodeSettings::ExtractAudio => "extract-audio".to_string(),
            NodeSettings::DetectSilence { noise_db } => format!("detect-silence:noise={noise_db}"),
            NodeSettings::DetectSubtitles { model, corrected_content } => {
                let mut h: u64 = 0;
                for b in corrected_content.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                format!("detect-subtitles:model={model}:cc={h:x}")
            }
            NodeSettings::SpeechBounds {
                threshold_mul,
                onset_windows,
                offset_windows,
                window_ms,
            } => format!("speech-bounds:t={threshold_mul}:on={onset_windows}:off={offset_windows}:w={window_ms}"),
            NodeSettings::TrimAudio => "trim-audio".to_string(),
            NodeSettings::TrimVideo => "trim-video".to_string(),
            NodeSettings::Scalar { value } => format!("scalar:{value}"),
            NodeSettings::Spline { keyframes } => {
                let mut h: u64 = 0;
                for kf in keyframes {
                    h = h.wrapping_mul(31).wrapping_add(kf.t.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.value.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.interpolation as u64);
                }
                format!("spline:{h:x}")
            }
            NodeSettings::Clip { trim_start_ms, trim_end_ms, time_in, time_out, preview_width, keyframes } => {
                let mut h: u64 = 0;
                for kf in keyframes {
                    h = h.wrapping_mul(31).wrapping_add(kf.t_ms.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.x.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.y.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.scale.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.alpha.to_bits());
                }
                format!("clip:s={trim_start_ms}:e={trim_end_ms}:in={time_in}:out={time_out}:pw={preview_width}:kf={h:x}")
            }
            NodeSettings::Mux { num_clips, fps } => {
                format!("mux:n={num_clips}:fps={fps}")
            }
            NodeSettings::MathOp => "math".to_string(),
            NodeSettings::Map => "map".to_string(),
            NodeSettings::SubgraphInput { output_kind } => format!("subgraph-input:{:?}", output_kind),
            NodeSettings::SubgraphOutput { name } => format!("subgraph-output:{name}"),
            NodeSettings::Reduce { operation } => format!("reduce:{:?}", operation),
            NodeSettings::AssBuilder { titles } => {
                let mut h: u64 = 0;
                for t in titles {
                    for b in t.text.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                    h = h.wrapping_mul(31).wrapping_add(t.time_in_ms.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(t.time_out_ms.to_bits());
                }
                format!("ass-builder:{:x}:{}", h, titles.len())
            }
            NodeSettings::SubtitlePiece { phrase, occurrence } => {
                format!("subtitle-piece:v2:{}:{}", phrase, occurrence)
            }
            NodeSettings::SpellCheck { content } => {
                let mut h: u64 = 0;
                for b in content.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                format!("spell-check:{:x}", h)
            }
            NodeSettings::Overlay { keyframes } => {
                let mut h: u64 = 0;
                for kf in keyframes {
                    h = h.wrapping_mul(31).wrapping_add(kf.t_ms.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.x.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.y.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.scale.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.alpha.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(kf.corner_radius.to_bits());
                }
                format!("overlay:{:x}:{}", h, keyframes.len())
            }
            NodeSettings::RemoveBackground { prompt } => format!("remove-bg:{prompt}"),
            NodeSettings::ResizeImage { width, height } => format!("resize:{width}x{height}"),
            NodeSettings::AddBorder { color, border_width } => format!("add-border:{color}:{border_width}"),
            NodeSettings::SubtitleTrack { styles, segments, resolution_x, resolution_y, fps } => {
                let mut h: u64 = 0;
                for s in styles {
                    for b in s.name.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                    h = h.wrapping_mul(31).wrapping_add(s.size as u64);
                    for b in s.color.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                    h = h.wrapping_mul(31).wrapping_add(s.bold as u64);
                }
                for seg in segments {
                    for b in seg.text.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                    h = h.wrapping_mul(31).wrapping_add(seg.start_ms.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(seg.end_ms.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(seg.track as u64);
                    h = h.wrapping_mul(31).wrapping_add(seg.pos_x.to_bits());
                    h = h.wrapping_mul(31).wrapping_add(seg.pos_y.to_bits());
                }
                format!("sub-track:{:x}:{}x{}", h, resolution_x, resolution_y)
            }
            NodeSettings::NamedInput { name } => format!("named-input:{name}"),
            NodeSettings::NamedOutput { names } => format!("named-output:{}", names.join(",")),
        }
    }
}

/// Describes the semantic type of a node's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortType {
    Number,
    Video,
    Audio,
    Image,
    SubtitleSegments,
    ClipDescriptor,
    AssSubtitles,
}

// Keep old name as alias for backward compatibility during migration
pub type NodeOutputKind = PortType;

impl NodeKind {
    pub fn output_ports(&self) -> Vec<PortDef> {
        match self {
            NodeKind::Input(InputNodeKind::Video) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Video },
                PortDef { name: "duration".into(), kind: PortType::Number },
                PortDef { name: "width".into(), kind: PortType::Number },
                PortDef { name: "height".into(), kind: PortType::Number },
            ],
            NodeKind::Input(InputNodeKind::Audio) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Audio },
                PortDef { name: "duration".into(), kind: PortType::Number },
            ],
            NodeKind::Input(InputNodeKind::Image) => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Image },
                PortDef { name: "width".into(), kind: PortType::Number },
                PortDef { name: "height".into(), kind: PortType::Number },
            ],
            NodeKind::Input(InputNodeKind::VideoArray) => vec![
                PortDef { name: String::new(), kind: PortType::Number },
            ],
            NodeKind::Process(pk) => pk.output_ports(),
            NodeKind::Reference { .. } => vec![], // resolved dynamically via source node
        }
    }

    pub fn output_ports_in_graph(&self, nodes: &[Node]) -> Vec<PortDef> {
        match self {
            NodeKind::Reference { source } => {
                resolve_reference(nodes, *source)
                    .map(|n| n.kind.output_ports())
                    .unwrap_or_default()
            }
            other => other.output_ports(),
        }
    }

    pub fn produced_output(&self) -> NodeOutputKind {
        match self {
            NodeKind::Input(InputNodeKind::Video) => NodeOutputKind::Video,
            NodeKind::Input(InputNodeKind::Audio) => NodeOutputKind::Audio,
            NodeKind::Input(InputNodeKind::Image) => NodeOutputKind::Image,
            NodeKind::Input(InputNodeKind::VideoArray) => PortType::Number,
            NodeKind::Process(p) => p.produced_output(),
            NodeKind::Reference { .. } => PortType::Number, // resolved dynamically
        }
    }
}

/// Resolve a reference chain to the actual (non-reference) node.
pub fn resolve_reference(nodes: &[Node], source: Uuid) -> Option<&Node> {
    let mut current = source;
    for _ in 0..20 {
        let node = nodes.iter().find(|n| n.id == current)?;
        match node.kind {
            NodeKind::Reference { source: next } => current = next,
            _ => return Some(node),
        }
    }
    None
}

// ─── Position ───

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

// ─── Asset (uploaded file) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: Uuid,
    pub kind: InputNodeKind,
    pub original_name: String,
    pub mime: String,
    pub size_bytes: u64,
    pub has_thumbnail: bool,
    pub has_waveform: bool,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

// ─── NodeOutput (processing result) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeOutput {
    pub file_name: String,
    pub mime: String,
    pub size_bytes: u64,
    pub cache_key: String,
    #[serde(default)]
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

// ─── SubGraph (for Map nodes) ───

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubGraph {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

// ─── Edge ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from_node: Uuid,
    #[serde(default)]
    pub from_port: String,
    pub to_node: Uuid,
    #[serde(default)]
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDef {
    pub name: String,
    pub kind: NodeOutputKind,
}

impl ProcessNodeKind {
    pub fn output_ports(&self) -> Vec<PortDef> {
        match self {
            ProcessNodeKind::SpeechBounds => vec![
                PortDef { name: "start".into(), kind: PortType::Number },
                PortDef { name: "end".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::ExtractAudio | ProcessNodeKind::TrimAudio => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Audio },
                PortDef { name: "duration".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::TrimVideo => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Video },
                PortDef { name: "duration".into(), kind: PortType::Number },
                PortDef { name: "width".into(), kind: PortType::Number },
                PortDef { name: "height".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::DetectSilence => vec![
                PortDef { name: String::new(), kind: PortType::Number },
            ],
            ProcessNodeKind::DetectSubtitles => vec![
                PortDef { name: String::new(), kind: PortType::SubtitleSegments },
            ],
            ProcessNodeKind::Scalar | ProcessNodeKind::Spline
            | ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide
            | ProcessNodeKind::SubgraphInput | ProcessNodeKind::SubgraphOutput
            | ProcessNodeKind::Reduce | ProcessNodeKind::NamedInput => vec![
                PortDef { name: String::new(), kind: PortType::Number },
            ],
            ProcessNodeKind::Clip | ProcessNodeKind::Overlay => vec![
                PortDef { name: String::new(), kind: PortType::ClipDescriptor },
            ],
            ProcessNodeKind::AssBuilder | ProcessNodeKind::SubtitleTrack => vec![
                PortDef { name: String::new(), kind: PortType::AssSubtitles },
            ],
            ProcessNodeKind::Mux => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Video },
            ],
            ProcessNodeKind::RemoveBackground | ProcessNodeKind::ResizeImage
            | ProcessNodeKind::AddBorder => vec![
                PortDef { name: String::new(), kind: NodeOutputKind::Image },
            ],
            ProcessNodeKind::SubtitlePiece => vec![
                PortDef { name: "start".into(), kind: PortType::Number },
                PortDef { name: "end".into(), kind: PortType::Number },
                PortDef { name: "segments".into(), kind: PortType::SubtitleSegments },
            ],
            ProcessNodeKind::Map => vec![
                PortDef { name: String::new(), kind: PortType::Number },
            ],
            ProcessNodeKind::NamedOutput => vec![], // dynamic, based on settings
        }
    }

    pub fn output_ports_with_settings(&self, settings: Option<&NodeSettings>) -> Vec<PortDef> {
        match self {
            ProcessNodeKind::NamedOutput => {
                if let Some(NodeSettings::NamedOutput { names }) = settings {
                    names.iter().map(|n| PortDef { name: n.clone(), kind: PortType::Number }).collect()
                } else {
                    vec![]
                }
            }
            _ => self.output_ports(),
        }
    }

    /// Port names that MUST be connected for the node to run.
    pub fn required_input_ports(&self) -> Vec<String> {
        match self {
            ProcessNodeKind::Scalar | ProcessNodeKind::Spline => vec![],
            ProcessNodeKind::TrimAudio => vec!["audio".into(), "start".into(), "end".into()],
            ProcessNodeKind::TrimVideo => vec!["video".into(), "start".into(), "end".into()],
            ProcessNodeKind::Clip => vec!["media".into(), "times".into()],
            ProcessNodeKind::Mux => vec!["duration".into(), "width".into(), "height".into()],
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => vec!["a".into(), "b".into()],
            ProcessNodeKind::Map => vec!["input".into()],
            ProcessNodeKind::SubgraphInput => vec![],
            ProcessNodeKind::SubgraphOutput => vec![String::new()],
            ProcessNodeKind::Reduce => vec!["array".into()],
            ProcessNodeKind::AssBuilder => vec!["subtitles".into()],
            ProcessNodeKind::SubtitlePiece => vec!["subtitles".into()],
            ProcessNodeKind::Overlay => vec!["image".into()],
            ProcessNodeKind::SubtitleTrack => vec!["subs".into()],
            _ => vec![String::new()],
        }
    }

    pub fn input_ports(&self) -> Vec<PortDef> {
        self.input_ports_with_settings(None)
    }

    pub fn input_ports_with_settings(&self, settings: Option<&NodeSettings>) -> Vec<PortDef> {
        match self {
            ProcessNodeKind::Scalar | ProcessNodeKind::Spline | ProcessNodeKind::NamedOutput => vec![],
            ProcessNodeKind::TrimAudio => vec![
                PortDef { name: "audio".into(), kind: NodeOutputKind::Audio },
                PortDef { name: "start".into(), kind: PortType::Number },
                PortDef { name: "end".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::TrimVideo => vec![
                PortDef { name: "video".into(), kind: NodeOutputKind::Video },
                PortDef { name: "start".into(), kind: PortType::Number },
                PortDef { name: "end".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
            | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => vec![
                PortDef { name: "a".into(), kind: PortType::Number },
                PortDef { name: "b".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::Map => vec![
                PortDef { name: "input".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::SubgraphInput => vec![],
            ProcessNodeKind::SubgraphOutput => vec![
                PortDef { name: String::new(), kind: PortType::Number },
            ],
            ProcessNodeKind::AssBuilder => vec![
                PortDef { name: "subtitles".into(), kind: PortType::AssSubtitles },
            ],
            ProcessNodeKind::SubtitlePiece => vec![
                PortDef { name: "subtitles".into(), kind: PortType::AssSubtitles },
            ],
            ProcessNodeKind::Overlay => vec![
                PortDef { name: "image".into(), kind: NodeOutputKind::Image },
                PortDef { name: "times".into(), kind: PortType::Number },
                PortDef { name: "background".into(), kind: NodeOutputKind::Video },
            ],
            ProcessNodeKind::SubtitleTrack => vec![
                PortDef { name: "subs".into(), kind: PortType::Number },
                PortDef { name: "video".into(), kind: NodeOutputKind::Video },
            ],
            ProcessNodeKind::Reduce => vec![
                PortDef { name: "array".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::Clip => vec![
                PortDef { name: "media".into(), kind: NodeOutputKind::Video },
                PortDef { name: "times".into(), kind: PortType::Number },
            ],
            ProcessNodeKind::Mux => vec![
                PortDef { name: "duration".into(), kind: PortType::Number },
                PortDef { name: "width".into(), kind: PortType::Number },
                PortDef { name: "height".into(), kind: PortType::Number },
                PortDef { name: "clips".into(), kind: PortType::ClipDescriptor },
                PortDef { name: "subtitles".into(), kind: PortType::AssSubtitles },
            ],
            _ => vec![PortDef {
                name: String::new(),
                kind: self.accepted_input(),
            }],
        }
    }
}

// ─── Task status ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Queued,
    Running { progress_pct: u8 },
    Done,
    Failed,
}

// ─── Node ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
    pub position: Position,
    #[serde(default)]
    pub asset: Option<Asset>,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub output: Option<NodeOutput>,
    #[serde(default)]
    pub settings: Option<NodeSettings>,
    #[serde(default)]
    pub subgraph: Option<Box<SubGraph>>,
    #[serde(default)]
    pub task_status: Option<TaskStatus>,
    #[serde(default)]
    pub needs_update: bool,
}

// ─── Project ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub project: ProjectSummary,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

// ─── Project CRUD DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProjectsOutput {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectInput {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectOutput {
    pub project: ProjectSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteProjectInput {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteProjectOutput {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameProjectInput {
    pub id: Uuid,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameProjectOutput {
    pub project: ProjectSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectInput {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectOutput {
    pub project: ProjectDetail,
}

// ─── Node CRUD DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeInput {
    pub project_id: Uuid,
    pub kind: NodeKind,
    pub position: Position,
    /// If Some, create inside this Map node's subgraph
    #[serde(default)]
    pub parent_map_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeOutput {
    pub node: Node,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteNodeInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    #[serde(default)]
    pub parent_map_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteNodeOutput {
    pub node_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodePositionInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub position: Position,
    #[serde(default)]
    pub parent_map_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodePositionOutput {
    pub node: Node,
}

// ─── Edge DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectNodesInput {
    pub project_id: Uuid,
    pub from_node: Uuid,
    #[serde(default)]
    pub from_port: String,
    pub to_node: Uuid,
    #[serde(default)]
    pub to_port: String,
    #[serde(default)]
    pub parent_map_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectNodesOutput {
    pub edge: Edge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectNodesInput {
    pub project_id: Uuid,
    pub from_node: Uuid,
    #[serde(default)]
    pub from_port: String,
    pub to_node: Uuid,
    #[serde(default)]
    pub to_port: String,
    #[serde(default)]
    pub parent_map_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectNodesOutput {}

// ─── Node settings DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodeSettingsInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub settings: NodeSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNodeSettingsOutput {
    pub node: Node,
}

// ─── Task DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunNodeInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunNodeOutput {
    pub task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusInput {
    pub task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusOutput {
    pub task_id: Uuid,
    pub node_id: Uuid,
    pub status: TaskStatus,
    pub error_message: Option<String>,
}

// ─── Upload DTOs ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBeginInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub kind: InputNodeKind,
    pub original_name: String,
    pub mime: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBeginOutput {
    pub upload_id: Uuid,
    pub chunk_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadChunkOutput {
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadFinalizeInput {
    pub project_id: Uuid,
    pub node_id: Uuid,
    pub upload_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadFinalizeOutput {
    pub node: Node,
}

// ─── API envelope ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiResponseEnvelope<T> {
    Ok(T),
    Err(ApiError),
}
