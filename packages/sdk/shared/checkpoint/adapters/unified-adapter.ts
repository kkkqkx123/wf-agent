/**
 * Checkpoint Trigger Adapters for Legacy Compatibility
 *
 * Provides mapping functions to convert between legacy checkpoint trigger types
 * (Workflow and Agent-specific) and the unified checkpoint trigger types.
 *
 * This enables gradual migration while maintaining backward compatibility.
 */

import { CheckpointTrigger } from '@wf-agent/types';
import type { CheckpointTrigger as CheckpointTriggerType } from '@wf-agent/types';
import type {
  WorkflowCheckpointTriggerType,
  AgentLoopCheckpointTriggerType,
} from '@wf-agent/types';

/**
 * Convert Workflow checkpoint trigger to unified type
 *
 * Mapping:
 * - NODE_BEFORE_EXECUTE → BEFORE_EXECUTE
 * - NODE_AFTER_EXECUTE → AFTER_EXECUTE
 * - TOOL_BEFORE → TOOL_BEFORE
 * - TOOL_AFTER → TOOL_AFTER
 * - HOOK → MANUAL
 * - TRIGGER → MANUAL
 *
 * @param workflowTrigger - Legacy workflow checkpoint trigger
 * @returns Unified checkpoint trigger
 *
 * @throws Error if trigger type is unrecognized
 */
export function workflowTriggerToUnified(
  workflowTrigger: WorkflowCheckpointTriggerType
): CheckpointTriggerType {
  const mapping: Record<WorkflowCheckpointTriggerType, CheckpointTrigger> = {
    NODE_BEFORE_EXECUTE: CheckpointTrigger.BEFORE_EXECUTE,
    NODE_AFTER_EXECUTE: CheckpointTrigger.AFTER_EXECUTE,
    TOOL_BEFORE: CheckpointTrigger.TOOL_BEFORE,
    TOOL_AFTER: CheckpointTrigger.TOOL_AFTER,
    HOOK: CheckpointTrigger.MANUAL,
    TRIGGER: CheckpointTrigger.MANUAL,
  };

  const unified = mapping[workflowTrigger];
  if (!unified) {
    throw new Error(
      `Unrecognized workflow checkpoint trigger: ${workflowTrigger}`
    );
  }

  return unified;
}

/**
 * Convert Agent loop checkpoint trigger to unified type
 *
 * Mapping:
 * - ON_ITERATION → ITERATION_END
 * - ON_COMPLETE → ON_COMPLETE
 * - ON_ERROR → ON_ERROR
 * - ON_PAUSE → ON_PAUSE
 * - ON_TOOL_CALL → TOOL_BEFORE
 * - ON_TOOL_RESULT → TOOL_AFTER
 * - ON_INTERVAL → INTERVAL
 * - MANUAL → MANUAL
 * - NEVER → NEVER
 *
 * @param agentTrigger - Legacy agent loop checkpoint trigger
 * @returns Unified checkpoint trigger
 *
 * @throws Error if trigger type is unrecognized
 */
export function agentTriggerToUnified(
  agentTrigger: AgentLoopCheckpointTriggerType
): CheckpointTriggerType {
  const mapping: Record<AgentLoopCheckpointTriggerType, CheckpointTrigger> = {
    ITERATION_END: CheckpointTrigger.ITERATION_END,
    ERROR: CheckpointTrigger.ON_ERROR,
    COMPLETE: CheckpointTrigger.ON_COMPLETE,
    PAUSE: CheckpointTrigger.ON_PAUSE,
    TOOL_CALL: CheckpointTrigger.TOOL_BEFORE,
    TOOL_RESULT: CheckpointTrigger.TOOL_AFTER,
    MANUAL: CheckpointTrigger.MANUAL,
    INTERVAL: CheckpointTrigger.INTERVAL,
    NEVER: CheckpointTrigger.NEVER,
  };

  const unified = mapping[agentTrigger];
  if (!unified) {
    throw new Error(
      `Unrecognized agent loop checkpoint trigger: ${agentTrigger}`
    );
  }

  return unified;
}

/**
 * Convert unified checkpoint trigger to Workflow trigger
 *
 * Reverse mapping (note: some information may be lost due to fewer workflow triggers)
 *
 * @param unifiedTrigger - Unified checkpoint trigger
 * @returns Workflow checkpoint trigger
 *
 * @throws Error if no mapping exists
 */
