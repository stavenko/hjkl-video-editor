pub mod helpers;
pub mod video_player;
pub mod modals;
pub mod asset_view;
pub mod subtitles_view;
pub mod overlay;
pub mod subtitle_track;

/// Canvas transform state shared between editor and helpers.
#[derive(Clone, Copy)]
pub struct CanvasTransform {
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale: f64,
}
