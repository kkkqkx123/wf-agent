use wf_core::event::EventBus;
use wf_types::events::{BaseEvent, EventType};
use wf_types::Id;

use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

pub struct AgentLoopStateTransitor;

impl AgentLoopStateTransitor {
    pub async fn start_agent_loop(entity: &AgentLoopEntity) -> AgentResult<()> {
        entity.state.write().await.start();
        Ok(())
    }

    pub async fn pause_agent_loop(entity: &AgentLoopEntity) -> AgentResult<()> {
        entity.state.write().await.pause();
        Ok(())
    }

    pub async fn resume_agent_loop(entity: &AgentLoopEntity) -> AgentResult<()> {
        entity.state.write().await.resume();
        Ok(())
    }

    pub async fn complete_agent_loop(entity: &AgentLoopEntity) -> AgentResult<()> {
        entity.state.write().await.complete();
        Ok(())
    }

    pub async fn fail_agent_loop(entity: &AgentLoopEntity, error: String) -> AgentResult<()> {
        entity.state.write().await.fail(error);
        Ok(())
    }

    pub async fn cancel_agent_loop(entity: &AgentLoopEntity) -> AgentResult<()> {
        entity.state.write().await.cancel();
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