export function unifiedTriggerToWorkflow(
  unifiedTrigger: CheckpointTriggerType
): WorkflowCheckpointTriggerType {
  const mapping: Record<CheckpointTrigger, WorkflowCheckpointTriggerType> = {
    [CheckpointTrigger.BEFORE_EXECUTE]: 'NODE_BEFORE_EXECUTE',
    [CheckpointTrigger.AFTER_EXECUTE]: 'NODE_AFTER_EXECUTE',
    [CheckpointTrigger.ON_ERROR]: 'NODE_BEFORE_EXECUTE',
    [CheckpointTrigger.BEFORE_RETRY]: 'NODE_BEFORE_EXECUTE',
    [CheckpointTrigger.AFTER_RETRY_SUCCESS]: 'NODE_AFTER_EXECUTE',
    [CheckpointTrigger.ON_FALLBACK]: 'NODE_AFTER_EXECUTE',
    [CheckpointTrigger.ITERATION_END]: 'NODE_AFTER_EXECUTE',
    [CheckpointTrigger.ITERATION_FAILED]: 'NODE_BEFORE_EXECUTE',
    [CheckpointTrigger.TOOL_BEFORE]: 'TOOL_BEFORE',
    [CheckpointTrigger.TOOL_AFTER]: 'TOOL_AFTER',
    [CheckpointTrigger.ON_PAUSE]: 'NODE_BEFORE_EXECUTE',
    [CheckpointTrigger.ON_CANCEL]: 'NODE_BEFORE_EXECUTE',
    [CheckpointTrigger.ON_COMPLETE]: 'NODE_AFTER_EXECUTE',
    [CheckpointTrigger.INTERVAL]: 'NODE_AFTER_EXECUTE',
    [CheckpointTrigger.MANUAL]: 'HOOK',
    [CheckpointTrigger.NEVER]: 'NODE_BEFORE_EXECUTE',
  };

  const workflow = mapping[unifiedTrigger as CheckpointTrigger];
  if (!workflow) {
    throw new Error(
      `Cannot map unified trigger to workflow: ${unifiedTrigger}`
    );
  }

  return workflow;
}

/**
 * Convert unified checkpoint trigger to Agent trigger
 *
 * @param unifiedTrigger - Unified checkpoint trigger
 * @returns Agent loop checkpoint trigger
 *
 * @throws Error if no mapping exists
 */
export function unifiedTriggerToAgent(
  unifiedTrigger: CheckpointTriggerType
): AgentLoopCheckpointTriggerType {
  const mapping: Record<CheckpointTrigger, AgentLoopCheckpointTriggerType> = {
    [CheckpointTrigger.BEFORE_EXECUTE]: 'ITERATION_END',
    [CheckpointTrigger.AFTER_EXECUTE]: 'ITERATION_END',
    [CheckpointTrigger.ON_ERROR]: 'ERROR',
    [CheckpointTrigger.BEFORE_RETRY]: 'ERROR',
    [CheckpointTrigger.AFTER_RETRY_SUCCESS]: 'ITERATION_END',
    [CheckpointTrigger.ON_FALLBACK]: 'COMPLETE',
    [CheckpointTrigger.ITERATION_END]: 'ITERATION_END',
    [CheckpointTrigger.ITERATION_FAILED]: 'ERROR',
    [CheckpointTrigger.TOOL_BEFORE]: 'TOOL_CALL',
    [CheckpointTrigger.TOOL_AFTER]: 'TOOL_RESULT',
    [CheckpointTrigger.ON_PAUSE]: 'PAUSE',
    [CheckpointTrigger.ON_CANCEL]: 'PAUSE',
    [CheckpointTrigger.ON_COMPLETE]: 'COMPLETE',
    [CheckpointTrigger.INTERVAL]: 'INTERVAL',
    [CheckpointTrigger.MANUAL]: 'MANUAL',
    [CheckpointTrigger.NEVER]: 'NEVER',
  };

  const agent = mapping[unifiedTrigger as CheckpointTrigger];
  if (!agent) {
    throw new Error(
      `Cannot map unified trigger to agent: ${unifiedTrigger}`
    );
  }

  return agent;
}

/**
 * Check if a unified trigger is primarily for Workflow execution
 *
 * Useful for filtering triggers in context-specific checkpoint creation.
 *
 * @param trigger - Unified checkpoint trigger
 * @returns true if this trigger is typically used in Workflow context
 */
