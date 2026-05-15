use std::path::Path;
use std::io::Write;

use image::{imageops, DynamicImage, Rgba, RgbaImage};
use tokio::io::AsyncWriteExt;

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

/// Generate a rounded-rect alpha mask PNG (white on black).
/// Returns PNG bytes. Used by Mux to apply corner radius via alphamerge.
pub fn generate_rounded_mask_png(w: u32, h: u32, radius_px: u32) -> Vec<u8> {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
    if radius_px > 0 {
        let r = radius_px as i64;
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = if (x as i64) < r && (y as i64) < r {
                    (r - x as i64, r - y as i64)
                } else if x as i64 >= w as i64 - r && (y as i64) < r {
                    (x as i64 - (w as i64 - r - 1), r - y as i64)
                } else if (x as i64) < r && y as i64 >= h as i64 - r {
                    (r - x as i64, y as i64 - (h as i64 - r - 1))
                } else if x as i64 >= w as i64 - r && y as i64 >= h as i64 - r {
                    (x as i64 - (w as i64 - r - 1), y as i64 - (h as i64 - r - 1))
                } else {
                    continue;
                };
                let dist = ((dx * dx + dy * dy) as f64).sqrt();
                if dist > r as f64 {
                    img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
                } else if dist > r as f64 - 1.5 {
                    let v = ((r as f64 - dist) / 1.5 * 255.0).clamp(0.0, 255.0) as u8;
                    img.put_pixel(x, y, Rgba([v, v, v, 255]));
                }
            }
        }
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

