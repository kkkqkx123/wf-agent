//! The LLM authentication panel: declared auth surfaces, their
//! stored-credential status and the live login-session view.
//!
//! The panel is pure data: the session controller owns the HTTP driving
//! (start / poll / cancel / logout) and pushes results in through
//! [`AuthPanel::set_rows`] / [`AuthPanel::set_session`]. Two modes share
//! one list:
//!
//! * surface mode lists providers; the item data is the provider id and
//!   `Select` starts a login;
//! * session mode lists the rendered login events; the item data is the
//!   session id and `Select` cancels the session.

use crate::panels::filter::{apply, PanelFilter};
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;

/// One declared auth surface as shown in the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthSurfaceRow {
    /// Provider id, matching provider definitions and profile references.
    pub id: String,
    /// Display name of the registered method, e.g. "Anthropic API key".
    pub display_name: String,
    /// `api_key` or `oauth`.
    pub method: String,
    /// Selector label for OAuth login options.
    pub login_label: Option<String>,
    /// True when OAuth access is subscription-backed (non-metered).
    pub is_subscription: bool,
    /// True when a credential is stored for this provider.
    pub has_credential: bool,
}

impl AuthSurfaceRow {
    /// One-line label: `id · display [stored|not logged in]`.
    fn label(&self) -> String {
        let state = if self.has_credential {
            "stored"
        } else {
            "not logged in"
        };
        format!("{} · {} ({})", self.id, self.display_name, state)
    }
}

/// Live login-session view: rendered challenge lines plus the input prompt
/// when the flow waits on the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuthSessionView {
    /// Session id driving poll / submit / cancel.
    pub session_id: String,
    /// Provider being authenticated.
    pub provider_id: String,
    /// Rendered event lines (authorization URL, device code, progress).
    pub lines: Vec<String>,
    /// Prompt to answer, when the session awaits input.
    pub awaiting_input: Option<String>,
    /// Server-advertised poll interval in seconds, when the flow carries one.
    pub poll_secs: Option<u64>,
}

/// The LLM authentication panel.
#[derive(Debug, Clone)]
pub struct AuthPanel {
    list: SelectList<String>,
    /// Filter text typed while the panel is open.
    filter: PanelFilter,
    /// Declared surfaces backing the surface-mode list; kept so the list
    /// can be rebuilt when a login session ends.
    rows: Vec<AuthSurfaceRow>,
    /// Active login session, when one is in flight.
    session: Option<AuthSessionView>,
}

impl AuthPanel {
    /// Build the panel from the declared auth surfaces.
    pub fn new(rows: &[AuthSurfaceRow]) -> Self {
        let mut panel = Self {
            list: SelectList::groups(Vec::new()),
            filter: PanelFilter::new(),
            rows: Vec::new(),
            session: None,
        };
        panel.set_rows(rows);
        panel
    }

    /// Replace the surface list (after login, logout or a refresh). An
    /// in-flight session survives the refresh so its view stays stable.
    pub fn set_rows(&mut self, rows: &[AuthSurfaceRow]) {
        self.rows = rows.to_vec();
        if let Some(session) = self.session.as_ref() {
            self.list = session_list(session);
            return;
        }
        self.rebuild_surface_list();
    }

