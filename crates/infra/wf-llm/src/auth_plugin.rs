//! Bridge a plugin-contributed [`PluginLlmAuth`] onto the host OAuth auth
//! surface.
//!
//! A plugin describes an authentication implementation (endpoints, client
//! id, flow) through the synchronous contribution contract; the host drives
//! it as a [`LoginFlow::External`] session. Structured values cross the
//! boundary as JSON: the declared auth parameters go in, events and prompts
//! come out, and the credential produced at the end is parsed into the host
//! [`Credential`] the registry persists. `refresh` and `resolve` are
//! pass-through calls with the credential as JSON.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use wf_plugin_sdk::{
    AuthDescriptor, PluginAuthEvent, PluginAuthNext, PluginAuthOption, PluginAuthPrompt,
    PluginLlmAuth,
};

use llm_auth::auth::{ModelAuth, OAuthAuth};
use llm_auth::credential::{Credential, OAuthCredential};
use llm_auth::error::{AuthError, AuthResult};
use llm_auth::interaction::{AuthEvent, AuthInfoLink, AuthPrompt, AuthPromptOption};
use llm_auth::session::{
    ExternalFlowDriver, ExternalPending, ExternalState, ExternalStep, LoginFlow,
};

/// Adapt a plugin-contributed auth implementation to the host [`OAuthAuth`].
///
/// The declared auth parameters (`params`, provider-specific endpoints /
/// client id / scopes / flow) are forwarded verbatim to `login_start`; the
/// plugin owns the protocol and the host owns the session lifecycle.
pub struct PluginAuthAdapter {
    auth: Arc<dyn PluginLlmAuth>,
    descriptor: AuthDescriptor,
    params: Value,
}

impl PluginAuthAdapter {
    /// Wrap one plugin auth implementation. `params` carries the provider
    /// definition's declared auth parameters verbatim as a JSON object;
    /// `None` becomes a null JSON value.
    pub fn new(auth: Arc<dyn PluginLlmAuth>, params: Option<Value>) -> Self {
        let descriptor = auth.describe().unwrap_or(AuthDescriptor {
            name: "plugin auth".to_string(),
            is_subscription: false,
            login_label: None,
        });
        let params = params.unwrap_or(Value::Null);
        Self {
            auth,
            descriptor,
            params,
        }
    }

    /// The plugin implementation behind this adapter.
    pub fn auth(&self) -> &Arc<dyn PluginLlmAuth> {
        &self.auth
    }
}

impl fmt::Debug for PluginAuthAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginAuthAdapter")
            .field("name", &self.descriptor.name)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl OAuthAuth for PluginAuthAdapter {
    fn name(&self) -> String {
        self.descriptor.name.clone()
    }

    fn is_subscription(&self) -> bool {
        self.descriptor.is_subscription
    }

    fn login_label(&self) -> Option<String> {
        self.descriptor.login_label.clone()
    }

    async fn begin_login(&self) -> AuthResult<LoginFlow> {
        // The plugin builds its own session from the declared parameters on
        // `login_start`; the seeded state is a placeholder the session only
        // stores and replays.
        Ok(LoginFlow::External(ExternalPending::new(
            Value::Null,
            Arc::new(PluginAuthDriver {
                auth: Arc::clone(&self.auth),
                params: self.params.clone(),
            }),
        )))
    }

    async fn refresh(&self, credential: &OAuthCredential) -> AuthResult<OAuthCredential> {
        let value = credential_to_value(credential)?;
        let refreshed = self
            .auth
            .refresh(value)
            .map_err(|e| AuthError::Refresh(e.to_string()))?;
        serde_json::from_value(refreshed)
            .map_err(|e| AuthError::Refresh(format!("plugin refresh credential: {e}")))
    }

    async fn to_auth(&self, credential: &OAuthCredential) -> AuthResult<ModelAuth> {
        let value = credential_to_value(credential)?;
        let resolved = self
            .auth
            .resolve(value)
            .map_err(|e| AuthError::MalformedCredential(e.to_string()))?;
        Ok(model_auth_from_value(resolved))
    }
}

/// Driver translating the host session's step calls into the plugin's
/// synchronous login contract.
struct PluginAuthDriver {
    auth: Arc<dyn PluginLlmAuth>,
    params: Value,
}

