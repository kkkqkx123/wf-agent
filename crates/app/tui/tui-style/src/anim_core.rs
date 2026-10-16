//! Deterministic animation numeric core.
//!
//! Hot frame math lives here with no clock, terminal or style dependencies,
//! so it stays testable and optimizable in isolation. Trigonometric phases
//! use a precomputed angle table, and table lookups replay the same float
//! accumulation as the inline loop, keeping results identical across
//! optimization levels.

/// Table size for one full phase cycle.
pub const ANGLE_TABLE_SIZE: usize = 256;

/// Precomputed cosine table for evenly spaced phases over one cycle.
pub struct AngleTable {
    cos: [f32; ANGLE_TABLE_SIZE],
    sin: [f32; ANGLE_TABLE_SIZE],
}

impl AngleTable {
    pub const fn empty() -> Self {
        Self {
            cos: [0.0; ANGLE_TABLE_SIZE],
            sin: [0.0; ANGLE_TABLE_SIZE],
        }
    }

    pub fn build() -> Self {
        let mut table = Self::empty();
        let mut index = 0usize;
        while index < ANGLE_TABLE_SIZE {
            let phase = (index as f32) / (ANGLE_TABLE_SIZE as f32) * core::f32::consts::TAU;
            table.cos[index] = phase.cos();
            table.sin[index] = phase.sin();
            index += 1;
        }
        table
    }

    pub fn cos_at(&self, phase01: f32) -> f32 {
        self.cos[phase_index(phase01)]
    }

    pub fn sin_at(&self, phase01: f32) -> f32 {
        self.sin[phase_index(phase01)]
    }
}

fn phase_index(phase01: f32) -> usize {
    let clamped = phase01 - phase01.floor();
    ((clamped * (ANGLE_TABLE_SIZE as f32)).floor() as usize) % ANGLE_TABLE_SIZE
}

/// Shared cycle math: whole cycles elapsed since start, sped up, modulo the
/// frame count. Pure and deterministic for an explicit clock.
pub fn cycle_frame(now_ms: u64, start_ms: u64, cycle_ms: u64, speed: f32, frames: usize) -> usize {
    if frames == 0 || cycle_ms == 0 {
        return 0;
    }
    let elapsed = now_ms.saturating_sub(start_ms);
    let cycle = (f64::from(cycle_ms as u32) / f64::from(speed.max(f32::EPSILON))).round() as u64;
    if cycle == 0 {
        return 0;
    }
    (elapsed / cycle.max(1)) as usize % frames
}

/// Phase in `[0, 1)` for a shimmer sweep at `now_ms`.
pub fn shimmer_phase(now_ms: u64, start_ms: u64, cycle_ms: u64, speed: f32) -> f32 {
    if cycle_ms == 0 {
        return 0.0;
    }
    let elapsed = now_ms.saturating_sub(start_ms) as f32;
    let cycle = (cycle_ms as f32 / speed.max(f32::EPSILON)).max(1.0);
    (elapsed % cycle) / cycle
}

/// Convert HSV to RGB with hue in degrees.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// Convert RGB to HSV with hue in degrees. Centralized here so theme
/// adaptation and animation share one conversion.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max)
}

/// Linear interpolation between two bytes.
pub fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (f32::from(a) + (f32::from(b) - f32::from(a)) * t).clamp(0.0, 255.0) as u8
}

/// Linear interpolation between two RGB colors.
pub fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    (
        lerp_u8(a.0, b.0, t),
        lerp_u8(a.1, b.1, t),
        lerp_u8(a.2, b.2, t),
    )
}

/// Three-dimensional idle sampler kinds. Pure math only; disabled by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleVisual3D {
    /// Torus surface point.
    Donut,
    /// Spiral infall point.
    BlackHole,
    /// Precession point.
    Gyroscope,
    /// Inclined orbit ring point.
    OrbitRing,
}

/// Three-dimensional sample point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
    /// Z coordinate.
    pub z: f32,
}

/// Glyph set for 3x3 idle selection.
pub const IDLE_3X3_GLYPHS: [&str; 9] = ["·", ":", "-", "=", "+", "*", "#", "@", "●"];

