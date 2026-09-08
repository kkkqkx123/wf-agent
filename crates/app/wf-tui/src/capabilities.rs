//! Terminal capability detection: keyboard enhancement, color depth,
//! synchronized output, hyperlinks, and terminal type identification.
//!
//! [`TerminalCapabilities`] is the aggregate result of probing the
//! connected terminal at startup. [`TerminalProbe`] orchestrates the
//! individual [`CapabilityProbe`]s.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// RGB color triplet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSet {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Terminal capability aggregate.
#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    // Keyboard
    /// Kitty keyboard enhancement protocol support.
    pub kitty_keyboard: bool,
    /// Bracketed paste mode support.
    pub bracketed_paste: bool,
    /// Focus event reporting.
    pub focus_events: bool,

    // Output
    /// Synchronized output (DCS/SGR) support.
    pub synchronized_output: bool,

    // Colors
    /// Detected color depth.
    pub color_depth: ColorDepth,
    /// True color (24-bit) support.
    pub true_color: bool,

    // Features
    /// OSC 8 hyperlink support.
    pub hyperlinks: bool,
    /// Terminal type identification.
    pub terminal_type: TerminalType,
    /// Terminal version string (from DA2).
    pub version: Option<String>,

    // OSC color detection
    /// The terminal's default background color, if detected via OSC 11.
    pub default_colors: Option<ColorSet>,
}

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self {
            kitty_keyboard: false,
            bracketed_paste: true, // near-universal
            focus_events: false,
            synchronized_output: false,
            color_depth: ColorDepth::Ansi16,
            true_color: false,
            hyperlinks: false,
            terminal_type: TerminalType::Unknown,
            version: None,
            default_colors: None,
        }
    }
}

/// Color depth capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    Monochrome,
    Ansi16,
    Ansi256,
    TrueColor,
}

impl ColorDepth {
    /// Detect from environment variables.
    pub fn detect_from_env() -> Self {
        Self::detect(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    }

    /// Detect from explicit values.
    pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Self {
        match colorterm.map(str::to_ascii_lowercase).as_deref() {
            Some("truecolor") | Some("24bit") => return Self::TrueColor,
            _ => {}
        }
        if term.map(|t| t.contains("256color")).unwrap_or(false) {
            Self::Ansi256
        } else {
            Self::Ansi16
        }
    }
}

/// Terminal type identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalType {
    Unknown,
    Alacritty,
    Kitty,
    WezTerm,
    Ghostty,
    WindowsTerminal,
    ITerm2,
    Tmux,
    Screen,
    Rio,
}

impl TerminalType {
    /// Detect from `TERM_PROGRAM` environment variable.
    pub fn detect_from_env() -> Self {
        match std::env::var("TERM_PROGRAM").ok().as_deref() {
            Some("kitty") => Self::Kitty,
            Some("WezTerm") => Self::WezTerm,
            Some("ghostty") => Self::Ghostty,
            Some("iTerm.app") => Self::ITerm2,
            Some("rio") => Self::Rio,
            Some("Alacritty") => Self::Alacritty,
            Some("mintty") => Self::WindowsTerminal,
            _ => {
                // Check for tmux/screen
                if std::env::var("TMUX").is_ok() {
                    Self::Tmux
                } else if std::env::var("STY").is_ok() {
                    Self::Screen
                } else {
                    Self::Unknown
                }
            }
        }
    }

    /// Whether this terminal is known to support Kitty keyboard protocol.
    pub fn supports_kitty_keyboard(&self) -> bool {
        matches!(
            self,
            Self::Kitty | Self::WezTerm | Self::Ghostty | Self::Rio
        )
    }

    /// Whether this terminal supports focus events.
    pub fn supports_focus_events(&self) -> bool {
        matches!(
            self,
            Self::Kitty | Self::WezTerm | Self::Ghostty | Self::ITerm2 | Self::Rio
        )
    }
}

/// Individual capability probe trait.
pub trait CapabilityProbe {
    /// Probe and populate the given capabilities.
    fn probe(&self, capabilities: &mut TerminalCapabilities);
    /// Probe name for debugging.
    fn name(&self) -> &'static str;
}

/// Orchestrates multiple capability probes.
pub struct TerminalProbe {
    capabilities: TerminalCapabilities,
    probes: Vec<Box<dyn CapabilityProbe>>,
}

impl TerminalProbe {
    pub fn new() -> Self {
        let probes: Vec<Box<dyn CapabilityProbe>> = vec![
            Box::new(KeyboardEnhancementProbe),
            Box::new(ColorDepthProbe),
            Box::new(TerminalTypeProbe),
            Box::new(FocusEventProbe),
        ];

        Self {
            capabilities: TerminalCapabilities::default(),
            probes,
        }
    }

    /// Run all probes and return the detected capabilities.
    pub fn detect() -> Self {
        if !std::io::stdout().is_terminal() {
            return Self {
                capabilities: TerminalCapabilities::default(),
                probes: Vec::new(),
            };
        }

        let mut probe = Self::new();
        probe.run();
        probe
    }

    /// Execute all registered probes.
    pub fn run(&mut self) {
        for probe in &self.probes {
            probe.probe(&mut self.capabilities);
        }
    }

    /// Get the detected capabilities.
    pub fn capabilities(&self) -> &TerminalCapabilities {
        &self.capabilities
    }

    /// Consume the probe and return the capabilities.
    pub fn into_capabilities(self) -> TerminalCapabilities {
        self.capabilities
    }
}

impl Default for TerminalProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe keyboard enhancement support.
struct KeyboardEnhancementProbe;

