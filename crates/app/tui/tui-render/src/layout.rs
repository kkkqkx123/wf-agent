//! Unified frame layout: chat, diagram and management splits.
//!
//! All geometry decisions flow through this module so narrow terminals and
//! diagram expansion behave consistently across session and management
//! screens.

use ratatui::layout::Rect;

/// Where a diagram pane sits relative to the chat area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramPosition {
    #[default]
    Side,
    Top,
}

/// Computed frame panes for one draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePanes {
    pub chat: Rect,
    pub diagram: Option<Rect>,
    pub aspect_bucket: u8,
}

/// Minimum chat width preserving a usable transcript column.
pub const MIN_CHAT_WIDTH: u16 = 30;
/// Minimum chat height preserving viewport context.
pub const MIN_CHAT_HEIGHT: u16 = 6;

/// Aspect bucket for diagram geometry: height-dominant frames reflow
/// diagrams, so the bucket participates in frame cache identity.
pub fn aspect_bucket(width: u16, height: u16) -> u8 {
    if height == 0 {
        return 0;
    }
    let ratio = (f32::from(width) / f32::from(height.max(1)) * 10.0).round() as u8;
    ratio.min(9)
}

/// Split `total` into chat plus an optional diagram pane.
pub fn split_panes(
    total: Rect,
    diagram_requested: u16,
    position: DiagramPosition,
    max_ratio: f32,
) -> FramePanes {
    let bucket = aspect_bucket(total.width, total.height);
    if diagram_requested == 0 {
        return FramePanes {
            chat: total,
            diagram: None,
            aspect_bucket: bucket,
        };
    }
    match position {
        DiagramPosition::Side => {
            let max_width = ((f32::from(total.width) * max_ratio).round() as u16).max(20);
            let diagram_width = diagram_requested.clamp(20, max_width);
            if total.width <= MIN_CHAT_WIDTH + 20 || diagram_width >= total.width {
                return FramePanes {
                    chat: total,
                    diagram: None,
                    aspect_bucket: bucket,
                };
            }
            let chat = Rect {
                width: total.width - diagram_width,
                ..total
            };
            let diagram = Rect {
                x: total.x + chat.width,
                width: diagram_width,
                ..total
            };
            FramePanes {
                chat,
                diagram: Some(diagram),
                aspect_bucket: bucket,
            }
        }
        DiagramPosition::Top => {
            let max_height = ((f32::from(total.height) * max_ratio).round() as u16).max(8);
            let diagram_height = diagram_requested.clamp(8, max_height);
            if total.height <= MIN_CHAT_HEIGHT + 8 || diagram_height >= total.height {
                return FramePanes {
                    chat: total,
                    diagram: None,
                    aspect_bucket: bucket,
                };
            }
            let diagram = Rect {
                height: diagram_height,
                ..total
            };
            let chat = Rect {
                y: total.y + diagram_height,
                height: total.height - diagram_height,
                ..total
            };
            FramePanes {
                chat,
                diagram: Some(diagram),
                aspect_bucket: bucket,
            }
        }
    }
}

/// Diagram allocation verdict for headless geometry assertions: what was
/// requested, what the transcript keeps, and whether a diagram pane was
/// granted. A refused diagram keeps the whole frame for the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagramProbe {
    /// Requested diagram width (side) or height (top) in cells.
    pub requested: u16,
    /// Granted diagram pane, if the frame could share it.
    pub granted: Option<Rect>,
    /// Transcript area after the split.
    pub chat: Rect,
}

/// Probe the diagram split without drawing: records the allocation verdict
/// so regression tests assert geometry instead of pixels.
pub fn probe_diagram(
    total: Rect,
    requested: u16,
    position: DiagramPosition,
    max_ratio: f32,
) -> DiagramProbe {
    let panes = split_panes(total, requested, position, max_ratio);
    DiagramProbe {
        requested,
        granted: panes.diagram,
        chat: panes.chat,
    }
}

/// Split a management frame into header plus body rows.
pub fn split_management(total: Rect, header_height: u16) -> (Rect, Rect) {
    if total.height <= header_height {
        return (total, total);
    }
    let header = Rect {
        height: header_height,
        ..total
    };
    let body = Rect {
        y: total.y + header_height,
        height: total.height - header_height,
        ..total
    };
    (header, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_diagram_request_keeps_full_chat() {
        let total = Rect::new(0, 0, 80, 24);
        let panes = split_panes(total, 0, DiagramPosition::Side, 0.4);
        assert_eq!(panes.chat, total);
        assert_eq!(panes.diagram, None);
    }

    #[test]
    fn narrow_frame_refuses_diagram() {
        let total = Rect::new(0, 0, 40, 24);
        let panes = split_panes(total, 30, DiagramPosition::Side, 0.4);
        assert_eq!(panes.diagram, None);
    }

    #[test]
    fn aspect_bucket_tracks_orientation() {
        assert_ne!(aspect_bucket(80, 24), aspect_bucket(24, 80));
    }

    #[test]
    fn diagram_probe_reports_grant_and_refusal() {
        let total = Rect::new(0, 0, 100, 30);
        let granted = probe_diagram(total, 30, DiagramPosition::Side, 0.4);
        assert_eq!(granted.requested, 30);
        let diagram = granted.granted.expect("diagram fits");
        assert_eq!(granted.chat.width + diagram.width, 100);
        let refused = probe_diagram(total, 0, DiagramPosition::Side, 0.4);
        assert_eq!(refused.granted, None);
        assert_eq!(refused.chat, total);
    }
}
