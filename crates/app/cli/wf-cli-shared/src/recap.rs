//! Screen-only recap gate shared by the CLI frontends.
//!
//! Whether a finished turn deserves its one-line note is a view decision:
//! the terminal knows how long the turn ran, whether focus left mid-turn
//! and how many rows the reply covers. The kernel never decides this.
//! The gate is a pure function so both frontends test the same thresholds.

use std::time::Duration;

/// Turns at or above this length earn a recap on duration alone.
pub const RECAP_AFTER: Duration = Duration::from_secs(120);

/// Whether a finished turn earns its automatic recap note. Manual `/recap`
/// bypasses this gate entirely; the display switch disables only the
/// automatic note.
pub fn wants_recap(
    ran: Option<Duration>,
    away: bool,
    reply_rows: usize,
    screen_rows: u16,
    enabled: bool,
    autonomous: bool,
) -> bool {
    if !enabled || autonomous {
        return false;
    }
    if away {
        return true;
    }
    if ran.is_some_and(|ran| ran >= RECAP_AFTER) {
        return true;
    }
    reply_rows > screen_rows as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(secs: u64) -> Option<Duration> {
        Some(Duration::from_secs(secs))
    }

    #[test]
    fn short_watched_turns_stay_silent() {
        assert!(!wants_recap(secs(10), false, 5, 24, true, false));
    }

    #[test]
    fn leaving_mid_turn_earns_a_recap() {
        assert!(wants_recap(secs(10), true, 5, 24, true, false));
    }

    #[test]
    fn long_turns_earn_a_recap() {
        assert!(wants_recap(secs(120), false, 5, 24, true, false));
        assert!(wants_recap(secs(600), false, 5, 24, true, false));
    }

    #[test]
    fn over_screen_replies_earn_a_recap() {
        assert!(wants_recap(secs(10), false, 25, 24, true, false));
        assert!(!wants_recap(secs(10), false, 24, 24, true, false));
    }

    #[test]
    fn disabled_switch_or_autonomous_turns_stay_silent() {
        assert!(!wants_recap(secs(600), true, 99, 24, false, false));
        assert!(!wants_recap(secs(600), true, 99, 24, true, true));
    }
}
