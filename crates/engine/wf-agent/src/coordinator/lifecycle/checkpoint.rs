use crate::checkpoint::{AgentCheckpointIntegration, AgentCheckpointStrategy};
use crate::coordinator::lifecycle::AgentLoopCoordinator;
use wf_tools::callback::AgentLoopConfig;

impl AgentLoopCoordinator {
    pub(super) fn build_checkpoint_integration(&self) -> Option<AgentCheckpointIntegration> {
        let strategy = self.checkpoint_strategy.as_ref()?;
        let _ = strategy;
        Some(self.build_checkpoint_integration_any())
    }

    /// Reject a run config carrying both an explicit coordinator strategy
    /// and a per-run message backstop: the two would silently resolve to
    /// the explicit strategy and drop the caller interval. Callers pass
    /// one source only; run entry points invoke this before building any
    /// integration so the conflict surfaces as validation, never as a
    /// silently ignored parameter.
    pub(super) fn validate_checkpoint_config(
        &self,
        config: &AgentLoopConfig,
    ) -> crate::error::AgentResult<()> {
        let has_explicit = self.checkpoint_strategy.is_some();
        let has_interval = config
            .checkpoint_message_interval
            .filter(|n| *n > 0)
            .is_some();
        if has_explicit && has_interval {
            return Err(crate::error::AgentError::Validation(
                "explicit checkpoint strategy and checkpoint_message_interval conflict: pass one source only"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Checkpoint integration for a concrete run config. An explicitly
    /// configured strategy wins; otherwise a `checkpoint_message_interval`
    /// derives a `from_agent_config` strategy (tool/compression boundaries
    /// on, message backstop at the requested interval) so the REST
    /// `checkpoint_message_interval` actually produces `Interval`
    /// checkpoints. With neither configured there is no integration
    /// (previous default: no checkpoints), preserving prior behavior.
    /// Conflicting inputs are rejected by `validate_checkpoint_config`
    /// before this runs, so the explicit-wins branch below never hides a
    /// caller interval.
    pub(super) fn checkpoint_integration_for_config(
        &self,
        config: &AgentLoopConfig,
    ) -> Option<AgentCheckpointIntegration> {
        // Ephemeral read-only turns take no checkpoints at all: no
        // interval checkpoints, no lifecycle checkpoints, no hook
        // checkpoints. The transcript still persists through the normal
        // record/message path; only the checkpoint chain stays empty.
        if config.temporary_mode == Some(wf_types::agent_execution::TemporaryMode::Ephemeral) {
            return None;
        }
        if self.checkpoint_strategy.is_some() {
            return self.build_checkpoint_integration();
        }
        let interval = config.checkpoint_message_interval.filter(|n| *n > 0)?;
        let strategy = AgentCheckpointStrategy::from_agent_config(
            self.default_max_iterations,
            true,
            true,
            true,
            Some(interval),
        );
        let mut cp = self.build_checkpoint_integration_any();
        cp = cp.with_strategy(strategy);
        Some(cp)
    }

    /// Assemble the checkpoint integration from shared components. Used
    /// unconditionally (i.e. also when no checkpoint strategy is configured)
    /// so checkpoint restore is always available: `resume_from_checkpoint`
    /// drives a fresh loop over a stored snapshot regardless of whether the
    /// original run persisted intermediate checkpoints.
    pub(super) fn build_checkpoint_integration_any(&self) -> AgentCheckpointIntegration {
        let mut cp = AgentCheckpointIntegration::new(self.store.clone());
        if let Some(ref manager) = self.file_checkpoint_manager {
            cp = cp.with_file_checkpoint_manager(manager.clone());
        }
        if let Some(ref bus) = self.checkpoint_event_bus {
            cp = cp.with_event_bus(bus.clone());
        }
        if let Some(ref bus) = self.checkpoint_execution_events {
            cp = cp.with_execution_event_bus(bus.clone());
        }
        if let Some(ref strategy) = self.checkpoint_strategy {
            cp = cp.with_strategy(strategy.clone());
        }
        cp
    }
}
