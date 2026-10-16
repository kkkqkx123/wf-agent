//! Streaming tail-follow catch-up: large appends slide to the bottom instead
//! of jumping.
//!
//! While the viewport follows the live tail, an append that grows the content
//! by more than a viewport would teleport the window. The catch-up keeps a
//! lag (display rows behind the live tail, capped to one viewport) and closes
//! it a few rows per frame. Explicit navigation snaps instantly, and static
//! (decoration-free) rendering skips the slide entirely.

/// Appends smaller than this close instantly instead of animating.
pub const TAIL_CATCHUP_MIN_JUMP: usize = 4;

/// Maximum rows the window advances toward the tail in one frame.
pub const TAIL_CATCHUP_MAX_STEP: usize = 3;

/// Cap a fresh lag to one viewport so catch-up never trails by more.
pub fn cap_lag(lag: usize, viewport: usize) -> usize {
    lag.min(viewport.max(1))
}

/// Rows to advance this frame toward the live tail.
///
/// Returns the full `lag` (snap) when the motion is static, when the lag fits
/// under [`TAIL_CATCHUP_MIN_JUMP`], or when the viewport cannot slide;
/// otherwise returns at most [`TAIL_CATCHUP_MAX_STEP`] rows.
pub fn catchup_step(lag: usize, viewport: usize, animated: bool) -> usize {
    if lag == 0 || viewport == 0 {
        return 0;
    }
    if !animated || lag < TAIL_CATCHUP_MIN_JUMP {
        return lag;
    }
    lag.min(TAIL_CATCHUP_MAX_STEP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_appends_snap_without_sliding() {
        assert_eq!(catchup_step(0, 10, true), 0);
        assert_eq!(catchup_step(1, 10, true), 1);
        assert_eq!(catchup_step(3, 10, true), 3);
    }

    #[test]
    fn large_appends_advance_a_few_rows_per_frame() {
        assert_eq!(catchup_step(10, 10, true), TAIL_CATCHUP_MAX_STEP);
        assert_eq!(catchup_step(4, 10, true), TAIL_CATCHUP_MAX_STEP.min(4));
        // Fixed append sequence follows the curve: bounded steps that sum to
        // the lag without overshooting.
        let mut lag = 10usize;
        let mut steps = Vec::new();
        while lag > 0 {
            let step = catchup_step(lag, 10, true);
            assert!((1..=TAIL_CATCHUP_MAX_STEP).contains(&step));
            steps.push(step);
            lag -= step;
        }
        assert_eq!(steps.iter().sum::<usize>(), 10);
        assert!(steps.len() > 1, "large appends must span frames");
    }

    #[test]
    fn static_rendering_snaps_instantly() {
        assert_eq!(catchup_step(25, 10, false), 25);
    }

    #[test]
    fn lag_never_exceeds_one_viewport() {
        assert_eq!(cap_lag(100, 10), 10);
        assert_eq!(cap_lag(5, 10), 5);
        assert_eq!(cap_lag(0, 10), 0);
    }
}