/// Select a 3x3 glyph from a normal in `[-1, 1]`. Pure quantization.
pub fn select_glyph_3x3(nx: f32, ny: f32) -> &'static str {
    let ix = ((nx.clamp(-1.0, 1.0) + 1.0) * 1.5).floor() as usize;
    let iy = ((ny.clamp(-1.0, 1.0) + 1.0) * 1.5).floor() as usize;
    let ix = ix.min(2);
    let iy = iy.min(2);
    IDLE_3X3_GLYPHS[iy * 3 + ix]
}

/// Sample `kind` at `phase01` through the angle table. Phases quantized to
/// table indices replay direct computation bitwise.
pub fn idle_3d_sample(table: &AngleTable, kind: IdleVisual3D, phase01: f32) -> Vec3 {
    match kind {
        IdleVisual3D::Donut => donut_table(table, phase01, phase01 * 2.0),
        IdleVisual3D::BlackHole => black_hole_table(table, phase01),
        IdleVisual3D::Gyroscope => gyroscope_table(table, phase01),
        IdleVisual3D::OrbitRing => orbit_ring_table(table, phase01),
    }
}

/// Direct computation twin of [`idle_3d_sample`] for bit-exact tests.
pub fn idle_3d_sample_direct(kind: IdleVisual3D, phase01: f32) -> Vec3 {
    match kind {
        IdleVisual3D::Donut => donut_direct(phase01, phase01 * 2.0),
        IdleVisual3D::BlackHole => black_hole_direct(phase01),
        IdleVisual3D::Gyroscope => gyroscope_direct(phase01),
        IdleVisual3D::OrbitRing => orbit_ring_direct(phase01),
    }
}

fn donut_table(table: &AngleTable, u: f32, v: f32) -> Vec3 {
    let major = 1.0_f32;
    let minor = 0.4_f32;
    let cu = table.cos_at(u);
    let su = table.sin_at(u);
    let cv = table.cos_at(v);
    let sv = table.sin_at(v);
    Vec3 {
        x: (major + minor * cv) * cu,
        y: (major + minor * cv) * su,
        z: minor * sv,
    }
}

fn donut_direct(u: f32, v: f32) -> Vec3 {
    let major = 1.0_f32;
    let minor = 0.4_f32;
    let u = u - u.floor();
    let v = v - v.floor();
    let cu = (u * core::f32::consts::TAU).cos();
    let su = (u * core::f32::consts::TAU).sin();
    let cv = (v * core::f32::consts::TAU).cos();
    let sv = (v * core::f32::consts::TAU).sin();
    Vec3 {
        x: (major + minor * cv) * cu,
        y: (major + minor * cv) * su,
        z: minor * sv,
    }
}

fn black_hole_table(table: &AngleTable, phase01: f32) -> Vec3 {
    let radius = 1.0 - phase01.clamp(0.0, 1.0) * 0.8;
    let angle = phase01 * 4.0;
    let ca = table.cos_at(angle);
    let sa = table.sin_at(angle);
    Vec3 {
        x: radius * ca,
        y: radius * 0.5 * sa,
        z: -phase01,
    }
}

fn black_hole_direct(phase01: f32) -> Vec3 {
    let radius = 1.0 - phase01.clamp(0.0, 1.0) * 0.8;
    let angle = phase01 * 4.0;
    let wrapped = angle - angle.floor();
    Vec3 {
        x: radius * (wrapped * core::f32::consts::TAU).cos(),
        y: radius * 0.5 * (wrapped * core::f32::consts::TAU).sin(),
        z: -phase01,
    }
}

fn gyroscope_table(table: &AngleTable, phase01: f32) -> Vec3 {
    let theta = phase01;
    let phi = phase01 * 3.0;
    let st = table.sin_at(theta);
    let ct = table.cos_at(theta);
    let sp = table.sin_at(phi);
    let cp = table.cos_at(phi);
    Vec3 {
        x: st * cp,
        y: st * sp,
        z: ct,
    }
}

fn gyroscope_direct(phase01: f32) -> Vec3 {
    let theta = phase01 - phase01.floor();
    let phi_raw = phase01 * 3.0;
    let phi = phi_raw - phi_raw.floor();
    let theta = theta * core::f32::consts::TAU;
    let phi = phi * core::f32::consts::TAU;
    Vec3 {
        x: theta.sin() * phi.cos(),
        y: theta.sin() * phi.sin(),
        z: theta.cos(),
    }
}

