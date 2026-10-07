//! Terminal interaction facilities shared by the TUI inline and full-screen
//! forms (both inside this crate, unrelated to the `wf-mini` binary).
//!
//! This module re-exports from focused sub-modules:
//! - Mode state machine and presets (`TerminalModes`)
//! - Control plane trait and implementations (`TerminalControl`,
//!   `CrosstermControl`, `FakeControl`)
//! - Guard RAII state machine (`TerminalGuard`)
//! - Byte-sequence writers (enter / restore / reassert)
//! - Orphan detection and the panic hook
//! - Stderr suppression (`TerminalStderrGuard`)
//! - SIGINT double-press tracker (`DoublePressTracker`)
//! - Terminal capability probing (`TerminalProbe`)

pub mod commands;
pub mod control;
pub mod crossterm;
pub mod guard;
pub mod modes;
pub mod orphan;
pub mod panic;
pub mod sequences;

pub use crate::probe::{ColorSet, TerminalProbe};
pub use crate::sigint::{DoublePressTracker, PressOutcome, SIGINT_DOUBLE_PRESS_WINDOW};
pub use crate::stderr::TerminalStderrGuard;

pub use commands::{
    keyboard_enhancement_env_disabled, keyboard_enhancement_flags, DISABLE_KEYBOARD_ENHANCEMENT_ENV,
};
pub use control::{FakeControl, TerminalControl};
pub use crossterm::CrosstermControl;
pub use guard::TerminalGuard;
pub use modes::TerminalModes;
pub use orphan::{client_orphaned, client_orphaned_with, has_controlling_terminal};
pub use panic::install_panic_hook;
pub use sequences::{write_enter_sequences, write_reassert_sequences, write_restore_sequences};

#[cfg(test)]
mod tests {
    use super::*;

    use std::io;