/// Generate a border frame PNG: colored rounded rect with transparent center.
/// The border is `border_width` pixels thick.
pub fn generate_border_png(w: u32, h: u32, radius_px: u32, border_width: u32, color: [u8; 3]) -> Vec<u8> {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    let outer_r = radius_px as i64;
    let inner_r = (radius_px as i64 - border_width as i64).max(0);
    let bw = border_width as i64;

    for y in 0..h {
        for x in 0..w {
            let xi = x as i64;
            let yi = y as i64;
            let wi = w as i64;
            let hi = h as i64;

            // Check if pixel is inside outer rounded rect
            let in_outer = is_inside_rounded_rect(xi, yi, wi, hi, outer_r);
            if !in_outer { continue; }

            // Check if pixel is inside inner area (content zone)
            let in_inner = xi >= bw && yi >= bw && xi < wi - bw && yi < hi - bw
                && is_inside_rounded_rect(xi - bw, yi - bw, wi - 2 * bw, hi - 2 * bw, inner_r);

            if !in_inner {
                // Anti-alias outer edge
                let outer_alpha = edge_alpha(xi, yi, wi, hi, outer_r);
                let a = (outer_alpha * 255.0) as u8;
                img.put_pixel(x, y, Rgba([color[0], color[1], color[2], a]));
            }
        }
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn is_inside_rounded_rect(x: i64, y: i64, w: i64, h: i64, r: i64) -> bool {
    if r <= 0 { return x >= 0 && y >= 0 && x < w && y < h; }
    // Check corners
    let (dx, dy) = if x < r && y < r {
        (r - x, r - y)
    } else if x >= w - r && y < r {
        (x - (w - r - 1), r - y)
    } else if x < r && y >= h - r {
        (r - x, y - (h - r - 1))
    } else if x >= w - r && y >= h - r {
        (x - (w - r - 1), y - (h - r - 1))
    } else {
        return x >= 0 && y >= 0 && x < w && y < h;
    };
    ((dx * dx + dy * dy) as f64).sqrt() <= r as f64
}

fn edge_alpha(x: i64, y: i64, w: i64, h: i64, r: i64) -> f64 {
    if r <= 0 { return 1.0; }
    let (dx, dy) = if x < r && y < r {
        (r - x, r - y)
    } else if x >= w - r && y < r {
        (x - (w - r - 1), r - y)
    } else if x < r && y >= h - r {
        (r - x, y - (h - r - 1))
    } else if x >= w - r && y >= h - r {
        (x - (w - r - 1), y - (h - r - 1))
    } else {
        return 1.0;
    };
    let dist = ((dx * dx + dy * dy) as f64).sqrt();
    if dist > r as f64 { 0.0 }
    else if dist > r as f64 - 1.5 { ((r as f64 - dist) / 1.5).clamp(0.0, 1.0) }
    else { 1.0 }
}

/// Generate a mask video with animated corner radius.
/// Each frame is a grayscale image: white = visible, black = hidden.
/// The radius is interpolated from the keyframes spline.
/// Output: video file at `output_path`.
/// Pre-process a video: apply animated corner_radius + border per frame.
/// Reads source video via ffmpeg pipe, applies rounding/border in Rust, writes result via ffmpeg pipe.
/// Output is RGBA video with transparent corners.
pub async fn preprocess_clip_with_border(
    ffmpeg: &Ffmpeg,
    source_path: &Path,
    output_path: &Path,
    fps: u32,
    duration_secs: f64,
    radius_keyframes: &[(f64, f64, String)],
    border_keyframes: &[(f64, f64, String)],
    color_keyframes: &[(f64, [u8; 3], String)],
    trim_start_secs: f64,
) -> Result<(), String> {
    // Probe source dimensions
    let probe = ffmpeg.probe(source_path).await.map_err(|e| e.to_string())?;
    let src_w = probe.width.unwrap_or(640);
    let src_h = probe.height.unwrap_or(480);
    let frame_size = (src_w * src_h * 4) as usize; // RGBA

    // Decoder: source video → raw RGBA frames
    let mut decoder = tokio::process::Command::new(ffmpeg.binary());
    decoder.arg("-hide_banner").arg("-loglevel").arg("error");
    if trim_start_secs > 0.0 {
        decoder.arg("-ss").arg(format!("{:.3}", trim_start_secs));
    }
    decoder.arg("-i").arg(source_path)
        .arg("-t").arg(format!("{:.3}", duration_secs))
        .arg("-f").arg("rawvideo")
        .arg("-pix_fmt").arg("rgba")
        .arg("-r").arg(format!("{fps}"))
        .arg("pipe:1");
    decoder.stdout(std::process::Stdio::piped());
    decoder.stderr(std::process::Stdio::null());
    let mut dec_child = decoder.spawn().map_err(|e| e.to_string())?;
    let mut dec_stdout = dec_child.stdout.take().ok_or("Failed to open decoder stdout")?;

    // Encoder: raw RGBA frames → output video
    let mut encoder = tokio::process::Command::new(ffmpeg.binary());
    encoder.arg("-y").arg("-hide_banner").arg("-loglevel").arg("error")
        .arg("-f").arg("rawvideo")
        .arg("-pix_fmt").arg("rgba")
        .arg("-s").arg(format!("{src_w}x{src_h}"))
        .arg("-r").arg(format!("{fps}"))
        .arg("-i").arg("pipe:0")
        .arg("-c:v").arg("ffv1")
        .arg("-pix_fmt").arg("rgba")
        .arg(output_path);
    encoder.stdin(std::process::Stdio::piped());
    encoder.stdout(std::process::Stdio::null());
    encoder.stderr(std::process::Stdio::piped());
    let mut enc_child = encoder.spawn().map_err(|e| e.to_string())?;
    let mut enc_stdin = enc_child.stdin.take().ok_or("Failed to open encoder stdin")?;

    let total_frames = (duration_secs * fps as f64).ceil() as u32;
    let mut frame_buf = vec![0u8; frame_size];

    use tokio::io::AsyncReadExt;
    for frame_idx in 0..total_frames {
        // Read one frame from decoder
        let mut read = 0;
        while read < frame_size {
            let n = dec_stdout.read(&mut frame_buf[read..]).await.map_err(|e| e.to_string())?;
            if n == 0 { break; }
            read += n;
        }
        if read < frame_size { break; } // EOF

        let t = if total_frames > 1 { frame_idx as f64 / (total_frames - 1) as f64 } else { 0.0 };
        let r_refs: Vec<(f64, f64, &str)> = radius_keyframes.iter().map(|(t, v, e)| (*t, *v, e.as_str())).collect();
        let b_refs: Vec<(f64, f64, &str)> = border_keyframes.iter().map(|(t, v, e)| (*t, *v, e.as_str())).collect();
        let c_refs: Vec<(f64, [u8; 3], &str)> = color_keyframes.iter().map(|(t, c, e)| (*t, *c, e.as_str())).collect();
        let radius = interpolate_keyframes(&r_refs, t);
        let bw = interpolate_keyframes(&b_refs, t) as u32;
        let border_color = interpolate_color(&c_refs, t);

        // Apply corner radius + border
        let mut img = RgbaImage::from_raw(src_w, src_h, frame_buf.clone())
            .ok_or("Failed to create image from raw frame")?;

        let outer_radius = radius + bw as f64;
        if outer_radius > 0.5 {
            // First apply outer rounding (makes corners transparent)
            apply_corner_radius_px(&mut img, outer_radius as i64);

            // Draw border: pixels between outer and inner radius
            if bw > 0 {
                let inner_r = radius.max(0.0) as i64;
                draw_border(&mut img, outer_radius as i64, inner_r, bw, border_color);
            }
        }

        enc_stdin.write_all(img.as_raw()).await.map_err(|e| e.to_string())?;
        frame_buf = img.into_raw();
    }

    drop(enc_stdin);
    drop(dec_stdout);
    let _ = dec_child.wait().await;
    let enc_output = enc_child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !enc_output.status.success() {
        let stderr = String::from_utf8_lossy(&enc_output.stderr);
        return Err(format!("Clip pre-processing failed: {stderr}"));
    }
    Ok(())
}

fn apply_corner_radius_px(img: &mut RgbaImage, r: i64) {
    if r <= 0 { return; }
    let w = img.width();
    let h = img.height();
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = if (x as i64) < r && (y as i64) < r {
                (r - x as i64, r - y as i64)
            } else if x as i64 >= w as i64 - r && (y as i64) < r {
                (x as i64 - (w as i64 - r - 1), r - y as i64)
            } else if (x as i64) < r && y as i64 >= h as i64 - r {
                (r - x as i64, y as i64 - (h as i64 - r - 1))
            } else if x as i64 >= w as i64 - r && y as i64 >= h as i64 - r {
                (x as i64 - (w as i64 - r - 1), y as i64 - (h as i64 - r - 1))
            } else {
                continue;
            };
            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            if dist > r as f64 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            } else if dist > r as f64 - 1.5 {
                let alpha = ((r as f64 - dist) / 1.5).clamp(0.0, 1.0);
                let p = img.get_pixel(x, y);
                img.put_pixel(x, y, Rgba([p[0], p[1], p[2], (p[3] as f64 * alpha) as u8]));
            }
        }
    }
}

