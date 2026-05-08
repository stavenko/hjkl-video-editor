use std::path::Path;

use image::{imageops, DynamicImage, Rgba, RgbaImage};

use crate::providers::ffmpeg::Ffmpeg;

/// Decoded clip properties at a given time
pub struct ClipFrame {
    pub image: DynamicImage,
    /// Center X in 0.0–1.0 of output
    pub x: f64,
    /// Center Y in 0.0–1.0 of output
    pub y: f64,
    /// Scale relative to output width
    pub scale: f64,
    /// Corner radius in 0.0–1.0 of min(clip_w, clip_h)
    pub corner_radius: f64,
}

/// Composite multiple clip frames onto a canvas
pub fn composite(width: u32, height: u32, clips: &[ClipFrame]) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    for clip in clips {
        if clip.scale <= 0.0 {
            continue;
        }

        let clip_w = (clip.image.width() as f64 * clip.scale) as u32;
        let clip_h = (clip.image.height() as f64 * clip.scale) as u32;
        if clip_w == 0 || clip_h == 0 {
            continue;
        }

        // Scale the clip — skip if already the right size
        let mut rgba = if clip_w == clip.image.width() && clip_h == clip.image.height() {
            clip.image.to_rgba8()
        } else {
            clip.image.resize_exact(clip_w, clip_h, imageops::FilterType::Nearest).into_rgba8()
        };

        // Apply corner radius
        if clip.corner_radius > 0.0 {
            apply_corner_radius(&mut rgba, clip.corner_radius);
        }

        // Position: x,y are center in 0–1 coords
        let cx = (clip.x * width as f64) as i64;
        let cy = (clip.y * height as f64) as i64;
        let ox = cx - clip_w as i64 / 2;
        let oy = cy - clip_h as i64 / 2;

        // Overlay with alpha
        overlay_at(&mut canvas, &rgba, ox, oy);
    }

    canvas
}

fn overlay_at(canvas: &mut RgbaImage, overlay: &RgbaImage, ox: i64, oy: i64) {
    for (x, y, pixel) in overlay.enumerate_pixels() {
        let cx = ox + x as i64;
        let cy = oy + y as i64;
        if cx < 0 || cy < 0 || cx >= canvas.width() as i64 || cy >= canvas.height() as i64 {
            continue;
        }
        let alpha = pixel[3] as f32 / 255.0;
        if alpha < 0.001 {
            continue;
        }
        let bg = canvas.get_pixel(cx as u32, cy as u32);
        let r = (pixel[0] as f32 * alpha + bg[0] as f32 * (1.0 - alpha)) as u8;
        let g = (pixel[1] as f32 * alpha + bg[1] as f32 * (1.0 - alpha)) as u8;
        let b = (pixel[2] as f32 * alpha + bg[2] as f32 * (1.0 - alpha)) as u8;
        canvas.put_pixel(cx as u32, cy as u32, Rgba([r, g, b, 255]));
    }
}

fn apply_corner_radius(img: &mut RgbaImage, radius_frac: f64) {
    let w = img.width();
    let h = img.height();
    let r = (radius_frac * w.min(h) as f64) as i64;
    if r <= 0 {
        return;
    }

    for y in 0..h {
        for x in 0..w {
            let dx;
            let dy;
            // Check which corner
            if (x as i64) < r && (y as i64) < r {
                dx = r - x as i64;
                dy = r - y as i64;
            } else if x as i64 >= w as i64 - r && (y as i64) < r {
                dx = x as i64 - (w as i64 - r - 1);
                dy = r - y as i64;
            } else if (x as i64) < r && y as i64 >= h as i64 - r {
                dx = r - x as i64;
                dy = y as i64 - (h as i64 - r - 1);
            } else if x as i64 >= w as i64 - r && y as i64 >= h as i64 - r {
                dx = x as i64 - (w as i64 - r - 1);
                dy = y as i64 - (h as i64 - r - 1);
            } else {
                continue;
            }

            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            if dist > r as f64 {
                let pixel = img.get_pixel_mut(x, y);
                pixel[3] = 0;
            } else if dist > (r as f64 - 1.5) {
                // Anti-alias
                let alpha = ((r as f64 - dist) / 1.5).clamp(0.0, 1.0);
                let pixel = img.get_pixel_mut(x, y);
                pixel[3] = (pixel[3] as f64 * alpha) as u8;
            }
        }
    }
}

/// Decode a single frame from a video at given time
pub async fn decode_frame(ffmpeg: &Ffmpeg, source: &Path, t_secs: f64) -> Result<DynamicImage, String> {
    let png_bytes = ffmpeg
        .generate_frame_at(source, t_secs)
        .await
        .map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&png_bytes).map_err(|e| e.to_string())?;
    Ok(img)
}

/// Load a static image
pub fn load_image(path: &Path) -> Result<DynamicImage, String> {
    image::open(path).map_err(|e| e.to_string())
}
