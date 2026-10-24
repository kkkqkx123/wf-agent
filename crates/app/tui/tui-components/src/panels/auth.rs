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

/// One selectable login input option.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthPromptOption {
    /// Option id submitted to the host.
    pub id: String,
    /// Human label rendered as `[N] label`.
    pub label: String,
}

impl AuthPromptOption {
    /// New prompt option.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
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
    /// True when the pending input is a secret and must be masked.
    pub secret: bool,
    /// Select options backing a numbered choice prompt.
    pub options: Vec<AuthPromptOption>,
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
    /// Pending login input typed while the session awaits it.
    pending_input: String,
}

impl AuthPanel {
    /// Build the panel from the declared auth surfaces.
    pub fn new(rows: &[AuthSurfaceRow]) -> Self {
        let mut panel = Self {
            list: SelectList::groups(Vec::new()),
            filter: PanelFilter::new(),
            rows: Vec::new(),
            session: None,
            pending_input: String::new(),
        };
        panel.set_rows(rows);
        panel
    }

    /// Replace the surface list (after login, logout or a refresh). An
    /// in-flight session survives the refresh so its view stays stable.
    pub fn set_rows(&mut self, rows: &[AuthSurfaceRow]) {
        self.rows = rows.to_vec();
        if let Some(session) = self.session.as_ref() {
            let pending = self.pending_input.clone();
            self.list = session_list(session, &pending);
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
        let next_id = session.as_ref().map(|view| view.session_id.clone());
        let current_id = self.session.as_ref().map(|view| view.session_id.clone());
        if next_id != current_id {
            self.pending_input.clear();
        }
        self.session = session;
        if let Some(session) = self.session.as_ref() {
            let pending = self.pending_input.clone();
            self.list = session_list(session, &pending);
        } else {
            self.pending_input.clear();
            self.rebuild_surface_list();
        }
    }

    /// Rebuild the surface-mode list from the stored rows, keeping the
    /// filter projection active.
    fn rebuild_surface_list(&mut self) {
        let mut group = Group::new(Some("llm auth"));
        for row in &self.rows {
            let described = row
                .login_label
                .clone()
                .unwrap_or_else(|| row.method.clone());
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

    /// Whether the active session waits for typed input.
    pub fn is_awaiting_input(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.awaiting_input.is_some())
    }

    /// Pending login input typed for the active session.
    pub fn auth_input_text(&self) -> &str {
        &self.pending_input
    }

    /// Append a character to the pending login input and refresh the view.
    pub fn auth_input_push(&mut self, c: char) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if session.awaiting_input.is_none() {
            return;
        }
        self.pending_input.push(c);
        self.list = session_list(&session, &self.pending_input);
    }

    /// Remove the last character from the pending login input.
    pub fn auth_input_backspace(&mut self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if session.awaiting_input.is_none() {
            return;
        }
        self.pending_input.pop();
        self.list = session_list(&session, &self.pending_input);
    }

    /// Take the pending login input, clearing the buffer and refreshing the
    /// session view.
    pub fn take_auth_input(&mut self) -> String {
        let taken = std::mem::take(&mut self.pending_input);
        if let Some(session) = self.session.clone() {
            self.list = session_list(&session, self.pending_input.as_str());
        }
        taken
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

/// Mask a secret login input for display, keeping its length visible.
fn mask_input(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    "•".repeat(input.chars().count())
}

/// The list shown while a login session is in flight: the rendered event
/// lines plus the input prompt when one is pending.
fn session_list(session: &AuthSessionView, pending_input: &str) -> SelectList<String> {
    let mut group = Group::new(Some(&format!("login · {}", session.provider_id)));
    for line in &session.lines {
        group = group.item(GroupItem::new(line.clone(), session.session_id.clone()));
    }
    if let Some(prompt) = &session.awaiting_input {
        group = group.item(GroupItem::new(
            format!("input needed: {prompt}"),
            session.session_id.clone(),
        ));
        let shown = if session.secret {
            mask_input(pending_input)
        } else {
            pending_input.to_string()
        };
        group = group.item(GroupItem::new(
            format!("input> {shown}"),
            session.session_id.clone(),
        ));
        group = group.item(GroupItem::new(
            "enter: submit · esc: cancel".to_string(),
            session.session_id.clone(),
        ));
    } else {
        group = group.item(GroupItem::new(
            "enter: cancel login".to_string(),
            session.session_id.clone(),
        ));
    }
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
            secret: false,
            options: Vec::new(),
        }));
        assert_eq!(
            panel.session().map(|s| s.provider_id.as_str()),
            Some("acme")
        );
        assert_eq!(panel.selected_id().as_deref(), Some("login-1"));
        // Filtering is inert while a session owns the list.
        panel.filter_push('x');
        assert!(panel.filter_text().is_empty());
        panel.set_session(None);
        assert_eq!(panel.selected_id().as_deref(), Some("acme"));
    }

    #[test]
    fn awaiting_input_collects_masked_text() {
        let mut panel = AuthPanel::new(&[row("acme", false)]);
        panel.set_session(Some(AuthSessionView {
            session_id: "login-1".to_string(),
            provider_id: "acme".to_string(),
            lines: Vec::new(),
            awaiting_input: Some("API key:".to_string()),
            poll_secs: None,
            secret: true,
            options: Vec::new(),
        }));
        assert!(panel.is_awaiting_input());
        panel.auth_input_push('s');
        panel.auth_input_push('k');
        assert_eq!(panel.auth_input_text(), "sk");
        panel.auth_input_backspace();
        assert_eq!(panel.auth_input_text(), "s");
        assert_eq!(panel.take_auth_input(), "s");
        assert_eq!(panel.auth_input_text(), "");
    }

    #[test]
    fn labels_mark_credential_state() {
        assert!(row("acme", true).label().contains("stored"));
        assert!(row("acme", false).label().contains("not logged in"));
    }
}
