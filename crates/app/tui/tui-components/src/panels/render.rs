//! Shared render helper and [`Renderable`] implementations for every panel.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;

use tui_core::renderable::Renderable;

use super::command::CommandPalette;
use super::mention::MentionPanel;
use super::model::ModelPanel;
use super::queued::QueuedPanel;
use super::skill::SkillPanel;
use super::workflow::WorkflowPanel;

/// Helper: render `lines` into `area` of `buf`, clipping to the area height.
fn render_lines_into(area: Rect, buf: &mut Buffer, lines: &[Line<'static>]) {
    for (i, line) in lines.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }
        let row = Rect {
            x: area.x,
            y: area.y + i as u16,
            width: area.width,
            height: 1,
        };
        crate::footer::render_line_into(row, buf, line);
    }
}

impl Renderable for CommandPalette {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_lines(area.width, area.height);
        render_lines_into(area, buf, &lines);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        16
    }
}

impl Renderable for ModelPanel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_lines(area.width, area.height);
        render_lines_into(area, buf, &lines);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        16
    }
}

impl Renderable for SkillPanel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_lines(area.width, area.height);
        render_lines_into(area, buf, &lines);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        16
    }
}

impl Renderable for WorkflowPanel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_lines(area.width, area.height);
        render_lines_into(area, buf, &lines);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        16
    }
}

impl Renderable for MentionPanel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_lines(area.width, area.height);
        render_lines_into(area, buf, &lines);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        16
    }
}

impl Renderable for QueuedPanel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_lines(area.width, area.height);
        render_lines_into(area, buf, &lines);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        16
    }
}