impl CapabilityProbe for KeyboardEnhancementProbe {
    fn probe(&self, caps: &mut TerminalCapabilities) {
        let term_type = TerminalType::detect_from_env();
        caps.kitty_keyboard = term_type.supports_kitty_keyboard()
            || detect_keyboard_enhancement_heuristic();
    }

    fn name(&self) -> &'static str {
        "keyboard_enhancement"
    }
}

/// Probe color depth.
struct ColorDepthProbe;

impl CapabilityProbe for ColorDepthProbe {
    fn probe(&self, caps: &mut TerminalCapabilities) {
        caps.color_depth = ColorDepth::detect_from_env();
        caps.true_color = matches!(caps.color_depth, ColorDepth::TrueColor);
    }

    fn name(&self) -> &'static str {
        "color_depth"
    }
}

/// Probe terminal type.
struct TerminalTypeProbe;

impl CapabilityProbe for TerminalTypeProbe {
    fn probe(&self, caps: &mut TerminalCapabilities) {
        caps.terminal_type = TerminalType::detect_from_env();
    }

    fn name(&self) -> &'static str {
        "terminal_type"
    }
}

/// Probe focus event support.
struct FocusEventProbe;

impl CapabilityProbe for FocusEventProbe {
    fn probe(&self, caps: &mut TerminalCapabilities) {
        let term_type = TerminalType::detect_from_env();
        caps.focus_events = term_type.supports_focus_events();
    }

    fn name(&self) -> &'static str {
        "focus_events"
    }
}

/// Heuristic keyboard enhancement detection via `TERM_PROGRAM` and version.
fn detect_keyboard_enhancement_heuristic() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        match term_program.as_str() {
            "kitty" | "WezTerm" | "ghostty" | "rio" => return true,
            _ => {}
        }
    }
    if let Ok(term_program_version) = std::env::var("TERM_PROGRAM_VERSION") {
        if let Ok(version) = term_program_version.parse::<f64>() {
            if version >= 3.5 {
                return true;
            }
        }
    }
    false
}

/// Query the terminal's default background color via OSC 11.
pub fn detect_default_colors() -> Option<ColorSet> {
    use std::io::Read;

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }

    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]11;?\x1b\\").ok()?;
    stdout.flush().ok()?;

    let mut buf = [0u8; 256];
    let mut stdin = std::io::stdin();
    let deadline = Instant::now() + Duration::from_millis(100);

    let mut total = 0usize;
    while Instant::now() < deadline && total < buf.len() {
        if crossterm::event::poll(Duration::from_millis(10)).unwrap_or(false) {
            // Drain pending events.
        }
        match stdin.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }

    if total == 0 {
        return None;
    }

    let response = String::from_utf8_lossy(&buf[..total]);
    parse_osc_color_response(&response)
}

/// Parse an OSC color response like `ESC ] 11 ; rgb:RRRR/GGGG/BBBB ESC \`.
fn parse_osc_color_response(response: &str) -> Option<ColorSet> {
    let rgb_start = response.find("rgb:")?;
    let hex = &response[rgb_start + 4..];
    let hex = hex
        .trim_end_matches('\x07')
        .trim_end_matches("\x1b\\");

    let parts: Vec<&str> = hex.split('/').collect();
    if parts.len() != 3 {
        return None;
    }

    let parse_component = |s: &str| -> Option<u8> {
        let val = u16::from_str_radix(s, 16).ok()?;
        Some(if s.len() > 2 {
            (val >> 8) as u8
        } else {
            val as u8
        })
    };

    Some(ColorSet {
        r: parse_component(parts[0])?,
        g: parse_component(parts[1])?,
        b: parse_component(parts[2])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_depth_detection() {
        assert_eq!(
            ColorDepth::detect(Some("truecolor"), Some("xterm")),
            ColorDepth::TrueColor
        );
        assert_eq!(
            ColorDepth::detect(Some("24bit"), None),
            ColorDepth::TrueColor
        );
        assert_eq!(
            ColorDepth::detect(None, Some("xterm-256color")),
            ColorDepth::Ansi256
        );
        assert_eq!(ColorDepth::detect(None, Some("dumb")), ColorDepth::Ansi16);
        assert_eq!(ColorDepth::detect(None, None), ColorDepth::Ansi16);
    }

    #[test]
    fn terminal_type_detection() {
        let mutex = std::sync::Mutex::new(());
        let _lock = mutex.lock().unwrap();
        std::env::set_var("TERM_PROGRAM", "kitty");
        assert_eq!(TerminalType::detect_from_env(), TerminalType::Kitty);
        std::env::set_var("TERM_PROGRAM", "WezTerm");
        assert_eq!(TerminalType::detect_from_env(), TerminalType::WezTerm);
        std::env::remove_var("TERM_PROGRAM");
        assert_eq!(TerminalType::detect_from_env(), TerminalType::Unknown);
    }

    #[test]
    fn kitty_keyboard_support() {
        assert!(TerminalType::Kitty.supports_kitty_keyboard());
        assert!(TerminalType::WezTerm.supports_kitty_keyboard());
        assert!(!TerminalType::ITerm2.supports_kitty_keyboard());
    }

    #[test]
    fn parse_osc_color_response_4digit_hex() {
        let resp = "\x1b]11;rgb:ffff/8080/0000\x1b\\";
        let c = parse_osc_color_response(resp).unwrap();
        assert_eq!(c, ColorSet { r: 255, g: 128, b: 0 });
    }

    #[test]
    fn parse_osc_color_response_invalid() {
        assert!(parse_osc_color_response("no color here").is_none());
    }

    #[test]
    fn default_capabilities_are_safe() {
        let caps = TerminalCapabilities::default();
        assert!(caps.bracketed_paste);
        assert!(!caps.kitty_keyboard);
        assert_eq!(caps.color_depth, ColorDepth::Ansi16);
    }
}
