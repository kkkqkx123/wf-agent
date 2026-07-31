use wf_core::event::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::Id;

use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentLoopStateTransitor;

impl AgentLoopStateTransitor {
    pub async fn start_agent_loop(
        entity: &AgentLoopEntity,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<()> {
        entity.state.write().await.start();
        if let Some(eb) = event_bus {
            Self::emit_event(eb, EventType::AgentStarted, entity.id().clone());
        }
        Ok(())
    }

    pub async fn pause_agent_loop(
        entity: &AgentLoopEntity,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<()> {
        entity.state.write().await.pause();
        if let Some(eb) = event_bus {
            Self::emit_event(eb, EventType::AgentPaused, entity.id().clone());
        }
        Ok(())
    }

    pub async fn resume_agent_loop(
        entity: &AgentLoopEntity,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<()> {
        entity.state.write().await.resume();
        if let Some(eb) = event_bus {
            Self::emit_event(eb, EventType::AgentResumed, entity.id().clone());
        }
        Ok(())
    }

    pub async fn complete_agent_loop(
        entity: &AgentLoopEntity,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<()> {
        entity.state.write().await.complete();
        if let Some(eb) = event_bus {
            Self::emit_event(eb, EventType::AgentCompleted, entity.id().clone());
        }
        Ok(())
    }

    pub async fn fail_agent_loop(
        entity: &AgentLoopEntity,
        error: String,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<()> {
        entity.state.write().await.fail(error);
        if let Some(eb) = event_bus {
            Self::emit_event(eb, EventType::AgentFailed, entity.id().clone());
        }
        Ok(())
    }

    pub async fn cancel_agent_loop(
        entity: &AgentLoopEntity,
        event_bus: Option<&EventBus>,
    ) -> AgentResult<()> {
        entity.state.write().await.cancel();
        if let Some(eb) = event_bus {
            Self::emit_event(eb, EventType::AgentCancelled, entity.id().clone());
        }
        Ok(())
    }

    pub fn emit_event(event_bus: &EventBus, event_type: EventType, execution_id: Id) {
        let event = BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: wf_common::now(),
            workflow_id: None,
            execution_id: Some(execution_id),
            agent_loop_id: None,
            metadata: None,
        };
        let _ = event_bus.publish(event);
    }
}