impl fmt::Debug for PluginAuthDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginAuthDriver").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl ExternalFlowDriver for PluginAuthDriver {
    async fn initial(&self, _state: &ExternalState) -> AuthResult<ExternalStep> {
        let challenge = self
            .auth
            .login_start(self.params.clone())
            .map_err(login_error)?;
        step_from_parts(challenge.events, challenge.session, challenge.next)
    }

    async fn poll(&self, state: &ExternalState) -> AuthResult<ExternalStep> {
        let step = self.auth.login_poll(state.clone()).map_err(login_error)?;
        step_from_parts(step.events, step.session, step.next)
    }

    async fn submit(&self, state: &ExternalState, input: String) -> AuthResult<ExternalStep> {
        let step = self
            .auth
            .login_submit(state.clone(), input)
            .map_err(login_error)?;
        step_from_parts(step.events, step.session, step.next)
    }
}

fn login_error(error: wf_plugin_sdk::PluginError) -> AuthError {
    AuthError::Login(error.to_string())
}

/// Map one plugin step (events + session + next) onto the host's
/// [`ExternalStep`].
fn step_from_parts(
    events: Vec<PluginAuthEvent>,
    session: Value,
    next: PluginAuthNext,
) -> AuthResult<ExternalStep> {
    let events = events.into_iter().map(event_from_plugin).collect();
    match next {
        PluginAuthNext::Pending => Ok(ExternalStep::Pending {
            state: session,
            events,
        }),
        PluginAuthNext::AwaitingInput { prompt } => Ok(ExternalStep::AwaitingInput {
            state: session,
            prompt: prompt_from_plugin(prompt),
            events,
        }),
        PluginAuthNext::Completed { credential } => Ok(ExternalStep::Completed {
            credential: credential_from_value(credential)?,
        }),
        PluginAuthNext::Failed { error } => Ok(ExternalStep::Failed { error, events }),
    }
}

fn credential_to_value(credential: &OAuthCredential) -> AuthResult<Value> {
    serde_json::to_value(credential)
        .map_err(|e| AuthError::MalformedCredential(format!("credential serialization: {e}")))
}

/// Parse the plugin's credential JSON (`{access, refresh, expires,
/// is_subscription, extra?}`) into the host credential.
fn credential_from_value(value: Value) -> AuthResult<Credential> {
    let credential: OAuthCredential = serde_json::from_value(value)
        .map_err(|e| AuthError::MalformedCredential(format!("plugin credential: {e}")))?;
    Ok(Credential::OAuth(credential))
}

