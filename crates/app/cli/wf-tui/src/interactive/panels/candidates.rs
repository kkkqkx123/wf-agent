//! Panel candidates: fetch profiles, skills, workflows and auth.

use super::auth::parse_auth_row;
use super::*;
use crate::footer::FooterRoute;
use crate::interactive::{
    InteractiveAction, InteractiveController, InteractiveEvent, PanelPayload,
};
use crate::keymap::{CKey, Key};
use tui_components::footer::PanelState;
use tui_components::panels::{
    AuthPanel, AuthPromptOption, AuthSessionView, AuthSurfaceRow, CommandPalette, MentionEntry,
    MentionPanel, ModelPanel, QueuedPanel, SkillPanel, WorkflowPanel, WorkflowSummaryRow,
};
use tui_core::command::{self, CommandRegistry, CommandSpec, RuntimeCommand};
use tui_core::keymap::KeyAction;

impl InteractiveController {
    /// Apply landed candidates to the cache and to the open panel. Results
    /// for a route the user already left are dropped.
    pub(crate) fn apply_panel_candidates(&mut self, route: FooterRoute, payload: PanelPayload) {
        match payload {
            PanelPayload::Profiles(profiles) => {
                self.panel_profiles = profiles;
                if self.footer.route != FooterRoute::Model {
                    return;
                }
                let panel = ModelPanel::new(&self.panel_profiles, self.model_override.as_deref());
                self.footer.panel = Some(PanelState::Model(panel));
            }
            PanelPayload::Skills(skills) => {
                self.panel_skills = skills;
                if self.footer.route != FooterRoute::Skill {
                    return;
                }
                let panel = SkillPanel::new(&self.panel_skills);
                self.footer.panel = Some(PanelState::Skill(panel));
            }
            PanelPayload::Workflows(workflows) => {
                self.panel_workflows = workflows;
                if self.footer.route != FooterRoute::Workflow {
                    return;
                }
                let panel = WorkflowPanel::new(&self.panel_workflows);
                self.footer.panel = Some(PanelState::Workflow(panel));
            }
            PanelPayload::MentionFiles(entries) => {
                self.panel_mention_files = entries;
                self.refresh_mention_entries();
            }
            PanelPayload::Auth(rows) => {
                if rows.is_empty() && !self.panel_auth.is_empty() {
                    // A refresh landed (login / logout): keep the cache warm
                    // only when it actually carried rows.
                    self.panel_auth.clear();
                } else if !rows.is_empty() {
                    self.panel_auth = rows;
                }
                if self.footer.route != FooterRoute::Auth {
                    return;
                }
                let session = match self.footer.panel.as_ref() {
                    Some(PanelState::Auth(panel)) => panel.session().cloned(),
                    _ => None,
                };
                let mut panel = AuthPanel::new(&self.panel_auth);
                panel.set_session(session);
                self.footer.panel = Some(PanelState::Auth(panel));
            }
            PanelPayload::AuthSession {
                session_id,
                lines,
                awaiting_input,
                poll_secs,
                secret,
                options,
                outcome,
                error,
            } => {
                let outcome = outcome.as_deref().unwrap_or_default();
                if self.footer.route != FooterRoute::Auth {
                    return;
                }
                match outcome {
                    "completed" => {
                        self.panel_auth.clear();
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(None);
                        }
                        self.fetch_panel_auth();
                        self.footer.show_notice(format!("logged in: {session_id}"));
                    }
                    "failed" => {
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(None);
                        }
                        let message = error.unwrap_or_else(|| "login failed".to_string());
                        self.footer.show_notice(format!("login failed: {message}"));
                    }
                    "cancelled" => {
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(None);
                        }
                    }
                    _ => {
                        let provider_id = match self.footer.panel.as_ref() {
                            Some(PanelState::Auth(panel)) => {
                                panel.session().map(|s| s.provider_id.clone())
                            }
                            _ => None,
                        }
                        .unwrap_or_default();
                        let view = AuthSessionView {
                            session_id: session_id.clone(),
                            provider_id,
                            lines,
                            awaiting_input: awaiting_input.clone(),
                            poll_secs,
                            secret,
                            options,
                        };
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(Some(view));
                        }
                        if awaiting_input.is_some() {
                            self.footer.show_notice(
                                "type input + Enter to submit, Esc to cancel".to_string(),
                            );
                        } else {
                            let delay = poll_secs.unwrap_or(AUTH_POLL_FALLBACK_SECS);
                            self.poll_auth_session(&session_id, delay);
                        }
                    }
                }
            }
            PanelPayload::MentionEntities { query, entries } => {
                self.mention_search_inflight = false;
                // A response for an outdated query never replaces the
                // candidates of the query under the cursor.
                let current = self.footer.composer.mention_query();
                if current.as_deref() != Some(query.as_str()) {
                    return;
                }
                self.panel_mention_entities = entries;
                self.refresh_mention_entries();
            }
        }
        let _ = route;
    }

    /// Load the model profiles for the model panel.
    pub(crate) fn fetch_panel_profiles(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client
                .get_json::<serde_json::Value>("/api/v1/llm/profiles")
                .await
            {
                Ok(value) => {
                    let profiles = items_of(&value)
                        .iter()
                        .filter_map(parse_profile)
                        .collect::<Vec<_>>();
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Model,
                        payload: PanelPayload::Profiles(profiles),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Model,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Load the skills for the skill panel.
    pub(crate) fn fetch_panel_skills(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.get_json::<serde_json::Value>("/api/v1/skills").await {
                Ok(value) => {
                    let skills =
                        serde_json::from_value::<Vec<wf_types::skill::SkillMetadata>>(value)
                            .unwrap_or_default();
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Skill,
                        payload: PanelPayload::Skills(skills),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Skill,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Load the workflow summaries for the workflow panel.
    pub(crate) fn fetch_panel_workflows(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client
                .get_json::<serde_json::Value>("/api/v1/workflows/summaries")
                .await
            {
                Ok(value) => {
                    let workflows = WorkflowSummaryRow::rows_for(&value);
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Workflow,
                        payload: PanelPayload::Workflows(workflows),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Workflow,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Load the declared LLM auth surfaces for the auth panel.
    pub(crate) fn fetch_panel_auth(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client
                .get_json::<serde_json::Value>("/api/v1/llm/auth")
                .await
            {
                Ok(value) => {
                    let rows = items_of(&value)
                        .iter()
                        .filter_map(parse_auth_row)
                        .collect::<Vec<_>>();
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Auth,
                        payload: PanelPayload::Auth(rows),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Auth,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    // ── auth login sessions ───────────────────────────────────────────
}
