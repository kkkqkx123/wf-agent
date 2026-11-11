//! TUI frame drawing: unified post-processing and recovered frames.

use std::io;
use std::time::{Duration, Instant};

use libc;

use crossterm::event::{self, Event, KeyEventKind, MouseEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::context_sidebar::{SidebarPlugin, SidebarSnapshot};
use crate::error::{CliError, CliResult};
use crate::framer::FrameRequester;
use crate::modal::ModalStack;
use crate::overlay::OverlayMode;
use crate::screens::{ScreenData, ScreenKind};
use crate::session_tabs::SessionTabs;
use crate::size::{ResizeDebouncer, Size};
use crate::state::AppState;
use crate::terminal::{CrosstermControl, TerminalGuard};
use crate::theme::{self, Theme};
use tui_core::command::{PendingSequence, SequenceMap};
use tui_core::keymap::Keymap;
use tui_core::plugin::{PluginHost, Slot};
use wf_client::WfClient;

use super::data_cache::DataResult;
use super::input_classify::{
    load_copy_target, load_mouse_opt_in, load_performance_tier_override, load_sidebar_opt_in,
    load_theme_choice, map_key,
};
use super::signals::{sigtstp_handler, theme_reload_signals};

use super::mouse::MousePress;
use super::TuiApp;
use super::*;

impl TuiApp {
    /// Draw one frame with unified post-processing: after every widget is
    /// composed the finished buffer passes the fixed theme, palette, emoji
    /// and image-cleanup order exactly once through the shared pipeline.
    /// Diagram geometry flows through [`crate::layout::split_panes`]: the
    /// chat column draws the session while a granted diagram pane registers
    /// deferred work and paints its placeholder. The frame tail flushes the
    /// deferred queue unconditionally so the cleanup proof advances every
    /// frame.
    pub(crate) fn draw_inner(
        &mut self,
        frame: &mut Frame,
        data: &ScreenData,
        requested: crate::redraw::RedrawScope,
    ) {
        // Partial scopes overwrite only their own cells, so the previous
        // composed frame seeds the surface first and the rest of the screen
        // survives. Without a matching previous frame the scope upgrades to
        // full, so a partial frame can never paint over a stale or blank
        // surface.
        let scope = if requested == crate::redraw::RedrawScope::Full
            || self.frame_cache.can_seed(frame.area())
        {
            requested
        } else {
            crate::redraw::RedrawScope::Full
        };
        if scope != crate::redraw::RedrawScope::Full {
            self.frame_cache.seed(frame.buffer_mut());
        }
        let area = frame.area();
        let diagram_requested = self
            .sessions
            .active()
            .map(|s| s.diagram_requested())
            .unwrap_or(0);
        let panes = crate::layout::split_panes(
            area,
            diagram_requested,
            crate::layout::DiagramPosition::Side,
            0.4,
        );
        // Reserve the bottom line for the transient notice, if any. With
        // more than one chat tab open the top line shows the tab strip.
        let notice = self.app.notice.current_text();
        let on_interactive = self.app.screen.navigation.current_kind() == ScreenKind::Interactive;
        let show_tabs = on_interactive && self.sessions.wants_strip();
        let (tab_area, chunks) = if show_tabs {
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                ])
                .split(panes.chat);
            (Some(parts[0]), vec![parts[1], parts[2]])
        } else {
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(panes.chat);
            (None, vec![parts[0], parts[1]])
        };
        if let Some(tab_area) = tab_area {
            self.tab_strip_area = Some(tab_area);
            let snapshot = self.sessions.snapshot();
            let items: Vec<crate::tab_bar::SessionTabItem> = snapshot
                .into_iter()
                .map(|(title, streaming, pinned)| {
                    let mut item = crate::tab_bar::SessionTabItem::new(title, streaming);
                    item.pinned = pinned;
                    item
                })
                .collect();
            crate::tab_bar::draw_session_tab_bar(
                frame,
                tab_area,
                &items,
                self.sessions.active_index(),
                &self.theme,
            );
        } else {
            self.tab_strip_area = None;
        }

        // Interactive is always the primary interface. When the sidebar is
        // enabled and the terminal is wide enough, the content column splits
        // into the chat view and the plugin sidebar on the right.
        if on_interactive {
            let show_sidebar = self.sidebar
                && self.sessions.active().is_some()
                && chunks[0].width >= SIDEBAR_MIN_WIDTH;
            let (main_area, sidebar_area) = if show_sidebar {
                let parts = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(40), Constraint::Length(SIDEBAR_WIDTH)])
                    .split(chunks[0]);
                (parts[0], Some(parts[1]))
            } else {
                (chunks[0], None)
            };
            // A covering layer must never promise clicks: drop the hover
            // before painting so no underline survives under modals,
            // blockers or full-screen overlays.
            if self.mouse_layer_suppressed() {
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_hover_cell();
                }
            }
            if let Some(session) = self.sessions.active_mut_if_renderable() {
                session.draw(frame, main_area, &self.theme, scope);
            }
            if let Some(area) = sidebar_area {
                let mut snapshot = self
                    .sessions
                    .active()
                    .map(|session| session.sidebar_snapshot())
                    .unwrap_or_default();
                snapshot.files = self.sidebar_fetch.files.clone().unwrap_or_default();
                snapshot.git = self.sidebar_fetch.git.clone().unwrap_or_default();
                snapshot.mcp = self.sidebar_fetch.mcp.clone().unwrap_or_default();
                self.plugins
                    .render_slot(Slot::SIDEBAR, frame, area, &self.theme, &snapshot);
            }
        }
        if !on_interactive || self.sessions.is_empty() {
            // Fallback to screens if no interactive controller is active
            let kind = self.app.screen.navigation.current_kind();
            let detail_key = self.current_detail_key();
            let detail_lines: &[String] = detail_key
                .as_ref()
                .and_then(|(key, _)| self.app.screen.detail_for(key))
                .map(|cached| cached.lines.as_slice())
                .unwrap_or(&[]);
            let ctx = crate::screens::ScreenContext {
                exec_view: self.app.screen.exec_view,
                workflow_view: self.app.screen.workflow_view,
                catalog_tab: self.app.screen.catalog_tab,
                detail_lines,
                detail_scroll: self.app.screen.detail_scroll,
                has_more: self.app.screen.is_truncated(kind),
                filter: self.app.screen.list_filter.as_str(),
            };
            self.app.screen.navigation.draw_with_settings_scroll(
                frame,
                chunks[0],
                data,
                &self.theme,
                self.app.screen.settings_scroll,
                &ctx,
            );
        }

        if let Some(diagram) = panes.diagram {
            self.deferred.defer_diagram("diagram");
            frame.render_widget(
                Paragraph::new("diagram").style(Style::default().fg(Color::DarkGray)),
                diagram,
            );
        }

        // Draw overlay if active. Management overlay drawing lives in the
        // screen module; the shell only decides when it is active.
        match self.overlay {
            OverlayMode::Sidebar => {
                crate::screen_draw::draw_sidebar_overlay(
                    frame,
                    area,
                    DASHBOARD_ENTRIES,
                    self.app.screen.navigation.selected(),
                );
            }
            OverlayMode::History => {
                let (mut view, panel) = self.history.split();
                let layout = crate::screen_draw::draw_transcript_overlay(
                    frame,
                    area,
                    &mut view,
                    Some(panel),
                    &self.theme,
                );
                let scroll = view.scroll;
                self.history.set_viewport(scroll, layout.at_top);
                self.history.set_layout(layout.content, layout.top_row);
                if scope == crate::redraw::RedrawScope::Full
                    || scope == crate::redraw::RedrawScope::BottomOnly
                {
                    self.history_hit_generation = self.history_hit_generation.wrapping_add(1);
                    self.history_badge = layout.badge;
                }
            }
            OverlayMode::SearchPalette => {
                let palette = crate::screen_draw::SearchPaletteView {
                    input: self.search_palette.input(),
                    query: self.search_palette.query(),
                    results: self.search_palette.results(),
                    truncated: self.search_palette.truncated(),
                    running: self.search_palette.running(),
                    selected: self.search_palette.selected(),
                    error: self.search_palette.error(),
                };
                self.search_palette_hit = Some(crate::screen_draw::draw_search_palette(
                    frame,
                    area,
                    &palette,
                    &self.theme,
                ));
            }
            OverlayMode::CommandPalette => {
                let picks = self.command_palette.picks();
                let rows: Vec<String> = picks
                    .iter()
                    .map(|pick| match pick {
                        tui_core::command::PalettePick::Builtin(spec) => {
                            let mut label = match spec.slash {
                                Some(slash) => format!("/{}", slash.name),
                                None => spec.title.to_string(),
                            };
                            if let Some(slash) = spec.slash {
                                if !slash.argument.is_empty() {
                                    label.push(' ');
                                    label.push_str(slash.argument);
                                }
                            }
                            let mut row = format!("{} — {}", label, spec.title);
                            if let Some(shortcut) = spec.shortcut() {
                                row.push_str(&format!(" ({shortcut})"));
                            }
                            row
                        }
                        tui_core::command::PalettePick::Plugin(index) => {
                            match self.command_palette.plugin_command(*index) {
                                Some(command) => {
                                    let mut row =
                                        format!("{} — {}", command.palette_label(), command.title);
                                    let origin = match command.origin {
                                        tui_core::command::CliCommandOrigin::Local => "local",
                                        tui_core::command::CliCommandOrigin::Snapshot => "server",
                                    };
                                    row.push_str(&format!(" [{origin}]"));
                                    if let Some(hint) = command.key_hint.as_deref() {
                                        row.push_str(&format!(" ({hint})"));
                                    }
                                    row
                                }
                                None => "plugin command (removed)".to_string(),
                            }
                        }
                    })
                    .collect();
                let view = crate::screen_draw::CommandPaletteView {
                    input: self.command_palette.filter(),
                    rows: &rows,
                    selected: self.command_palette.selected(),
                };
                self.command_palette_hit = Some(crate::screen_draw::draw_command_palette(
                    frame,
                    area,
                    &view,
                    &self.theme,
                ));
            }
            OverlayMode::SessionList => {
                let active = self.sessions.active_index();
                let rows: Vec<crate::screen_draw::SessionListRow<'_>> = self
                    .sessions
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| crate::screen_draw::SessionListRow {
                        title: tab.title.as_str(),
                        session_id: tab
                            .execution_id()
                            .or_else(|| tab.holder.session_id())
                            .unwrap_or("?"),
                        streaming: tab.streaming(),
                        pinned: tab.pinned,
                        active: index == active,
                    })
                    .collect();
                let recent: Vec<String> = self
                    .session_list
                    .recent_visible()
                    .iter()
                    .map(|row| row.display())
                    .collect();
                let view = crate::screen_draw::SessionListView {
                    tabs: &rows,
                    tab_selected: self.session_list.selected(),
                    show_recent: self.session_list.pane()
                        == crate::session_list::SessionListPane::Recent,
                    recent_rows: &recent,
                    recent_selected: self.session_list.recent_selected(),
                    recent_filter: self.session_list.recent_filter(),
                    recent_named_only: self.session_list.named_only(),
                    recent_pending: self.session_list.pending_delete_id(),
                };
                crate::screen_draw::draw_session_list(frame, area, &view, &self.theme);
            }
            OverlayMode::SessionTree => {
                if self.session_tree.is_loading() {
                    crate::screen_draw::draw_session_tree_placeholder(
                        frame,
                        area,
                        self.session_tree.session_id(),
                        "Loading session tree…",
                        &self.theme,
                    );
                } else if let Some(error) = self.session_tree.error() {
                    crate::screen_draw::draw_session_tree_placeholder(
                        frame,
                        area,
                        self.session_tree.session_id(),
                        &format!("failed to load: {error}"),
                        &self.theme,
                    );
                } else if let Some(session_id) = self.session_tree.session_id() {
                    let rows = self.session_tree.rows();
                    let status = self.session_tree.status_line();
                    let view = crate::screen_draw::SessionTreeView {
                        session_id,
                        rows: &rows,
                        selected: self.session_tree.selected(),
                        status: &status,
                        restore_prompt: self.session_tree.restore_prompt(),
                    };
                    crate::screen_draw::draw_session_tree(frame, area, &view, &self.theme);
                }
            }
            OverlayMode::None => {}
        }

        if let Some(text) = notice {
            frame.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::Yellow)),
                chunks[1],
            );
        }

        if !self.modals.is_empty() {
            self.modals.draw(frame, area, &self.theme);
        }

        let _deferred = self.deferred.flush();
        // Capture the composed frame before adaptation so the next partial
        // frame seeds pre-adaptation content and adaptation still runs exactly
        // once per frame over the whole buffer.
        self.frame_cache.capture(frame.buffer_mut());
        crate::post_process::finish_frame(
            frame.buffer_mut(),
            &self.theme,
            self.theme_mode,
            self.theme_explicit,
            crate::post_process::EmojiPreference::Native,
            &mut self.finish_cleanups,
        );
    }

    /// Render the panic fallback after a draw panic: the loop survives and
    /// the next frame paints normally again. A panicking widget truly
    /// desynchronizes the surface, so the next frame hard-clears before
    /// recomposing.
    pub(crate) fn draw_recovered(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> CliResult<()> {
        terminal
            .draw(|frame| {
                let area = frame.area();
                crate::render_model::draw_recovered_frame(frame.buffer_mut(), area);
            })
            .map_err(|e| CliError::Terminal(format!("recovered draw failed: {e}")))?;
        self.invalidation
            .request(crate::redraw::InvalidationKind::Hard);
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        Ok(())
    }
}