export function isWorkflowTrigger(trigger: CheckpointTriggerType): boolean {
  const workflowTriggers: CheckpointTrigger[] = [
    CheckpointTrigger.BEFORE_EXECUTE,
    CheckpointTrigger.AFTER_EXECUTE,
    CheckpointTrigger.TOOL_BEFORE,
    CheckpointTrigger.TOOL_AFTER,
    CheckpointTrigger.BEFORE_RETRY,
    CheckpointTrigger.AFTER_RETRY_SUCCESS,
    CheckpointTrigger.ON_FALLBACK,
    CheckpointTrigger.ON_ERROR,
  ];
  return workflowTriggers.includes(trigger as CheckpointTrigger);
}

/**
 * Check if a unified trigger is primarily for Agent execution
 *
 * Useful for filtering triggers in context-specific checkpoint creation.
 *
 * @param trigger - Unified checkpoint trigger
 * @returns true if this trigger is typically used in Agent context
 */
export function isAgentTrigger(trigger: CheckpointTriggerType): boolean {
  const agentTriggers: CheckpointTrigger[] = [
    CheckpointTrigger.ITERATION_END,
    CheckpointTrigger.ITERATION_FAILED,
    CheckpointTrigger.ON_ERROR,
    CheckpointTrigger.BEFORE_RETRY,
    CheckpointTrigger.AFTER_RETRY_SUCCESS,
    CheckpointTrigger.ON_FALLBACK,
    CheckpointTrigger.TOOL_BEFORE,
    CheckpointTrigger.TOOL_AFTER,
    CheckpointTrigger.INTERVAL,
  ];
  return agentTriggers.includes(trigger as CheckpointTrigger);
}

/**
 * Get all unified triggers that map to a specific workflow trigger
 *
 * Useful for reverse mapping when you need to know all equivalent triggers.
 *
 * @param workflowTrigger - Workflow checkpoint trigger
 * @returns Array of equivalent unified triggers
 */
export function getUnifiedTriggersForWorkflow(
  workflowTrigger: WorkflowCheckpointTriggerType
): CheckpointTriggerType[] {
  const triggers: CheckpointTrigger[] = [];

  // Map back from workflow trigger
  switch (workflowTrigger) {
    case 'NODE_BEFORE_EXECUTE':
      triggers.push(CheckpointTrigger.BEFORE_EXECUTE, CheckpointTrigger.ON_ERROR);
      break;
    case 'NODE_AFTER_EXECUTE':
      triggers.push(CheckpointTrigger.AFTER_EXECUTE, CheckpointTrigger.ON_COMPLETE);
      break;
    case 'TOOL_BEFORE':
      triggers.push(CheckpointTrigger.TOOL_BEFORE);
      break;
    case 'TOOL_AFTER':
      triggers.push(CheckpointTrigger.TOOL_AFTER);
      break;
    case 'HOOK':
      triggers.push(CheckpointTrigger.MANUAL);
      break;
    case 'TRIGGER':
      triggers.push(CheckpointTrigger.MANUAL);
      break;
  }

  return triggers;
}

/**
 * Get all unified triggers that map to a specific agent trigger
 *
 * @param agentTrigger - Agent loop checkpoint trigger
 * @returns Array of equivalent unified triggers
 */
export function getUnifiedTriggersForAgent(
  agentTrigger: AgentLoopCheckpointTriggerType
): CheckpointTriggerType[] {
  const triggers: CheckpointTrigger[] = [];

  switch (agentTrigger) {
    case 'ITERATION_END':
      triggers.push(CheckpointTrigger.ITERATION_END, CheckpointTrigger.AFTER_EXECUTE);
      break;
    case 'ERROR':
      triggers.push(CheckpointTrigger.ON_ERROR, CheckpointTrigger.ITERATION_FAILED);
      break;
    case 'COMPLETE':
      triggers.push(CheckpointTrigger.ON_COMPLETE);
      break;
    case 'PAUSE':
      triggers.push(CheckpointTrigger.ON_PAUSE);
      break;
    case 'TOOL_CALL':
      triggers.push(CheckpointTrigger.TOOL_BEFORE);
      break;
    case 'TOOL_RESULT':
      triggers.push(CheckpointTrigger.TOOL_AFTER);
      break;
    case 'MANUAL':
      triggers.push(CheckpointTrigger.MANUAL);
      break;
    case 'INTERVAL':
      triggers.push(CheckpointTrigger.INTERVAL);
      break;
    case 'NEVER':
      triggers.push(CheckpointTrigger.NEVER);
      break;
  }

  return triggers;
}
