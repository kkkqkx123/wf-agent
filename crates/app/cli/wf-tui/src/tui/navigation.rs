//! Navigation helpers: goto/go_back, screen scrolling, detail view cycling.

use crate::screens::ScreenKind;

use super::TuiApp;

impl TuiApp {
    pub(super) fn scroll_settings(&mut self, delta: isize) {
        if delta >= 0 {
            self.app.screen.settings_scroll = self
                .app
                .screen
                .settings_scroll
                .saturating_add(delta as usize);
        } else {
            self.app.screen.settings_scroll = self
                .app
                .screen
                .settings_scroll
                .saturating_sub(delta.unsigned_abs());
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Scroll the executions/workflow/catalog detail pane. Reports whether
    /// the current screen owns a detail pane so callers can fall through.
    pub(super) fn scroll_detail(&mut self, delta: isize) -> bool {
        if !matches!(
            self.app.screen.navigation.current_kind(),
            ScreenKind::Executions | ScreenKind::Workflow | ScreenKind::Catalog
        ) {
            return false;
        }
        if delta >= 0 {
            self.app.screen.detail_scroll =
                self.app.screen.detail_scroll.saturating_add(delta as usize);
        } else {
            self.app.screen.detail_scroll = self
                .app
                .screen
                .detail_scroll
                .saturating_sub(delta.unsigned_abs());
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        true
    }

    /// Cycle the executions/workflow subview and fetch its detail lines.
    /// On the catalog screen the same keys cycle the kind tab instead.
    pub(super) fn cycle_detail_view(&mut self, forward: bool) {
        match self.app.screen.navigation.current_kind() {
            ScreenKind::Executions => {
                let view = self.app.screen.exec_view;
                self.app.screen.exec_view = if forward { view.next() } else { view.prev() };
                self.request_detail_for_current();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            ScreenKind::Workflow => {
                let view = self.app.screen.workflow_view;
                self.app.screen.workflow_view = if forward { view.next() } else { view.prev() };
                self.request_detail_for_current();
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            ScreenKind::Catalog => {
                self.cycle_catalog_tab(forward);
            }
            _ => {}
        }
    }

    /// Cycle the catalog kind tab, wrapping around the five groups.
    pub(super) fn cycle_catalog_tab(&mut self, forward: bool) {
        use tui_core::screen_data::catalog_tab_from_index;
        let total = tui_core::screen_data::CatalogKind::ALL.len() + 1;
        let current = tui_core::screen_data::catalog_tab_index(self.app.screen.catalog_tab);
        let next = if forward {
            (current + 1) % total
        } else {
            (current + total - 1) % total
        };
        self.app
            .screen
            .switch_catalog_tab(catalog_tab_from_index(next));
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Number of selectable rows on the current screen.
    pub(super) fn nav_len(&self) -> usize {
        match self.app.screen.navigation.current_kind() {
            kind if kind.has_data() => self.display_data().row_count().max(1),
            _ => 1,
        }
    }

    pub(super) fn goto(&mut self, kind: ScreenKind) {
        self.app.screen.navigation.navigate_to(kind);
        self.app.screen.settings_scroll = 0;
        self.app.screen.detail_scroll = 0;
        self.app.screen.list_filter.clear();
        if kind == ScreenKind::Catalog {
            self.app.screen.reset_catalog();
        }
        self.request_data(kind);
        self.request_detail_for_current();
    }

    pub(super) fn go_back(&mut self) -> bool {
        if self.app.screen.navigation.go_back() {
            let kind = self.app.screen.navigation.current_kind();
            self.app.screen.settings_scroll = 0;
            self.app.screen.detail_scroll = 0;
            self.app.screen.list_filter.clear();
            self.request_data(kind);
            self.request_detail_for_current();
            true
        } else {
            false
        }
    }

}
