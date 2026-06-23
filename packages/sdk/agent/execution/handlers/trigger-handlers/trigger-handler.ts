/**
 * Agent Trigger Handler Module
 *
 * Implements Agent-specific trigger execution logic based on sdk/shared/triggers generic framework.
 *
 * Supported Trigger Lifecycle Points:
 * - BEFORE_ITERATION: before iteration starts
 * - AFTER_ITERATION: after iteration completes
 * - ON_ERROR: when an error occurs
 * - ON_TOOL_CALL: when a tool is called
 */

import type { AgentLoopEntity } from "../../../entities/agent-loop-entity.js";
import type { AgentTrigger } from "@wf-agent/types";
import {
  executeTriggers,
  type BaseTriggerDefinition,
  type BaseEventData,
  type TriggerHandler,
  type TriggerExecutionResult,
  type TriggerExecutorConfig,
} from "../../../../shared/triggers/index.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import type { AgentStateCoordinator } from "../../../state-managers/agent-state-coordinator.js";

const logger = createContextualLogger({ component: "AgentTriggerHandler" });

/**
 * Build execution context from agent state
 */
function buildExecutionContext(
  entity: AgentLoopEntity,
  stateCoordinator: AgentStateCoordinator,
): Record<string, unknown> {
  const conversationManager = stateCoordinator.getConversationManager();

  return {
    iteration: entity.state.currentIteration,
    messageCount: conversationManager.getMessageCount(),
    status: entity.state.status,
    timestamp: Date.now(),
    toolCallCount: entity.state.toolCallCount,
  };
}

/**
 * Convert AgentTrigger to BaseTriggerDefinition for executor compatibility
 */
function convertAgentTriggerToBaseTrigger(trigger: AgentTrigger): BaseTriggerDefinition {
  return {
    id: trigger.id,
    name: trigger.name || trigger.id,
    description: trigger.description,
    enabled: trigger.enabled !== false,
    maxTriggers: trigger.maxTriggers,
    triggerCount: 0,
    condition: {
      eventType: trigger.condition.eventType,
      eventName: trigger.condition.eventName,
      condition: trigger.condition.condition,
    },
    action: {
      type: trigger.action.type,
      parameters: trigger.action.parameters || {},
    },
  };
}

/**
 * Execute triggers for a specific lifecycle point
 *
 * @param entity: Agent Loop entity
 * @param triggers: Triggers to execute
 * @param event: Event that triggered execution
 * @param handler: Handler function for matched triggers
 * @param stateCoordinator: Agent State Coordinator for context
 */
export async function executeAgentTriggers(
  entity: AgentLoopEntity,
  triggers: AgentTrigger[],
  event: BaseEventData,
  handler: TriggerHandler,
  stateCoordinator: AgentStateCoordinator,
): Promise<TriggerExecutionResult[]> {
  if (!triggers || triggers.length === 0) {
    return [];
  }

  // Build execution context
  const executionContext = buildExecutionContext(entity, stateCoordinator);

  // Convert AgentTriggers to BaseTriggerDefinitions for executor
  const baseTriggers = triggers.map(convertAgentTriggerToBaseTrigger);

  // Create trigger executor config with state manager and execution context
  const config: TriggerExecutorConfig = {
    errorHandling: "log",
    executionContext,
    stateManager: entity.triggerStateManager,
  };

  // Execute triggers with state management
  try {
    const results = await executeTriggers(baseTriggers, event, handler, config);

    logger.debug("Triggers executed", {
      agentLoopId: entity.id,
      triggersCount: triggers.length,
      executedCount: results.filter(r => r.success).length,
    });

    return results;
  } catch (error) {
    logger.warn("Trigger execution failed", {
      agentLoopId: entity.id,
      error: error instanceof Error ? error.message : String(error),
    });
    return [];
  }
}
