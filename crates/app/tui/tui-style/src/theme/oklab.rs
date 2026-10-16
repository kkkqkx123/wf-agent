//! Perceptual color math in OKLab for theme validation and remapping.
//!
//! All helpers are pure and dependency-free so palette checks stay
//! deterministic in unit tests. Conversions follow the reference OKLab
//! definition (sRGB to linear, linear to LMS, cube root, Lab weights) with
//! the matching inverse for remap round trips.

use super::data::Rgb;

/// OKLab coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Convert an 8-bit color to OKLab.
pub fn rgb_to_oklab(rgb: Rgb) -> Oklab {
    let r = srgb_to_linear(f32::from(rgb.r) / 255.0);
    let g = srgb_to_linear(f32::from(rgb.g) / 255.0);
    let b = srgb_to_linear(f32::from(rgb.b) / 255.0);
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    Oklab {
        l: 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        a: 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        b: 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    }
}

/// Convert OKLab back to 8-bit sRGB, clamping out-of-gamut channels.
pub fn oklab_to_rgb(lab: Oklab) -> Rgb {
    let l_ = lab.l + 0.3963377774 * lab.a + 0.2158037573 * lab.b;
    let m_ = lab.l - 0.1055613458 * lab.a - 0.0638541728 * lab.b;
    let s_ = lab.l - 0.0894841775 * lab.a - 1.2914855480 * lab.b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    let clamp = |c: f32| (linear_to_srgb(c) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb::new(clamp(r), clamp(g), clamp(b))
}

/// Perceptual distance between two colors.
pub fn oklab_distance(a: Rgb, b: Rgb) -> f32 {
    let x = rgb_to_oklab(a);
    let y = rgb_to_oklab(b);
    let dl = f64::from(x.l - y.l);
    let da = f64::from(x.a - y.a);
    let db = f64::from(x.b - y.b);
    (dl * dl + da * da + db * db).sqrt() as f32
}

/// WCAG-style contrast ratio from linearized channels.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f32 {
    let luminance = |c: Rgb| {
        let r = srgb_to_linear(f32::from(c.r) / 255.0);
        let g = srgb_to_linear(f32::from(c.g) / 255.0);
        let bl = srgb_to_linear(f32::from(c.b) / 255.0);
        0.2126 * r + 0.7152 * g + 0.0722 * bl
    };
    let l1 = luminance(a);
    let l2 = luminance(b);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// Simulate protanopia (red-blind) in linear RGB with the Machado matrix.
pub fn simulate_protanopia(rgb: Rgb) -> Rgb {
    simulate(rgb, [[0.567, 0.433, 0.0], [0.558, 0.442, 0.0], [0.0, 0.242, 0.758]])
}

/// Simulate deuteranopia (green-blind) in linear RGB.
pub fn simulate_deuteranopia(rgb: Rgb) -> Rgb {
    simulate(rgb, [[0.625, 0.375, 0.0], [0.7, 0.3, 0.0], [0.0, 0.3, 0.7]])
}

/// Simulate tritanopia (blue-blind) in linear RGB.
pub fn simulate_tritanopia(rgb: Rgb) -> Rgb {
    simulate(rgb, [[0.95, 0.05, 0.0], [0.0, 0.433, 0.567], [0.0, 0.475, 0.525]])
}

fn simulate(rgb: Rgb, m: [[f32; 3]; 3]) -> Rgb {
    let r = srgb_to_linear(f32::from(rgb.r) / 255.0);
    let g = srgb_to_linear(f32::from(rgb.g) / 255.0);
    let b = srgb_to_linear(f32::from(rgb.b) / 255.0);
    let mix = |row: [f32; 3]| {
        (linear_to_srgb(row[0] * r + row[1] * g + row[2] * b) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb::new(mix(m[0]), mix(m[1]), mix(m[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_white_distance_is_large() {
        let d = oklab_distance(Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
        assert!(d > 0.8, "distance was {d}");
    }

    #[test]
    fn identical_colors_have_zero_distance() {
        let c = Rgb::new(0x22, 0xD3, 0xEE);
        assert!(oklab_distance(c, c) < 1e-5);
    }

    #[test]
    fn round_trip_preserves_default_palette() {
        for c in [
            Rgb::new(0xE5, 0xE7, 0xEB),
            Rgb::new(0x22, 0xD3, 0xEE),
            Rgb::new(0x4A, 0xDE, 0x80),
            Rgb::new(0xF8, 0x71, 0x71),
        ] {
            let back = oklab_to_rgb(rgb_to_oklab(c));
            let drift = oklab_distance(c, back);
            assert!(drift < 0.01, "round trip drifted {drift} for {c:?}");
        }
    }

    #[test]
    fn contrast_extremes_match_wcag() {
        let ratio = contrast_ratio(Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
        assert!((ratio - 21.0).abs() < 0.5, "ratio was {ratio}");
        assert!((contrast_ratio(Rgb::new(10, 10, 10), Rgb::new(10, 10, 10)) - 1.0).abs() < 0.01);
    }

    #[test]
    fn colorblind_simulation_keeps_grayscale_stable() {
        let gray = Rgb::new(128, 128, 128);
        assert!(oklab_distance(simulate_protanopia(gray), gray) < 0.05);
        assert!(oklab_distance(simulate_deuteranopia(gray), gray) < 0.05);
        assert!(oklab_distance(simulate_tritanopia(gray), gray) < 0.05);
    }
}
