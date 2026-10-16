//! Performance-policy application for the interactive session.

use crate::interactive::InteractiveController;

impl InteractiveController {
    /// Apply a capability policy: minimal tiers drop the spinner and shimmer,
    /// reduced tiers keep the spinner but lose decorative shimmer.
    pub fn apply_perf_policy(&mut self, policy: crate::perf::TuiPerfPolicy) {
        self.simplified_render = policy.simplified_render;
        self.spinner_enabled = policy.keep_spinner;
        let mode = if policy.decorative_animations {
            crate::motion::MotionMode::Animated
        } else if policy.keep_spinner {
            crate::motion::MotionMode::Reduced
        } else {
            crate::motion::MotionMode::Static
        };
        for line in &mut self.scrollback {
            line.set_motion_mode(mode);
        }
        if let Some(streaming) = &mut self.streaming {
            streaming.set_motion_mode(mode);
        }
    }

    /// Show the active performance tier marker in the footer status line.
    pub fn set_perf_tier(&mut self, label: &'static str) {
        self.footer.set_perf_tier(label);
    }
}
