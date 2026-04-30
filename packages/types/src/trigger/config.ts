/**
 * Trigger Configuration Type Definition
 */

import type { ID, Metadata } from "../common.js";
import { EventType } from "../events/index.js";
import type { MessageRole } from "../message/index.js";

/**
 * Trigger Condition Interface
 * Since it is an event judgment, there is no need to use the condition type
 */
export interface TriggerCondition {
  /** Event Type */
  eventType: EventType;
  /** Custom event name (only for NODE_CUSTOM_EVENT events) */
  eventName?: string;
  /** condition metadata */
  metadata?: Metadata;
}

/**
 * Trigger Action Type
 */
export type TriggerActionType =
  /** Stop Workflow Execution */
  | "stop_workflow_execution"
  /** Pause workflow execution */
  | "pause_workflow_execution"
  /** Resume workflow execution */
  | "resume_workflow_execution"
  /** Skip node */
  | "skip_node"
  /** Setting Variables */
  | "set_variable"
  /** Send notification */
  | "send_notification"
  /** Customized Actions */
  | "custom"
  /** Application message manipulation (context compression, etc.) */
  | "apply_message_operation"
  /** Execution triggers subworkflows */
  | "execute_triggered_subgraph"
  /** executable script */
  | "execute_script";

// ============================================================================
// Parameter definitions for each action type
// ============================================================================

/**
 * Stop Workflow Execution Action Parameters
 */
export interface StopWorkflowExecutionActionParameters {
  /** Execution ID */
  executionId: ID;
  /** Whether to force stop */
  force?: boolean;
}

/**
 * Pause Workflow Execution Action Parameters
 */
export interface PauseWorkflowExecutionActionParameters {
  /** Execution ID */
  executionId: ID;
  /** Reason for suspension */
  reason?: string;
  /** Whether mandatory suspension */
  force?: boolean;
}

/**
 * Resume Workflow Execution Action Parameters
 */
export interface ResumeWorkflowExecutionActionParameters {
  /** Execution ID */
  executionId: ID;
}

/**
 * Skip Node Action Parameters
 */
export interface SkipNodeActionParameters {
  /** Execution ID */
  executionId: ID;
  /** Node ID */
  nodeId: ID;
}

/**
 * Setting variable action parameters
 */
export interface SetVariableActionParameters {
  /** Execution ID */
  executionId: ID;
  /** variable key-value pair */
  variables: Record<string, unknown>;
  /** variable scope */
  scope?: "global" | "workflowExecution" | "local" | "loop";
}

/**
 * Send Notification Action Parameters
 */
export interface SendNotificationActionParameters {
  /** notification message */
  message: string;
  /** recipient list */
  recipients?: string[];
  /** Notification level */
  level?: "info" | "warning" | "error" | "success";
  /** Notification channels */
  channel?: "email" | "sms" | "push" | "webhook" | "in_app";
}

/**
 * Customizing Action Parameters
 */
export interface CustomActionParameters {
  /** Custom processor name */
  handlerName: string;
  /** Customized parameters */
  data?: Record<string, unknown>;
}

/**
 * Application Message Action Parameters
 */
export interface ApplyMessageOperationActionParameters {
  /** Execution ID */
  executionId: ID;
  /** Type of operation */
  operationType: "compress" | "truncate" | "summarize" | "mark" | "unmark";
  /** Operational Configuration */
  config?: Record<string, unknown>;
}

/**
 * Execute Script Action Parameters
 */
export interface ExecuteScriptActionParameters {
  /** Script name (must be registered in the ScriptRegistry) */
  scriptName: string;
  /** Parameters passed to the script (accessible within the script via environment variables) */
  parameters?: Record<string, unknown>;
  /** Execution timeout (milliseconds, overriding script default configuration) */
  timeout?: number;
  /** Whether to ignore errors when script execution fails (does not affect the trigger execution result, default false) */
  ignoreError?: boolean;
  /** Whether to verify script existence before execution (default true) */
  validateExistence?: boolean;
}

// ============================================================================
// Trigger action recognizes union type
// ============================================================================

/**
 * Trigger Action Base Interface
 */
interface BaseTriggerAction {
  /** Action Metadata */
  metadata?: Metadata;
}

/**
 * Stop Workflow Execution Action
 */
export interface StopWorkflowExecutionAction extends BaseTriggerAction {
  type: "stop_workflow_execution";
  parameters: StopWorkflowExecutionActionParameters;
}

