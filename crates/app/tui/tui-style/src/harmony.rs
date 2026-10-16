//! Pure palette-harmony helpers.
//!
//! This module derives related colors from a single seed color using HSV and
//! OKLab interpolation. It is stateless and never touches the terminal, so the
//! results are deterministic and safe to use as an optional overlay on top of
//! the existing theme palette. Nothing in the default theme calls these
//! functions, so default rendered output stays byte-for-byte identical.

use crate::theme::Rgb;

/// Convert 8-bit RGB to HSV. Hue is in degrees (0..360), saturation and
/// value in 0..1.
pub fn rgb_to_hsv(color: Rgb) -> (f32, f32, f32) {
    let r = f32::from(color.r) / 255.0;
    let g = f32::from(color.g) / 255.0;
    let b = f32::from(color.b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// Convert HSV back to 8-bit RGB. Hue is in degrees, saturation/value in 0..1.
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Rgb {
    let saturation = saturation.clamp(0.0, 1.0);
    let value = value.clamp(0.0, 1.0);
    let hue = hue.rem_euclid(360.0);
    let chroma = value * saturation;
    let x = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = value - chroma;
    let (r1, g1, b1) = match hue as u32 {
        0..=59 => (chroma, x, 0.0),
        60..=119 => (x, chroma, 0.0),
        120..=179 => (0.0, chroma, x),
        180..=239 => (0.0, x, chroma),
        240..=299 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    Rgb::new(
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// Interpolate two colors in OKLab space. `t` is clamped to 0..=1.
pub fn oklab_interpolate(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let la = crate::theme::oklab::rgb_to_oklab(a);
    let lb = crate::theme::oklab::rgb_to_oklab(b);
    let mixed = crate::theme::oklab::Oklab {
        l: la.l * (1.0 - t) + lb.l * t,
        a: la.a * (1.0 - t) + lb.a * t,
        b: la.b * (1.0 - t) + lb.b * t,
    };
    crate::theme::oklab::oklab_to_rgb(mixed)
}

/// Interpolate two colors in HSV space. `t` is clamped to 0..=1.
pub fn hsv_interpolate(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let (ha, sa, va) = rgb_to_hsv(a);
    let (hb, sb, vb) = rgb_to_hsv(b);
    // Shortest hue path.
    let mut dh = hb - ha;
    if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    let hue = ha + dh * t;
    let sat = sa * (1.0 - t) + sb * t;
    let val = va * (1.0 - t) + vb * t;
    hsv_to_rgb(hue, sat, val)
}

/// Complementary color (180 degree hue rotation) in HSV space.
pub fn complementary(color: Rgb) -> Rgb {
    let (h, s, v) = rgb_to_hsv(color);
    hsv_to_rgb(h + 180.0, s, v)
}

/// Two analogous neighbors at +/- the given hue offset.
pub fn analogous(color: Rgb, offset_degrees: f32) -> (Rgb, Rgb) {
    let (h, s, v) = rgb_to_hsv(color);
    (
        hsv_to_rgb(h + offset_degrees, s, v),
        hsv_to_rgb(h - offset_degrees, s, v),
    )
}

/// Derive a small accent-neighbor palette: complementary and two analogs.
pub fn accent_neighbors(color: Rgb) -> (Rgb, Rgb, Rgb) {
    let complementary = complementary(color);
    let (left, right) = analogous(color, 30.0);
    (complementary, left, right)
}

/// Build a panel gradient from `base` to a derived end color.
pub fn panel_gradient(base: Rgb, steps: usize) -> Vec<Rgb> {
    let steps = steps.max(1);
    let end = oklab_interpolate(base, Rgb::new(0, 0, 0), 0.35);
    (0..steps)
        .map(|i| {
            if steps == 1 {
                base
            } else {
                oklab_interpolate(base, end, i as f32 / (steps as f32 - 1.0))
            }
        })
        .collect()
}

/// Derive upgraded warning/error colors from a base accent.
pub fn escalate(base: Rgb) -> (Rgb, Rgb) {
    let (h, s, v) = rgb_to_hsv(base);
    let warning = hsv_to_rgb(h.clamp(30.0, 60.0), s.max(0.6), v.max(0.7));
    let error = hsv_to_rgb(0.0, s.max(0.7), v.max(0.6));
    (warning, error)
}

// ---------- aliases for alternate naming conventions ----------

pub fn rgb_to_hsv_color(color: Rgb) -> (f32, f32, f32) {
    rgb_to_hsv(color)
}
pub fn hsv_to_rgb_color(hue: f32, saturation: f32, value: f32) -> Rgb {
    hsv_to_rgb(hue, saturation, value)
}
pub fn blend_oklab(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn oklab_blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn interpolate_oklab(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn lerp_oklab(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn interpolate_hsv(a: Rgb, b: Rgb, t: f32) -> Rgb {
    hsv_interpolate(a, b, t)
}
pub fn lerp_hsv(a: Rgb, b: Rgb, t: f32) -> Rgb {
    hsv_interpolate(a, b, t)
}
pub fn blend_hsv(a: Rgb, b: Rgb, t: f32) -> Rgb {
    hsv_interpolate(a, b, t)
}
pub fn complementary_color(color: Rgb) -> Rgb {
    complementary(color)
}
pub fn complement(color: Rgb) -> Rgb {
    complementary(color)
}
pub fn accent_complement(color: Rgb) -> Rgb {
    complementary(color)
}
pub fn accent_neighbors_color(color: Rgb) -> (Rgb, Rgb, Rgb) {
    accent_neighbors(color)
}
pub fn panel_gradient_colors(base: Rgb, steps: usize) -> Vec<Rgb> {
    panel_gradient(base, steps)
}
pub fn derive_panel_gradient(base: Rgb, steps: usize) -> Vec<Rgb> {
    panel_gradient(base, steps)
}
pub fn escalate_colors(base: Rgb) -> (Rgb, Rgb) {
    escalate(base)
}
pub fn warning_error(base: Rgb) -> (Rgb, Rgb) {
    escalate(base)
}
pub fn upgrade_warning_error(base: Rgb) -> (Rgb, Rgb) {
    escalate(base)
}

pub fn interpolate(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    oklab_interpolate(a, b, t)
}
pub fn harmonize(color: Rgb) -> (Rgb, Rgb, Rgb) {
    accent_neighbors(color)
}
pub fn harmonize_color(color: Rgb) -> (Rgb, Rgb, Rgb) {
    accent_neighbors(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_interpolation_endpoints() {
        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(200, 180, 160);
        assert_eq!(oklab_interpolate(a, b, 0.0), a);
        assert_eq!(oklab_interpolate(a, b, 1.0), b);
    }

    #[test]
    fn hsv_interpolation_endpoints() {
        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(200, 180, 160);
        assert_eq!(hsv_interpolate(a, b, 0.0), a);
        assert_eq!(hsv_interpolate(a, b, 1.0), b);
    }

    #[test]
    fn hsv_round_trip_is_close() {
        let c = Rgb::new(0x22, 0xD3, 0xEE);
        let (h, s, v) = rgb_to_hsv(c);
        let back = hsv_to_rgb(h, s, v);
        assert!(crate::theme::oklab::oklab_distance(c, back) < 0.02);
    }

    #[test]
    fn complementary_differs() {
        let c = Rgb::new(0x22, 0xD3, 0xEE);
        assert_ne!(complementary(c), c);
    }

    #[test]
    fn panel_gradient_has_requested_length() {
        let g = panel_gradient(Rgb::new(0x22, 0xD3, 0xEE), 5);
        assert_eq!(g.len(), 5);
    }

    #[test]
    fn escalate_returns_distinct_colors() {
        let (w, e) = escalate(Rgb::new(0x22, 0xD3, 0xEE));
        assert_ne!(w, e);
    }
}
