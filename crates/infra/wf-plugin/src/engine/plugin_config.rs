use serde_json::Value;

use super::PluginEngine;
use crate::error::{PluginError, PluginResult};
use crate::events::PluginEvent;

impl PluginEngine {
    /// Current plugin-specific configuration, or `None` when the plugin has
    /// no configuration.
    pub fn get_plugin_config(&self, plugin_id: &str) -> Option<Value> {
        self.options.config.get(plugin_id).cloned()
    }

    pub async fn update_plugin_config(
        &mut self,
        plugin_id: &str,
        config: Value,
    ) -> PluginResult<()> {
        let instance = self
            .registry
            .instance(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;

        let schema = self
            .registry
            .get(plugin_id)
            .and_then(|info| info.manifest.config_schema);
        wf_plugin_sdk::validate_config_for(plugin_id, &config, schema.as_ref())?;

        self.options
            .config
            .insert(plugin_id.to_owned(), config.clone());

        match self
            .guard
            .execute(plugin_id, instance.on_config_change(&config))
            .await
        {
            Ok(_) => {}
            Err(e) => {
                return Err(PluginError::ConfigChangeFailed {
                    plugin_id: plugin_id.to_owned(),
                    message: e.to_string(),
                });
            }
        }

        self.publish(PluginEvent::ConfigChanged {
            plugin_id: plugin_id.to_owned(),
            config,
        });

        if let Err(e) = self.refresh_plugin_contributions(plugin_id).await {
            let message = format!("contribution refresh failed: {e}");
            self.registry.set_error(plugin_id, message.clone());
            self.publish(PluginEvent::Error {
                plugin_id: plugin_id.to_owned(),
                error: message.clone(),
            });
            return Err(PluginError::ConfigChangeFailed {
                plugin_id: plugin_id.to_owned(),
                message,
            });
        }

        Ok(())
    }

    pub async fn refresh_plugin_contributions(&self, plugin_id: &str) -> PluginResult<bool> {
        let instance = self
            .registry
            .instance(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_owned()))?;
        if !self
            .guard
            .execute(plugin_id, instance.reload_declaration())
            .await?
        {
            return Ok(false);
        }
        if let Some(ref bridge) = self.bridge {
            if let Err(e) = bridge
                .unsync_all(plugin_id, &self.contribution_manager)
                .await
            {
                tracing::warn!(
                    plugin_id,
                    "plugin bridge unsync failed during refresh: {}",
                    e
                );
            }
        }
        self.contribution_manager.unregister_all(plugin_id);
        self.contribution_manager.start_registration(plugin_id);
        self.sync_manifest_llm_providers(plugin_id);
        let mut registrar = self.contribution_manager.as_registrar();
        self.guard
            .execute(plugin_id, async {
                instance.register_contributions(&mut registrar)
            })
            .await?;
        self.check_manifest_contributions(plugin_id);
        let records: Vec<crate::registry::ContributionRecord> = self
            .contribution_manager
            .contributions_for(plugin_id)
            .into_iter()
            .map(
                |(contribution_type, key)| crate::registry::ContributionRecord {
                    contribution_type,
                    key,
                    plugin_id: plugin_id.to_owned(),
                },
            )
            .collect();
        self.registry.replace_contributions(plugin_id, records);
        if let Some(ref bridge) = self.bridge {
            bridge
                .sync_all(plugin_id, &self.contribution_manager)
                .await?;
        }
        tracing::info!(plugin_id, "plugin contributions refreshed");
        Ok(true)
    }
}
