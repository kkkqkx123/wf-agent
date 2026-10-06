//! Theme resolution and live probing.

use std::time::Duration;

use super::cache::{load_theme_cache, load_theme_file, load_theme_overrides, save_theme_cache};
use super::data::{Rgb, Theme, ThemeOverrides};
use super::derive::derive_theme;

/// Default OSC response wait
pub const OSC_PROBE_TIMEOUT: Duration = Duration::from_millis(100);

/// Pure resolution over already-loaded inputs. The caller supplies the file,
/// live probe, cache and override values; no filesystem or terminal access
/// happens here, so the full priority chain stays unit-testable.
pub fn resolve_theme(
    file: Option<Theme>,
    probed_bg: Option<Rgb>,
    probed_fg: Option<Rgb>,
    cached: Option<Theme>,
    overrides: Option<ThemeOverrides>,
) -> Theme {
    let mut theme = if let Some(theme) = file {
        theme
    } else if let Some(bg) = probed_bg {
        let theme = derive_theme(bg, probed_fg);
        save_theme_cache(&theme);
        theme
    } else if let Some(cached) = cached {
        let mut cached = cached;
        cached.source = super::data::ThemeSource::Cached;
        cached
    } else {
        Theme::dark_default()
    };
    if let Some(overrides) = overrides {
        theme = overrides.apply(theme);
    }
    theme
}

/// Probe the terminal theme with the default timeout; never panics —
/// failures fall back to the cache / built-in dark theme. The live OSC
/// query runs in `tui-terminal`; this crate only maps the result.
pub fn probe_theme() -> Theme {
    probe_theme_with_timeout(OSC_PROBE_TIMEOUT)
}

/// [`probe_theme`] with an explicit timeout.
pub fn probe_theme_with_timeout(timeout: Duration) -> Theme {
    let file = load_theme_file();
    let (probed_bg, probed_fg) = if file.is_none() {
        let (fg, bg) = tui_terminal::probe::probe_osc_colors(timeout);
        (
            bg.map(|c| Rgb::new(c.r, c.g, c.b)),
            fg.map(|c| Rgb::new(c.r, c.g, c.b)),
        )
    } else {
        (None, None)
    };
    let cached = load_theme_cache();
    let overrides = load_theme_overrides();
    resolve_theme(file, probed_bg, probed_fg, cached, overrides)
}

/// Fallback chain: last-known-good cache → built-in dark theme.
pub fn fallback_theme() -> Theme {
    load_theme_cache().unwrap_or_else(Theme::dark_default)
}
