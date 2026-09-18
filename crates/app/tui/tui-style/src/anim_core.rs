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
}