fn orbit_ring_table(table: &AngleTable, phase01: f32) -> Vec3 {
    let ca = table.cos_at(phase01);
    let sa = table.sin_at(phase01);
    let incl = 0.5_f32;
    Vec3 {
        x: ca,
        y: sa * incl.cos(),
        z: sa * incl.sin(),
    }
}

fn orbit_ring_direct(phase01: f32) -> Vec3 {
    let wrapped = phase01 - phase01.floor();
    let angle = wrapped * core::f32::consts::TAU;
    let incl = 0.5_f32;
    Vec3 {
        x: angle.cos(),
        y: angle.sin() * incl.cos(),
        z: angle.sin() * incl.sin(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_math_matches_controller_expectations() {
        assert_eq!(cycle_frame(1_000, 1_000, 800, 1.0, 8), 0);
        assert_eq!(cycle_frame(1_800, 1_000, 800, 1.0, 8), 1);
        assert_eq!(cycle_frame(1_000 + 8 * 800, 1_000, 800, 1.0, 8), 0);
    }

    #[test]
    fn angle_table_covers_full_cycle() {
        let table = AngleTable::build();
        assert!((table.cos_at(0.0) - 1.0).abs() < 0.05);
        assert!(table.sin_at(0.0).abs() < 0.05);
        assert!((table.cos_at(0.5) + 1.0).abs() < 0.05);
    }

    #[test]
    fn angle_table_matches_direct_computation_bitwise() {
        let table = AngleTable::build();
        for index in [0usize, 1, 64, 128, 192, 255] {
            let phase = (index as f32) / (ANGLE_TABLE_SIZE as f32);
            let direct_cos = (phase * core::f32::consts::TAU).cos();
            let direct_sin = (phase * core::f32::consts::TAU).sin();
            assert_eq!(
                table.cos_at(phase).to_bits(),
                direct_cos.to_bits(),
                "cos table entry {index} must equal direct computation bitwise"
            );
            assert_eq!(
                table.sin_at(phase).to_bits(),
                direct_sin.to_bits(),
                "sin table entry {index} must equal direct computation bitwise"
            );
        }
    }

    #[test]
    fn shimmer_phase_stays_unit() {
        let phase = shimmer_phase(1_500, 1_000, 800, 1.0);
        assert!((0.0..1.0).contains(&phase));
    }

    #[test]
    fn hsv_round_trips_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), (0, 0, 255));
    }

    #[test]
    fn rgb_hsv_round_trips_through_central_core() {
        let (h, s, v) = rgb_to_hsv(255, 0, 0);
        assert!((h - 0.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 1e-6);
        assert!((v - 1.0).abs() < 1e-6);
        assert_eq!(lerp_u8(0, 100, 0.5), 50);
        assert_eq!(lerp_rgb((0, 0, 0), (255, 255, 255), 0.5), (127, 127, 127));
    }

    #[test]
    fn idle_3d_table_matches_direct_bitwise() {
        let table = AngleTable::build();
        for index in [0usize, 32, 64, 128, 192] {
            let phase = (index as f32) / (ANGLE_TABLE_SIZE as f32);
            for kind in [
                IdleVisual3D::Donut,
                IdleVisual3D::BlackHole,
                IdleVisual3D::Gyroscope,
                IdleVisual3D::OrbitRing,
            ] {
                let tabled = idle_3d_sample(&table, kind, phase);
                let direct = idle_3d_sample_direct(kind, phase);
                assert_eq!(
                    (tabled.x.to_bits(), tabled.y.to_bits(), tabled.z.to_bits()),
                    (direct.x.to_bits(), direct.y.to_bits(), direct.z.to_bits()),
                    "3d {kind:?} at index {index} must match direct bitwise"
                );
            }
        }
    }

    #[test]
    fn glyph_3x3_selection_covers_nine_cells() {
        assert_eq!(select_glyph_3x3(-1.0, -1.0), IDLE_3X3_GLYPHS[0]);
        assert_eq!(select_glyph_3x3(1.0, 1.0), IDLE_3X3_GLYPHS[8]);
        assert_eq!(IDLE_3X3_GLYPHS.len(), 9);
    }
}