    /// The id under the cursor: a provider id in surface mode, the session
    /// id in session mode.
    pub fn selected_id(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// The active login session, when one is in flight.
    pub fn session(&self) -> Option<&AuthSessionView> {
        self.session.as_ref()
    }

    /// Install or clear the live login-session view. Clearing the session
    /// rebuilds the surface-mode list from the stored rows.
    pub fn set_session(&mut self, session: Option<AuthSessionView>) {
        self.session = session;
        if let Some(session) = self.session.as_ref() {
            self.list = session_list(session);
        } else {
            self.rebuild_surface_list();
        }
    }

    /// Rebuild the surface-mode list from the stored rows, keeping the
    /// filter projection active.
    fn rebuild_surface_list(&mut self) {
        let mut group = Group::new(Some("llm auth"));
        for row in &self.rows {
            let described = row.login_label.clone().unwrap_or_else(|| row.method.clone());
            group = group.item(GroupItem::new(row.label(), row.id.clone()).described(described));
        }
        self.list = SelectList::groups(vec![group]);
        apply(&mut self.list, self.filter.text());
    }

    /// Apply a keymap action; returns whether it was consumed.
    pub fn handle(&mut self, action: KeyAction) -> bool {
        match action {
            KeyAction::MovePrev => {
                self.list.navigate(NavigateDir::Prev);
                true
            }
            KeyAction::MoveNext => {
                self.list.navigate(NavigateDir::Next);
                true
            }
            KeyAction::Clear => {
                self.filter.clear();
                apply(&mut self.list, self.filter.text());
                true
            }
            _ => false,
        }
    }

    /// Append a character to the filter (surface mode only).
    pub fn filter_push(&mut self, c: char) {
        if self.session.is_some() {
            return;
        }
        self.filter.push(c);
        apply(&mut self.list, self.filter.text());
    }

    /// Remove the last grapheme from the filter (surface mode only).
    pub fn filter_backspace(&mut self) {
        if self.session.is_some() {
            return;
        }
        self.filter.backspace();
        apply(&mut self.list, self.filter.text());
    }

    /// The filter text typed so far.
    pub fn filter_text(&self) -> &str {
        self.filter.text()
    }

    /// Render the panel rows.
    pub fn render_lines(
        &self,
        width: u16,
        window_height: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        self.list.render_lines(width, window_height)
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }
}

/// The list shown while a login session is in flight: the rendered event
/// lines plus the input prompt when one is pending.
fn session_list(session: &AuthSessionView) -> SelectList<String> {
    let mut group = Group::new(Some(&format!("login · {}", session.provider_id)));
    for line in &session.lines {
        group = group.item(GroupItem::new(line.clone(), session.session_id.clone()));
    }
    if let Some(prompt) = &session.awaiting_input {
        group = group.item(GroupItem::new(
            format!("input needed: {prompt}"),
            session.session_id.clone(),
        ));
    }
    group = group.item(GroupItem::new(
        "enter: cancel login".to_string(),
        session.session_id.clone(),
    ));
    SelectList::groups(vec![group])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, has_credential: bool) -> AuthSurfaceRow {
        AuthSurfaceRow {
            id: id.to_string(),
            display_name: format!("{id} display"),
            method: "api_key".to_string(),
            login_label: None,
            is_subscription: false,
            has_credential,
        }
    }

    #[test]
    fn surface_mode_selects_provider_ids() {
        let panel = AuthPanel::new(&[row("acme", false), row("stored", true)]);
        assert_eq!(panel.selected_id().as_deref(), Some("acme"));
        assert!(panel.session().is_none());
    }

    #[test]
    fn session_mode_swaps_the_list() {
        let mut panel = AuthPanel::new(&[row("acme", false)]);
        panel.set_session(Some(AuthSessionView {
            session_id: "login-1".to_string(),
            provider_id: "acme".to_string(),
            lines: vec!["open: https://acme.test/authorize".to_string()],
            awaiting_input: None,
            poll_secs: Some(5),
        }));
        assert_eq!(panel.session().map(|s| s.provider_id.as_str()), Some("acme"));
        assert_eq!(panel.selected_id().as_deref(), Some("login-1"));
        // Filtering is inert while a session owns the list.
        panel.filter_push('x');
        assert!(panel.filter_text().is_empty());
        panel.set_session(None);
        assert_eq!(panel.selected_id().as_deref(), Some("acme"));
    }

    #[test]
    fn labels_mark_credential_state() {
        assert!(row("acme", true).label().contains("stored"));
        assert!(row("acme", false).label().contains("not logged in"));
    }
}