fn draw_border(img: &mut RgbaImage, outer_r: i64, inner_r: i64, bw: u32, color: [u8; 3]) {
    let w = img.width();
    let h = img.height();
    let bw_i = bw as i64;
    for y in 0..h {
        for x in 0..w {
            let xi = x as i64;
            let yi = y as i64;
            let wi = w as i64;
            let hi = h as i64;

            // Must be inside outer rect
            let outer_a = corner_alpha(xi, yi, wi, hi, outer_r);
            if outer_a < 0.001 { continue; }

            // Check if in border zone (outside inner rect)
            let in_inner = xi >= bw_i && yi >= bw_i && xi < wi - bw_i && yi < hi - bw_i;
            if in_inner {
                let inner_a = corner_alpha(xi - bw_i, yi - bw_i, wi - 2 * bw_i, hi - 2 * bw_i, inner_r);
                if inner_a > 0.999 { continue; } // fully inside content
                // Anti-alias border/content boundary
                let border_a = 1.0 - inner_a;
                let p = img.get_pixel(x, y);
                let r = (color[0] as f64 * border_a + p[0] as f64 * inner_a) as u8;
                let g = (color[1] as f64 * border_a + p[1] as f64 * inner_a) as u8;
                let b = (color[2] as f64 * border_a + p[2] as f64 * inner_a) as u8;
                let a = (outer_a * 255.0) as u8;
                img.put_pixel(x, y, Rgba([r, g, b, a]));
            } else {
                // In border zone
                let a = (outer_a * 255.0) as u8;
                img.put_pixel(x, y, Rgba([color[0], color[1], color[2], a]));
            }
        }
    }
}

/// Generate a single RGBA frame: rounded rect with optional border.
/// White content area, colored border, transparent outside.
fn generate_mask_border_frame(w: u32, h: u32, outer_radius: u32, border_width: u32, border_color: [u8; 3]) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    let r = outer_radius as i64;
    let bw = border_width as i64;
    let inner_r = (r - bw).max(0);

    for y in 0..h {
        for x in 0..w {
            let xi = x as i64;
            let yi = y as i64;
            let wi = w as i64;
            let hi = h as i64;

            // Distance from nearest corner for outer rect
            let outer_alpha = corner_alpha(xi, yi, wi, hi, r);
            if outer_alpha < 0.001 { continue; } // outside outer rect

            // Check if in border zone
            let in_inner = xi >= bw && yi >= bw && xi < wi - bw && yi < hi - bw;
            if in_inner {
                let inner_alpha = corner_alpha(xi - bw, yi - bw, wi - 2 * bw, hi - 2 * bw, inner_r);
                if inner_alpha > 0.999 {
                    // Fully inside content area — white
                    let a = (outer_alpha * 255.0) as u8;
                    img.put_pixel(x, y, Rgba([255, 255, 255, a]));
                } else if inner_alpha > 0.001 {
                    // Anti-alias edge between border and content
                    let a = (outer_alpha * 255.0) as u8;
                    let br = border_color[0];
                    let bg = border_color[1];
                    let bb = border_color[2];
                    let cr = (255.0 * inner_alpha + br as f64 * (1.0 - inner_alpha)) as u8;
                    let cg = (255.0 * inner_alpha + bg as f64 * (1.0 - inner_alpha)) as u8;
                    let cb = (255.0 * inner_alpha + bb as f64 * (1.0 - inner_alpha)) as u8;
                    img.put_pixel(x, y, Rgba([cr, cg, cb, a]));
                } else {
                    // In border zone (inner corner)
                    let a = (outer_alpha * 255.0) as u8;
                    img.put_pixel(x, y, Rgba([border_color[0], border_color[1], border_color[2], a]));
                }
            } else {
                // In border zone (edges)
                let a = (outer_alpha * 255.0) as u8;
                img.put_pixel(x, y, Rgba([border_color[0], border_color[1], border_color[2], a]));
            }
        }
    }
    img
}

