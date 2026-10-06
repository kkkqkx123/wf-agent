//! Process-wide panic hook that restores the real terminal before
//! delegating to the previous hook.

use std::io;

use super::commands::{DisableAlternateScroll, ResetKeyboardEnhancementFlags};

/// Install a process-wide panic hook that restores the *real* terminal
/// (disable paste / focus / mouse / alternate scroll, keyboard pop plus
/// strong reset, show cursor, reset colors, leave alt screen, disable raw
/// mode) before delegating to the previous hook. Idempotent: a second call
/// is a no-op.
pub fn install_panic_hook() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(
            stdout,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableFocusChange,
            crossterm::event::DisableMouseCapture,
            DisableAlternateScroll,
            crossterm::event::PopKeyboardEnhancementFlags,
            ResetKeyboardEnhancementFlags,
            crossterm::cursor::Show,
            crossterm::style::ResetColor,
            crossterm::terminal::LeaveAlternateScreen
        );
        let _ = crossterm::terminal::disable_raw_mode();
        previous(info);
    }));
}
