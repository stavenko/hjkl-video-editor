use api_types::{Interpolation, SplineKeyframe};

/// Evaluate a set of keyframes at time t (0.0–1.0).
/// Returns the interpolated value.
pub fn evaluate(keyframes: &[SplineKeyframe], t: f64) -> f64 {
    if keyframes.is_empty() {
        return 0.0;
    }
    if keyframes.len() == 1 {
        return keyframes[0].value;
    }

    // Clamp t
    let t = t.clamp(0.0, 1.0);

    // Before first keyframe
    if t <= keyframes[0].t {
        return keyframes[0].value;
    }
    // After last keyframe
    if t >= keyframes[keyframes.len() - 1].t {
        return keyframes[keyframes.len() - 1].value;
    }

    // Find segment
    let mut i = 0;
    while i + 1 < keyframes.len() && keyframes[i + 1].t < t {
        i += 1;
    }

    let kf0 = &keyframes[i];
    let kf1 = &keyframes[i + 1];
    let seg_t = if (kf1.t - kf0.t).abs() < 1e-12 {
        0.0
    } else {
        (t - kf0.t) / (kf1.t - kf0.t)
    };

    match kf0.interpolation {
        Interpolation::Step => kf0.value,
        Interpolation::Linear => lerp(kf0.value, kf1.value, seg_t),
        Interpolation::CatmullRom => {
            // Get 4 points for Catmull-Rom
            let p0 = if i > 0 {
                keyframes[i - 1].value
            } else {
                // Phantom point: reflect
                2.0 * kf0.value - kf1.value
            };
            let p1 = kf0.value;
            let p2 = kf1.value;
            let p3 = if i + 2 < keyframes.len() {
                keyframes[i + 2].value
            } else {
                // Phantom point: reflect
                2.0 * kf1.value - kf0.value
            };
            catmull_rom(p0, p1, p2, p3, seg_t)
        }
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}