/// Returns alpha (0.0..1.0) for a pixel considering rounded corners.
fn corner_alpha(x: i64, y: i64, w: i64, h: i64, r: i64) -> f64 {
    if r <= 0 || w <= 0 || h <= 0 {
        return if x >= 0 && y >= 0 && x < w && y < h { 1.0 } else { 0.0 };
    }
    if x < 0 || y < 0 || x >= w || y >= h { return 0.0; }

    let (dx, dy) = if x < r && y < r {
        (r - x, r - y)
    } else if x >= w - r && y < r {
        (x - (w - r - 1), r - y)
    } else if x < r && y >= h - r {
        (r - x, y - (h - r - 1))
    } else if x >= w - r && y >= h - r {
        (x - (w - r - 1), y - (h - r - 1))
    } else {
        return 1.0; // not in a corner region
    };

    let dist = ((dx * dx + dy * dy) as f64).sqrt();
    if dist > r as f64 { 0.0 }
    else if dist > r as f64 - 1.5 { ((r as f64 - dist) / 1.5).clamp(0.0, 1.0) }
    else { 1.0 }
}

/// Apply easing function to linear fraction 0..1
fn apply_easing(frac: f64, easing: &str) -> f64 {
    match easing {
        "EaseIn" => frac * frac,
        "EaseOut" => 1.0 - (1.0 - frac) * (1.0 - frac),
        "EaseInOut" => {
            if frac < 0.5 { 2.0 * frac * frac }
            else { 1.0 - (-2.0 * frac + 2.0_f64).powi(2) / 2.0 }
        }
        "Step" => 0.0,
        _ => frac, // Linear, CatmullRom
    }
}

/// Interpolation with easing between keyframes. Tuple: (t, value, easing_name)
fn interpolate_keyframes(kfs: &[(f64, f64, &str)], t: f64) -> f64 {
    if kfs.is_empty() { return 0.0; }
    if kfs.len() == 1 { return kfs[0].1; }
    if t <= kfs[0].0 { return kfs[0].1; }
    if t >= kfs[kfs.len() - 1].0 { return kfs[kfs.len() - 1].1; }

    let mut i = 0;
    while i + 1 < kfs.len() && kfs[i + 1].0 < t { i += 1; }
    if i + 1 >= kfs.len() { return kfs[kfs.len() - 1].1; }

    let (t0, v0, easing) = kfs[i];
    let (t1, v1, _) = kfs[i + 1];
    let seg = t1 - t0;
    if seg <= 0.0 { return v0; }
    let frac = ((t - t0) / seg).clamp(0.0, 1.0);
    let eased = apply_easing(frac, easing);
    if easing == "Step" { v0 } else { v0 + (v1 - v0) * eased }
}

/// Interpolation of RGB color with easing. Tuple: (t, [r,g,b], easing_name)
fn interpolate_color(kfs: &[(f64, [u8; 3], &str)], t: f64) -> [u8; 3] {
    if kfs.is_empty() { return [255, 255, 255]; }
    if kfs.len() == 1 { return kfs[0].1; }
    if t <= kfs[0].0 { return kfs[0].1; }
    if t >= kfs[kfs.len() - 1].0 { return kfs[kfs.len() - 1].1; }

    let mut i = 0;
    while i + 1 < kfs.len() && kfs[i + 1].0 < t { i += 1; }
    if i + 1 >= kfs.len() { return kfs[kfs.len() - 1].1; }

    let (t0, c0, easing) = kfs[i];
    let (t1, c1, _) = kfs[i + 1];
    let seg = t1 - t0;
    if seg <= 0.0 { return c0; }
    let frac = ((t - t0) / seg).clamp(0.0, 1.0);
    let eased = apply_easing(frac, easing);
    [
        (c0[0] as f64 + (c1[0] as f64 - c0[0] as f64) * eased) as u8,
        (c0[1] as f64 + (c1[1] as f64 - c0[1] as f64) * eased) as u8,
        (c0[2] as f64 + (c1[2] as f64 - c0[2] as f64) * eased) as u8,
    ]
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