    #[test]
    fn enter_then_restore_emits_reversed_sequence() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw"
            ]
        );
        guard.restore().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
        assert_eq!(guard.modes(), TerminalModes::OFF);
    }

    #[test]
    fn repeated_enter_of_same_modes_is_a_no_op() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::MINI).unwrap();
        let len = guard.control().ops.len();
        guard.enter(TerminalModes::MINI).unwrap();
        assert_eq!(guard.control().ops.len(), len);
    }

    #[test]
    fn enter_delta_only_flips_changed_switches() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::MINI).unwrap();
        // raw + paste stay on; only the cursor becomes hidden.
        guard
            .enter(TerminalModes {
                cursor_hidden: true,
                ..TerminalModes::MINI
            })
            .unwrap();
        assert_eq!(
            guard.control().ops,
            vec!["enable_bracketed_paste", "enable_raw", "hide_cursor"]
        );
    }

    #[test]
    fn double_enter_double_exit_stays_consistent() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        for _ in 0..2 {
            guard.enter(TerminalModes::TUI).unwrap();
            guard.restore().unwrap();
        }
        // Two symmetric enter/restore cycles with no residue in between.
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
        assert_eq!(guard.modes(), TerminalModes::OFF);
    }

    #[test]
    fn restore_is_idempotent() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        guard.restore().unwrap();
        let len = guard.control().ops.len();
        guard.restore().unwrap();
        guard.restore().unwrap();
        assert_eq!(guard.control().ops.len(), len);
    }

    #[test]
    fn with_restored_runs_op_between_restore_and_reenter() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        let ops_before = guard.control().ops.len();

        let answer = guard
            .with_restored(None, || {
                // Inside the window the tracked modes are all off; the
                // restore cycle is the last thing that ran.
                42
            })
            .unwrap();

        assert_eq!(answer, 42);
        assert_eq!(guard.modes(), TerminalModes::TUI);
        let window = &guard.control().ops[ops_before..];
        assert_eq!(
            window,
            vec![
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
            ]
        );
    }

    #[test]
    fn drop_restores_everything() {
        use std::cell::RefCell;
        use std::rc::Rc;

        // Shared recorder so the ops survive the guard drop.
        #[derive(Debug, Default)]
        struct Shared {
            ops: Rc<RefCell<Vec<&'static str>>>,
        }
        impl TerminalControl for Shared {
            fn enable_raw(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_raw");
                Ok(())
            }
            fn disable_raw(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_raw");
                Ok(())
            }
            fn enter_alt_screen(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enter_alt_screen");
                Ok(())
            }
            fn leave_alt_screen(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("leave_alt_screen");
                Ok(())
            }
            fn enable_bracketed_paste(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_bracketed_paste");
                Ok(())
            }
            fn disable_bracketed_paste(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_bracketed_paste");
                Ok(())
            }
            fn hide_cursor(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("hide_cursor");
                Ok(())
            }
            fn show_cursor(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("show_cursor");
                Ok(())
            }
            fn enable_focus_change(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_focus_change");
                Ok(())
            }
            fn disable_focus_change(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_focus_change");
                Ok(())
            }
            fn enable_mouse_capture(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_mouse_capture");
                Ok(())
            }
            fn disable_mouse_capture(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_mouse_capture");
                Ok(())
            }
            fn enable_alternate_scroll(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("enable_alternate_scroll");
                Ok(())
            }
            fn disable_alternate_scroll(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("disable_alternate_scroll");
                Ok(())
            }
            fn push_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("push_keyboard_enhancement");
                Ok(())
            }
            fn pop_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("pop_keyboard_enhancement");
                Ok(())
            }
            fn set_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("set_keyboard_enhancement");
                Ok(())
            }
            fn reset_keyboard_enhancement(&mut self) -> io::Result<()> {
                self.ops.borrow_mut().push("reset_keyboard_enhancement");
                Ok(())
            }
        }

        let ops = Rc::new(RefCell::new(Vec::new()));
        {
            let mut guard = TerminalGuard::new(Shared {
                ops: Rc::clone(&ops),
            });
            guard.enter(TerminalModes::TUI).unwrap();
            assert_eq!(ops.borrow().len(), 4);
        } // drop → restore cycle
        assert_eq!(
            *ops.borrow(),
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
    }

    #[test]
    fn input_modes_enter_and_restore_in_reverse_order() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(modes).unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_focus_change",
                "enable_mouse_capture",
                "enable_alternate_scroll",
                "enable_raw",
                "push_keyboard_enhancement",
            ]
        );
        guard.restore().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_focus_change",
                "enable_mouse_capture",
                "enable_alternate_scroll",
                "enable_raw",
                "push_keyboard_enhancement",
                "pop_keyboard_enhancement",
                "disable_raw",
                "disable_alternate_scroll",
                "disable_mouse_capture",
                "disable_focus_change",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
        assert_eq!(guard.modes(), TerminalModes::OFF);
    }

    #[test]
    fn disabled_input_modes_emit_no_extra_ops() {
        // With every input switch off the guard behaves exactly like the
        // legacy baseline: the rollback line for all three capabilities.
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        guard.restore().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "hide_cursor",
                "enter_alt_screen",
                "enable_bracketed_paste",
                "enable_raw",
                "disable_raw",
                "disable_bracketed_paste",
                "leave_alt_screen",
                "show_cursor",
            ]
        );
    }

    #[test]
    fn reassert_reenables_without_touching_the_keyboard_stack() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(modes).unwrap();
        guard.control().ops.clear();
        guard.reassert().unwrap();
        assert_eq!(
            guard.control().ops,
            vec![
                "enable_bracketed_paste",
                "enable_focus_change",
                "enable_mouse_capture",
                "enable_alternate_scroll",
                "set_keyboard_enhancement",
            ]
        );
        // The tracked modes are unchanged by the reassert.
        assert_eq!(guard.modes(), modes);
    }

    #[test]
    fn reassert_with_all_input_off_only_touches_paste() {
        let mut guard = TerminalGuard::new(FakeControl::default());
        guard.enter(TerminalModes::TUI).unwrap();
        guard.control().ops.clear();
        guard.reassert().unwrap();
        assert_eq!(guard.control().ops, vec!["enable_bracketed_paste"]);
    }

    #[test]
    fn keyboard_flags_cover_three_items_without_full_key_reporting() {
        use ::crossterm::event::KeyboardEnhancementFlags;
        let flags = keyboard_enhancement_flags();
        assert!(flags.contains(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES));
        assert!(flags.contains(KeyboardEnhancementFlags::REPORT_EVENT_TYPES));
        assert!(flags.contains(KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS));
        assert!(!flags.contains(KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES));
    }

    #[test]
    fn keyboard_env_switch_defaults_on_and_parses_explicit_values() {
        let var = DISABLE_KEYBOARD_ENHANCEMENT_ENV;
        let saved = std::env::var(var).ok();
        let restore = |saved: &Option<String>| {
            if let Some(value) = saved {
                std::env::set_var(var, value);
            } else {
                std::env::remove_var(var);
            }
        };

        std::env::remove_var(var);
        assert!(!keyboard_enhancement_env_disabled());
        for truthy in ["1", "true", "TRUE", "yes"] {
            std::env::set_var(var, truthy);
            assert!(keyboard_enhancement_env_disabled(), "value {truthy}");
        }
        for falsy in ["0", "false", "no"] {
            std::env::set_var(var, falsy);
            assert!(!keyboard_enhancement_env_disabled(), "value {falsy}");
        }
        std::env::set_var(var, "unset-value");
        assert!(!keyboard_enhancement_env_disabled());
        restore(&saved);
    }

    #[test]
    fn enter_bytes_cover_all_switches_when_enabled() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut out = Vec::new();
        write_enter_sequences(&mut out, modes).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("\x1b[?25l\x1b[?1049h"));
        assert!(text.contains("\x1b[?2004h"));
        assert!(text.contains("\x1b[?1004h"));
        assert!(text.contains("\x1b[?1000h"));
        assert!(text.contains("\x1b[?1006h"));
        assert!(text.contains("\x1b[?1007h"));
        assert!(text.contains("\x1b[>7u"));
    }

    #[test]
    fn enter_bytes_match_legacy_baseline_when_input_off() {
        let mut out = Vec::new();
        write_enter_sequences(&mut out, TerminalModes::TUI).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b[?25l\x1b[?1049h\x1b[?2004h"
        );
        let mut restore = Vec::new();
        write_restore_sequences(&mut restore, TerminalModes::TUI).unwrap();
        assert_eq!(
            String::from_utf8(restore).unwrap(),
            "\x1b[?2004l\x1b[?1049l\x1b[?25h"
        );
    }

    #[test]
    fn restore_bytes_unwind_in_reverse_enter_order() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut out = Vec::new();
        write_restore_sequences(&mut out, modes).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            concat!(
                "\x1b[<1u",
                "\x1b[?1007l",
                "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l",
                "\x1b[?1004l",
                "\x1b[?2004l",
                "\x1b[?1049l",
                "\x1b[?25h",
            )
        );
    }

    #[test]
    fn reassert_bytes_use_set_form_and_never_push() {
        let modes = TerminalModes::TUI.with_input_modes(true, true, true, true);
        let mut out = Vec::new();
        write_reassert_sequences(&mut out, modes).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("\x1b[?2004h"));
        assert!(text.contains("\x1b[?1004h"));
        assert!(text.contains("\x1b[?1000h"));
        assert!(text.contains("\x1b[=7u"));
        assert!(
            !text.contains("\x1b[>"),
            "reassert must not push the keyboard stack"
        );

        let mut off = Vec::new();
        write_reassert_sequences(&mut off, TerminalModes::TUI).unwrap();
        assert_eq!(String::from_utf8(off).unwrap(), "\x1b[?2004h");
    }

    #[test]
    fn orphan_rule_needs_eof_without_controlling_terminal() {
        assert!(client_orphaned_with(true, false));
        assert!(!client_orphaned_with(true, true));
        assert!(!client_orphaned_with(false, false));
        assert!(!client_orphaned_with(false, true));
    }
}