/// Read the plugin's resolve result (`api_key` / `headers` / `base_url` /
/// `is_subscription` / `source`) into the host model auth. Unknown fields
/// are ignored; missing fields keep their defaults.
fn model_auth_from_value(value: Value) -> ModelAuth {
    let headers: BTreeMap<String, String> = value
        .get("headers")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    ModelAuth {
        api_key: value
            .get("api_key")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        headers,
        base_url: value
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        is_subscription: value
            .get("is_subscription")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        source: value
            .get("source")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

fn event_from_plugin(event: PluginAuthEvent) -> AuthEvent {
    match event {
        PluginAuthEvent::Info { message } => AuthEvent::Info {
            message,
            links: Vec::<AuthInfoLink>::new(),
        },
        PluginAuthEvent::AuthUrl {
            url,
            instructions,
        } => AuthEvent::AuthUrl { url, instructions },
        PluginAuthEvent::DeviceCode {
            user_code,
            verification_uri,
            message,
        } => {
            // The host device-code event has no message field; surface the
            // plugin's extra text as an info event in front of the code.
            if let Some(message) = message {
                tracing::debug!("plugin device-code message: {message}");
            }
            AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                interval_seconds: None,
                expires_in_seconds: None,
            }
        }
        PluginAuthEvent::Progress { message } => AuthEvent::Progress { message },
    }
}

fn prompt_from_plugin(prompt: PluginAuthPrompt) -> AuthPrompt {
    match prompt {
        PluginAuthPrompt::Text {
            message,
            placeholder,
        } => AuthPrompt::Text {
            message,
            placeholder,
        },
        PluginAuthPrompt::Secret {
            message,
            placeholder,
        } => AuthPrompt::Secret {
            message,
            placeholder,
        },
        PluginAuthPrompt::Select { message, options } => AuthPrompt::Select {
            message,
            options: options
                .into_iter()
                .map(|PluginAuthOption { label, value }| AuthPromptOption {
                    id: value,
                    label,
                    description: None,
                })
                .collect(),
        },
        PluginAuthPrompt::ManualCode {
            message,
            placeholder,
        } => AuthPrompt::ManualCode {
            message,
            placeholder,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wf_plugin_sdk::PluginError;

    struct ScriptedAuth {
        descriptor: AuthDescriptor,
    }

    impl PluginLlmAuth for ScriptedAuth {
        fn describe(&self) -> wf_plugin_sdk::PluginResult<AuthDescriptor> {
            Ok(self.descriptor.clone())
        }

        fn login_start(
            &self,
            config: Value,
        ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginChallenge> {
            Ok(wf_plugin_sdk::PluginLoginChallenge {
                events: vec![PluginAuthEvent::AuthUrl {
                    url: "https://example.test/authorize".to_string(),
                    instructions: Some("open the url".to_string()),
                }],
                session: json!({ "config": config, "stage": "authorize" }),
                next: PluginAuthNext::AwaitingInput {
                    prompt: PluginAuthPrompt::ManualCode {
                        message: "paste the callback url".to_string(),
                        placeholder: Some("https://example.test/cb?code=...".to_string()),
                    },
                },
            })
        }

        fn login_poll(
            &self,
            session: Value,
        ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginStep> {
            Ok(wf_plugin_sdk::PluginLoginStep {
                events: vec![PluginAuthEvent::Progress {
                    message: "waiting".to_string(),
                }],
                session,
                next: PluginAuthNext::Pending,
            })
        }

        fn login_submit(
            &self,
            session: Value,
            input: String,
        ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginStep> {
            assert_eq!(input, "https://example.test/cb?code=abc");
            Ok(wf_plugin_sdk::PluginLoginStep {
                events: Vec::new(),
                session,
                next: PluginAuthNext::Completed {
                    credential: json!({
                        "access": "at",
                        "refresh": "rt",
                        "expires": 123,
                        "is_subscription": true,
                    }),
                },
            })
        }

        fn refresh(&self, credential: Value) -> wf_plugin_sdk::PluginResult<Value> {
            assert_eq!(credential["refresh"], "rt");
            Ok(json!({ "access": "at2", "refresh": "rt2", "expires": 456 }))
        }

        fn resolve(&self, credential: Value) -> wf_plugin_sdk::PluginResult<Value> {
            assert_eq!(credential["access"], "at");
            Ok(json!({
                "api_key": "at",
                "base_url": "https://example.test/v1",
                "is_subscription": true,
                "headers": { "x-extra": "1" },
                "unknown_field": "ignored",
            }))
        }
    }

    fn adapter() -> PluginAuthAdapter {
        PluginAuthAdapter::new(
            Arc::new(ScriptedAuth {
                descriptor: AuthDescriptor {
                    name: "Example Sub".to_string(),
                    is_subscription: true,
                    login_label: Some("Sign in with Example Sub".to_string()),
                },
            }),
            Some(json!({ "flow": "pkce_paste" })),
        )
    }

    #[tokio::test]
    async fn describe_surfaces_metadata() {
        let auth = adapter();
        assert_eq!(auth.name(), "Example Sub");
        assert!(auth.is_subscription());
        assert_eq!(auth.login_label().as_deref(), Some("Sign in with Example Sub"));
    }

    #[tokio::test]
    async fn begin_login_seeds_external_flow_with_params() {
        let auth = adapter();
        let flow = auth.begin_login().await.expect("begin_login");
        let LoginFlow::External(pending) = flow else {
            panic!("expected external flow");
        };
        let step = pending
            .driver()
            .initial(pending.state())
            .await
            .expect("initial");
        let ExternalStep::AwaitingInput { state, prompt, events } = step else {
            panic!("expected awaiting input");
        };
        assert_eq!(state["config"]["flow"], "pkce_paste");
        assert_eq!(state["stage"], "authorize");
        assert!(matches!(prompt, AuthPrompt::ManualCode { .. }));
        assert!(matches!(
            events.as_slice(),
            [AuthEvent::AuthUrl { .. }]
        ));
    }

    #[tokio::test]
    async fn submit_completes_with_host_credential() {
        let auth = adapter();
        let flow = auth.begin_login().await.expect("begin_login");
        let LoginFlow::External(pending) = flow else {
            panic!("expected external flow");
        };
        let step = pending
            .driver()
            .submit(&json!({}), "https://example.test/cb?code=abc".to_string())
            .await
            .expect("submit");
        let ExternalStep::Completed { credential } = step else {
            panic!("expected completed");
        };
        let Credential::OAuth(oauth) = credential else {
            panic!("expected oauth credential");
        };
        assert_eq!(oauth.access, "at");
        assert_eq!(oauth.refresh, "rt");
        assert_eq!(oauth.expires, 123);
        assert!(oauth.is_subscription);
    }

    #[tokio::test]
    async fn refresh_and_resolve_pass_through_json() {
        let auth = adapter();
        let credential = OAuthCredential {
            refresh: "rt".to_string(),
            access: "at".to_string(),
            expires: 1,
            is_subscription: true,
            extra: Default::default(),
        };
        let refreshed = auth.refresh(&credential).await.expect("refresh");
        assert_eq!(refreshed.access, "at2");
        assert_eq!(refreshed.expires, 456);
        assert!(!refreshed.is_subscription);

        let resolved = auth.to_auth(&credential).await.expect("to_auth");
        assert_eq!(resolved.api_key.as_deref(), Some("at"));
        assert_eq!(resolved.base_url.as_deref(), Some("https://example.test/v1"));
        assert!(resolved.is_subscription);
        assert_eq!(resolved.headers.get("x-extra").map(String::as_str), Some("1"));
    }

    #[tokio::test]
    async fn plugin_errors_map_onto_auth_errors() {
        struct FailingAuth;

        impl PluginLlmAuth for FailingAuth {
            fn describe(&self) -> wf_plugin_sdk::PluginResult<AuthDescriptor> {
                Err(PluginError::InvalidContribution {
                    plugin_id: "p".to_string(),
                    message: "boom".to_string(),
                })
            }

            fn login_start(
                &self,
                _config: Value,
            ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginChallenge> {
                Err(PluginError::Timeout {
                    plugin_id: "p".to_string(),
                })
            }

            fn login_poll(
                &self,
                _session: Value,
            ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginStep> {
                Err(PluginError::Timeout {
                    plugin_id: "p".to_string(),
                })
            }

            fn login_submit(
                &self,
                _session: Value,
                _input: String,
            ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginStep> {
                Err(PluginError::Timeout {
                    plugin_id: "p".to_string(),
                })
            }

            fn refresh(&self, _credential: Value) -> wf_plugin_sdk::PluginResult<Value> {
                Err(PluginError::Timeout {
                    plugin_id: "p".to_string(),
                })
            }

            fn resolve(&self, _credential: Value) -> wf_plugin_sdk::PluginResult<Value> {
                Err(PluginError::Timeout {
                    plugin_id: "p".to_string(),
                })
            }
        }

        // A failing describe falls back to the generic name instead of
        // failing construction.
        let auth = PluginAuthAdapter::new(Arc::new(FailingAuth), None);
        assert_eq!(auth.name(), "plugin auth");
        let flow = auth.begin_login().await.expect("begin_login");
        let LoginFlow::External(pending) = flow else {
            panic!("expected external flow");
        };
        let err = pending
            .driver()
            .initial(pending.state())
            .await
            .expect_err("must fail");
        let AuthError::Login(message) = err else {
            panic!("expected login error");
        };
        assert!(message.contains("timeout"));
    }

    #[tokio::test]
    async fn malformed_completed_credential_is_rejected() {
        struct BadCredentialAuth;

        impl PluginLlmAuth for BadCredentialAuth {
            fn describe(&self) -> wf_plugin_sdk::PluginResult<AuthDescriptor> {
                Ok(AuthDescriptor {
                    name: "bad".to_string(),
                    is_subscription: false,
                    login_label: None,
                })
            }

            fn login_start(
                &self,
                _config: Value,
            ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginChallenge> {
                Ok(wf_plugin_sdk::PluginLoginChallenge {
                    events: Vec::new(),
                    session: json!({}),
                    next: PluginAuthNext::Completed {
                        credential: json!({ "access": "only-access" }),
                    },
                })
            }

            fn login_poll(
                &self,
                _session: Value,
            ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginStep> {
                unreachable!()
            }

            fn login_submit(
                &self,
                _session: Value,
                _input: String,
            ) -> wf_plugin_sdk::PluginResult<wf_plugin_sdk::PluginLoginStep> {
                unreachable!()
            }

            fn refresh(&self, _credential: Value) -> wf_plugin_sdk::PluginResult<Value> {
                unreachable!()
            }

            fn resolve(&self, _credential: Value) -> wf_plugin_sdk::PluginResult<Value> {
                unreachable!()
            }
        }

        let auth = PluginAuthAdapter::new(Arc::new(BadCredentialAuth), None);
        let flow = auth.begin_login().await.expect("begin_login");
        let LoginFlow::External(pending) = flow else {
            panic!("expected external flow");
        };
        let err = pending
            .driver()
            .initial(pending.state())
            .await
            .expect_err("must fail");
        assert!(matches!(err, AuthError::MalformedCredential(_)));
    }
}
