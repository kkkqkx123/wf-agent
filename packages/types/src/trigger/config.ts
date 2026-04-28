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
  /** Stop Thread */
  | "stop_thread"
  /** Pause thread */
  | "pause_thread"
  /** Resume thread */
  | "resume_thread"
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
 * Stop Thread Action Parameters
 */
export interface StopThreadActionParameters {
  /** Thread ID */
  threadId: ID;
  /** Whether to force stop */
  force?: boolean;
}

/**
 * Pause Thread Action Parameters
 */
export interface PauseThreadActionParameters {
  /** Thread ID */
  threadId: ID;
  /** Reason for suspension */
  reason?: string;
  /** Whether mandatory suspension */
  force?: boolean;
}

/**
 * Recovery Thread Action Parameters
 */
export interface ResumeThreadActionParameters {
  /** Thread ID */
  threadId: ID;
}

/**
 * Skip Node Action Parameters
 */
export interface SkipNodeActionParameters {
  /** Thread ID */
  threadId: ID;
  /** Node ID */
  nodeId: ID;
}

/**
 * Setting variable action parameters
 */
export interface SetVariableActionParameters {
  /** Thread ID */
  threadId: ID;
  /** variable key-value pair */
  variables: Record<string, unknown>;
  /** variable scope */
  scope?: "global" | "thread" | "local" | "loop";
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
  /** Thread ID */
  threadId: ID;
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
 * Stop Thread Action
 */
export interface StopThreadAction extends BaseTriggerAction {
  type: "stop_thread";
  parameters: StopThreadActionParameters;
}

/**
 * Pause Thread Action
 */
export interface PauseThreadAction extends BaseTriggerAction {
  type: "pause_thread";
  parameters: PauseThreadActionParameters;
}

/**
 * Resume thread action
 */
export interface ResumeThreadAction extends BaseTriggerAction {
  type: "resume_thread";
  parameters: ResumeThreadActionParameters;
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
  | StopThreadAction
  | PauseThreadAction
  | ResumeThreadAction
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
