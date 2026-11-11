//! Mouse press handling: button routing, palette and tab-strip landing.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use tui_core::panel_select::TextPos;

use crate::interactive::TranscriptSelection;
use crate::modal::{ModalMouse, ModalMouseKind};
use crate::overlay::OverlayMode;
use crate::screens::ScreenKind;

use super::super::input_classify::{
    classify_mouse, click_kind_for_count, mouse_bypasses_capture, mouse_is_left_button,
    mouse_is_right_button, next_click_count, ClickKind, MouseAction, MouseOutcome,
};
use super::super::TuiApp;
use super::*;

impl TuiApp {
    pub(crate) fn handle_mouse_press(&mut self, mouse: MouseEvent) -> bool {
        let is_right = mouse_is_right_button(mouse.kind);
        if !mouse_is_left_button(mouse.kind) && !is_right {
            return false;
        }
        if self.mouse_layer_suppressed() {
            return false;
        }
        if !is_right
            && self.overlay == OverlayMode::None
            && self.app.screen.navigation.current_kind() == ScreenKind::Interactive
        {
            if let Some(hit) = self.tab_strip_hit_at(mouse.column, mouse.row) {
                return self.land_tab_strip_hit(hit);
            }
        }
        // Palette overlays own their rows: a left press inside the list
        // arms a same-cell release that focuses (search) or runs
        // (command) the row. Clicks outside the list stay suppressed.
        if !is_right
            && matches!(
                self.overlay,
                OverlayMode::CommandPalette | OverlayMode::SearchPalette
            )
        {
            return self.handle_palette_press(mouse);
        }
        // A press never drops a previous drag selection; the release
        // decides: a same-cell release with a prior selection keeps it
        // and runs no click, a drag replaces it below. Alt+Click keeps
        // the committed regions so several disjoint ranges can be copied.
        let transcript_add = !is_right && mouse.modifiers.contains(KeyModifiers::ALT);
        let had_selection = !transcript_add
            && self
                .sessions
                .active()
                .is_some_and(|session| session.has_transcript_selection());
        if transcript_add {
            if let Some(session) = self.sessions.active_mut() {
                session.begin_additive_transcript_selection();
            }
        }
        let in_history = self.overlay == OverlayMode::History;
        if !in_history && self.app.screen.navigation.current_kind() != ScreenKind::Interactive {
            return false;
        }
        if !in_history {
            let inside_scroll = self.sessions.active().is_some_and(|session| {
                session.last_scroll_area().is_some_and(|area| {
                    mouse.column >= area.x
                        && mouse.column < area.x.saturating_add(area.width)
                        && mouse.row >= area.y
                        && mouse.row < area.y.saturating_add(area.height)
                })
            });
            if !inside_scroll {
                let inside_composer = self.sessions.active().is_some_and(|session| {
                    session.last_input_area().is_some_and(|area| {
                        crate::interactive::mouse_hit::composer_contains(
                            &area,
                            mouse.column,
                            mouse.row,
                        )
                    })
                });
                if !inside_composer {
                    return false;
                }
                let session_gen = self
                    .sessions
                    .active()
                    .map(|session| session.hit_generation())
                    .unwrap_or(0);
                self.mouse_press = Some(MousePress {
                    col: mouse.column,
                    row: mouse.row,
                    session_gen,
                    history_gen: self.history_hit_generation,
                    in_history,
                    in_gutter: false,
                    gutter_thumb: false,
                    in_composer: true,
                    transcript_anchor: None,
                    history_anchor: false,
                    transcript_add: false,
                    had_selection: false,
                    is_right,
                    palette: None,
                    palette_rows: 0,
                });
                self.clear_mouse_hover();
                return false;
            }
        }
        // Palette overlays own their rows (see the early press routing
        // above); the scrollback paths below never run for them.
        let session_gen = self
            .sessions
            .active()
            .map(|session| session.hit_generation())
            .unwrap_or(0);
        let (in_gutter, gutter_thumb) = if in_history {
            (false, false)
        } else {
            self.sessions.active().map_or((false, false), |session| {
                let gutter = session.session_gutter_rect();
                let hit = gutter.is_some_and(|rect| {
                    tui_components::scrollbar::gutter_contains(rect, mouse.column, mouse.row)
                });
                if hit {
                    let thumb = session.gutter_press_on_thumb(mouse.row);
                    (true, thumb)
                } else {
                    (false, false)
                }
            })
        };
        if in_gutter && gutter_thumb && !is_right {
            if let Some(session) = self.sessions.active_mut() {
                session.begin_gutter_drag();
            }
        }
        // A fresh left press in the history pager arms a drag selection; any
        // previous region is dropped so each press starts a new range.
        let history_anchor = if in_history && !is_right {
            self.history.clear_selection();
            self.history.begin_selection(mouse.column, mouse.row)
        } else {
            false
        };
        self.mouse_press = Some(MousePress {
            col: mouse.column,
            row: mouse.row,
            session_gen,
            history_gen: self.history_hit_generation,
            in_history,
            in_gutter,
            gutter_thumb,
            in_composer: false,
            transcript_anchor: if is_right || in_gutter {
                None
            } else {
                self.content_anchor_for(mouse)
            },
            history_anchor,
            transcript_add,
            had_selection,
            is_right,
            palette: None,
            palette_rows: 0,
        });
        self.clear_mouse_hover();
        false
    }

    /// Arm one palette row press. Only left presses inside the published
    /// list area arm; clicks outside the list stay suppressed without
    /// arming so the overlay never closes by accident.
    pub(crate) fn handle_palette_press(&mut self, mouse: MouseEvent) -> bool {
        let overlay = self.overlay;
        let layout = match overlay {
            OverlayMode::CommandPalette => self.command_palette_hit.clone(),
            OverlayMode::SearchPalette => self.search_palette_hit.clone(),
            _ => None,
        };
        let Some(layout) = layout else {
            return false;
        };
        if layout.item_at(mouse.column, mouse.row).is_none() {
            return false;
        }
        self.mouse_press = Some(MousePress {
            col: mouse.column,
            row: mouse.row,
            session_gen: 0,
            history_gen: 0,
            in_history: false,
            in_gutter: false,
            gutter_thumb: false,
            in_composer: false,
            transcript_anchor: None,
            history_anchor: false,
            transcript_add: false,
            had_selection: false,
            is_right: false,
            palette: Some(overlay),
            palette_rows: layout.targets.len(),
        });
        self.clear_mouse_hover();
        false
    }

    /// Land one palette release: same overlay, same cell, same displayed
    /// row count. Command rows focus and arm an async run; search rows
    /// only focus, matching the keyboard (results have no open action).
    pub(crate) fn land_palette_release(
        &mut self,
        palette: OverlayMode,
        palette_rows: usize,
        mouse: MouseEvent,
    ) -> bool {
        let layout = match palette {
            OverlayMode::CommandPalette => self.command_palette_hit.clone(),
            OverlayMode::SearchPalette => self.search_palette_hit.clone(),
            _ => None,
        };
        let Some(layout) = layout else {
            return false;
        };
        if layout.targets.len() != palette_rows {
            return false;
        }
        let Some(index) = layout.item_at(mouse.column, mouse.row) else {
            return false;
        };
        match palette {
            OverlayMode::CommandPalette => {
                self.command_palette.select(index);
                self.pending_mouse_action = Some(PendingMouseAction::RunPaletteCommand);
                true
            }
            OverlayMode::SearchPalette => {
                self.search_palette.select(index);
                true
            }
            _ => false,
        }
    }
}