/**
 * Pause Workflow Execution Action
 */
export interface PauseWorkflowExecutionAction extends BaseTriggerAction {
  type: "pause_workflow_execution";
  parameters: PauseWorkflowExecutionActionParameters;
}

/**
 * Resume Workflow Execution Action
 */
export interface ResumeWorkflowExecutionAction extends BaseTriggerAction {
  type: "resume_workflow_execution";
  parameters: ResumeWorkflowExecutionActionParameters;
}

/**
 * Skip Node Action
 */
export interface SkipNodeAction extends BaseTriggerAction {
  type: "skip_node";
  parameters: SkipNodeActionParameters;
}

/**
 * Setting Variable Actions
 */
export interface SetVariableAction extends BaseTriggerAction {
  type: "set_variable";
  parameters: SetVariableActionParameters;
}

/**
 * Send Notification Action
 */
export interface SendNotificationAction extends BaseTriggerAction {
  type: "send_notification";
  parameters: SendNotificationActionParameters;
}

/**
 * Customized Actions
 */
export interface CustomAction extends BaseTriggerAction {
  type: "custom";
  parameters: CustomActionParameters;
}

/**
 * Apply message manipulation actions
 */
export interface ApplyMessageOperationAction extends BaseTriggerAction {
  type: "apply_message_operation";
  parameters: ApplyMessageOperationActionParameters;
}

/**
 * Execute the trigger sub-workflow action
 */
export interface ExecuteTriggeredSubgraphAction extends BaseTriggerAction {
  type: "execute_triggered_subgraph";
  parameters: ExecuteTriggeredSubgraphActionConfig;
}

/**
 * Executing Script Actions
 */
export interface ExecuteScriptAction extends BaseTriggerAction {
  type: "execute_script";
  parameters: ExecuteScriptActionParameters;
}

/**
 * Trigger Action Union Types
 * Implementing type safety with recognizable unions
 */
export type TriggerAction =
  | StopWorkflowExecutionAction
  | PauseWorkflowExecutionAction
  | ResumeWorkflowExecutionAction
  | SkipNodeAction
  | SetVariableAction
  | SendNotificationAction
  | CustomAction
  | ApplyMessageOperationAction
  | ExecuteTriggeredSubgraphAction
  | ExecuteScriptAction;

/**
 * Type guards: check to see if it's a specific type of trigger action
 */
export function isTriggerActionType<T extends TriggerActionType>(
  action: TriggerAction,
  type: T,
): action is TriggerAction & { type: T } {
  return action.type === type;
}

/**
 * Execution Trigger Sub-Workflow Action Configuration
 * For triggers to initiate isolated subworkflow execution
 */
export interface ExecuteTriggeredSubgraphActionConfig {
  /** Trigger subworkflow ID (workflow containing the START_FROM_TRIGGER node) */
  triggeredWorkflowId: ID;
  /** Whether to wait for completion (default true, synchronized execution) */
  waitForCompletion?: boolean;
  /** Timeout time (milliseconds) */
  timeout?: number;
  /** Whether to record history */
  recordHistory?: boolean;
}

/**
 * Execute Script Action Configuration
 * For triggers to execute registered scripts
 */
export interface ExecuteScriptActionConfig {
  /** Script name (must be registered in the ScriptRegistry) */
  scriptName: string;
  /** Parameters passed to the script (accessible within the script via environment variables) */
  parameters?: Record<string, unknown>;
  /** Execution timeout (milliseconds, overriding script default configuration) */
  timeout?: number;
  /** Whether to ignore errors when script execution fails (does not affect the trigger execution result, default false) */
  ignoreError?: boolean;
  /** Whether to verify script existence before execution (default true) */
  validateExistence?: boolean;
}

/**
 * Conversation History Return Configuration Options
 * Used to control which messages are passed from the master workflow to the trigger child workflow
 */
export interface ConversationHistoryOptions {
  /** Return the last N messages */
  lastN?: number;
  /** Returns the last N messages for the specified role */
  lastNByRole?: {
    role: MessageRole;
    count: number;
  };
  /** Returns all messages for the specified role */
  byRole?: MessageRole;
  /** Returns a specified range of messages (based on the full message list) */
  range?: {
    start: number;
    end: number;
  };
  /** Returns a specified range of messages for a specified role */
  rangeByRole?: {
    role: MessageRole;
    start: number;
    end: number;
  };
}
