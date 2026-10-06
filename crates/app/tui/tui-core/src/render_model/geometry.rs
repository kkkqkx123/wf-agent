//! Headless geometry probes: frame splitting and unified layout snapshots.

use ratatui::layout::Rect;

/// Key rectangles a headless test can assert without pixel comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameGeometry {
    /// Full frame area.
    pub frame: Rect,
    /// Scrollback viewport area.
    pub viewport: Rect,
    /// Optional animation bounding box.
    pub anim_bounds: Option<Rect>,
}

impl FrameGeometry {
    /// Split a frame into scrollback viewport plus footer/input rows.
    pub fn split(frame: Rect, footer_height: u16, input_height: u16) -> Self {
        let viewport_height = frame
            .height
            .saturating_sub(footer_height + input_height + 1);
        let viewport = Rect {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: viewport_height,
        };
        Self {
            frame,
            viewport,
            anim_bounds: None,
        }
    }

    /// Attach the recorded animation bounds so animation-only frames can
    /// assert the exact cells they may touch.
    pub fn with_anim_bounds(mut self, area: crate::redraw::AnimationArea) -> Self {
        self.anim_bounds = area.bounds();
        self
    }
}

/// Key layout rectangles a headless test asserts without pixel comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutProbe {
    /// Scrollback viewport area.
    pub viewport: Rect,
    /// Footer/status area.
    pub footer: Rect,
    /// Input/composer area.
    pub input: Rect,
}

/// Split a frame into viewport, footer and input rectangles for the given
/// row heights. The probe carries no pixels, only geometry.
pub fn probe_layout(frame: Rect, footer_height: u16, input_height: u16) -> LayoutProbe {
    let viewport_height = frame.height.saturating_sub(footer_height + input_height);
    let footer_y = frame.y + viewport_height;
    LayoutProbe {
        viewport: Rect {
            height: viewport_height,
            ..frame
        },
        footer: Rect {
            y: footer_y,
            height: footer_height.min(frame.height.saturating_sub(viewport_height)),
            ..frame
        },
        input: Rect {
            y: footer_y + footer_height.min(frame.height.saturating_sub(viewport_height)),
            height: input_height.min(frame.height.saturating_sub(viewport_height + footer_height)),
            ..frame
        },
    }
}

/// Unified geometry snapshot: viewport, footer, input, diagram and sidebar
/// rectangles plus the decision inputs that produced them. Regression tests
/// assert this struct instead of pixels; production fills it from the same
/// layout entry points the draw path uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnifiedGeometryProbe {
    pub viewport: Rect,
    pub footer: Rect,
    pub input: Rect,
    pub diagram: Option<Rect>,
    pub sidebar: Option<Rect>,
    pub aspect_bucket: u8,
    pub diagram_requested: u16,
    pub max_ratio: u32,
    pub focused: bool,
    pub overlay_active: bool,
}

/// Probe unified geometry without drawing.
pub fn probe_unified_geometry(
    frame: Rect,
    footer_height: u16,
    input_height: u16,
    diagram_requested: u16,
    sidebar_active: bool,
    focused: bool,
    overlay_active: bool,
) -> UnifiedGeometryProbe {
    let layout = probe_layout(frame, footer_height, input_height);
    let aspect_bucket = {
        let h = frame.height.max(1);
        ((f32::from(frame.width) / f32::from(h) * 10.0).round() as u8).min(9)
    };
    let sidebar = sidebar_active.then(|| Rect {
        x: frame.x,
        y: frame.y,
        width: (f32::from(frame.width) * 0.3) as u16,
        height: frame.height.saturating_sub(1),
    });
    let diagram = if diagram_requested == 0 {
        None
    } else {
        let max_width = ((f32::from(frame.width) * 0.4).round() as u16).max(20);
        let w = diagram_requested.clamp(20, max_width);
        if frame.width <= 50 || w >= frame.width {
            None
        } else {
            Some(Rect {
                x: frame.x + frame.width - w,
                width: w,
                ..frame
            })
        }
    };
    UnifiedGeometryProbe {
        viewport: layout.viewport,
        footer: layout.footer,
        input: layout.input,
        diagram,
        sidebar,
        aspect_bucket,
        diagram_requested,
        max_ratio: 40,
        focused,
        overlay_active,
    }
}
