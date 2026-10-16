//! Explicit render modes for spacing, diagrams and math.
//!
//! The canonical markdown dialect version stays as the coarse cache
//! generation, while these three switches ride along as fine-grained inputs:
//! every switch participates in preparation cache keys, so flipping any of
//! them invalidates laid-out rows. [`RenderOptions`] is the single context
//! threaded from callers through parsing and layout.

/// Row-gap mode between blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpacingMode {
    /// No extra gaps between blocks.
    #[default]
    Compact = 0,
    /// One blank row between top-level blocks.
    Comfortable = 1,
}

impl SpacingMode {
    /// Byte value folded into cache keys.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode from a key byte, defaulting to compact.
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => SpacingMode::Comfortable,
            _ => SpacingMode::Compact,
        }
    }
}

/// Diagram display mode for fenced diagram sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartMode {
    /// Text fallback: keep the fence source as readable rows.
    #[default]
    Text = 0,
    /// Graphical placeholder: bordered box registered for rasterization.
    Graphical = 1,
}

impl ChartMode {
    /// Byte value folded into cache keys.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode from a key byte, defaulting to text.
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => ChartMode::Graphical,
            _ => ChartMode::Text,
        }
    }
}

/// LaTeX rendering mode for display math.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LatexMode {
    /// Unicode fallback text beside the raw expression.
    #[default]
    Unicode = 0,
    /// External TeX-to-image backend when available, otherwise Unicode
    /// plus an explanatory reason.
    TexImage = 1,
}

impl LatexMode {
    /// Byte value folded into cache keys.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode from a key byte, defaulting to unicode.
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => LatexMode::TexImage,
            _ => LatexMode::Unicode,
        }
    }
}

/// Combined render options threaded through parsing and layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderOptions {
    pub spacing: SpacingMode,
    pub chart: ChartMode,
    pub latex: LatexMode,
}

impl RenderOptions {
    /// Default options: compact spacing, text diagrams, unicode math.
    pub fn new() -> Self {
        Self::default()
    }

    /// Options with explicit modes.
    pub fn with_modes(spacing: SpacingMode, chart: ChartMode, latex: LatexMode) -> Self {
        Self {
            spacing,
            chart,
            latex,
        }
    }

    /// Key bytes `(spacing, chart, latex)` for preparation caches.
    pub fn as_key_bytes(self) -> (u8, u8, u8) {
        (self.spacing.as_u8(), self.chart.as_u8(), self.latex.as_u8())
    }

    /// Rebuild from key bytes.
    pub fn from_key_bytes(spacing: u8, chart: u8, latex: u8) -> Self {
        Self {
            spacing: SpacingMode::from_u8(spacing),
            chart: ChartMode::from_u8(chart),
            latex: LatexMode::from_u8(latex),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_encode_to_zero_bytes() {
        let options = RenderOptions::new();
        assert_eq!(options.as_key_bytes(), (0, 0, 0));
        assert_eq!(
            RenderOptions::from_key_bytes(0, 0, 0),
            RenderOptions::default()
        );
    }

    #[test]
    fn each_mode_flips_its_key_byte() {
        let spacing = RenderOptions::with_modes(
            SpacingMode::Comfortable,
            ChartMode::Text,
            LatexMode::Unicode,
        );
        assert_eq!(spacing.as_key_bytes(), (1, 0, 0));
        let chart = RenderOptions::with_modes(
            SpacingMode::Compact,
            ChartMode::Graphical,
            LatexMode::Unicode,
        );
        assert_eq!(chart.as_key_bytes(), (0, 1, 0));
        let latex =
            RenderOptions::with_modes(SpacingMode::Compact, ChartMode::Text, LatexMode::TexImage);
        assert_eq!(latex.as_key_bytes(), (0, 0, 1));
    }
}
