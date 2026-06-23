/**
 * Trigger Template Validators
 *
 * Validation logic for trigger template definitions stored in the registry.
 * These constants and validators ensure that trigger templates conform to the expected structure
 * before they are registered or updated.
 */

import type { TriggerTemplate } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

export const TRIGGER_TEMPLATE_EVENT_TYPES = [
  "WORKFLOW_EXECUTION_STARTED",
  "WORKFLOW_EXECUTION_COMPLETED",
  "WORKFLOW_EXECUTION_FAILED",
  "WORKFLOW_EXECUTION_PAUSED",
  "WORKFLOW_EXECUTION_RESUMED",
  "WORKFLOW_EXECUTION_CANCELLED",
  "WORKFLOW_EXECUTION_STATE_CHANGED",
  "WORKFLOW_EXECUTION_FORK_STARTED",
  "WORKFLOW_EXECUTION_FORK_COMPLETED",
  "WORKFLOW_EXECUTION_JOIN_STARTED",
  "WORKFLOW_EXECUTION_JOIN_CONDITION_MET",
  "WORKFLOW_EXECUTION_COPY_STARTED",
  "WORKFLOW_EXECUTION_COPY_COMPLETED",
  "NODE_STARTED",
  "NODE_COMPLETED",
  "NODE_FAILED",
  "NODE_CUSTOM_EVENT",
  "TOKEN_LIMIT_EXCEEDED",
  "TOKEN_USAGE_WARNING",
  "CONTEXT_COMPRESSION_REQUESTED",
  "CONTEXT_COMPRESSION_COMPLETED",
  "MESSAGE_ADDED",
  "TOOL_CALL_STARTED",
  "TOOL_CALL_COMPLETED",
  "TOOL_CALL_FAILED",
  "TOOL_ADDED",
  "CONVERSATION_STATE_CHANGED",
  "ERROR",
  "CHECKPOINT_CREATED",
  "CHECKPOINT_RESTORED",
  "CHECKPOINT_DELETED",
  "CHECKPOINT_FAILED",
  "SUBGRAPH_STARTED",
  "SUBGRAPH_COMPLETED",
  "TRIGGERED_SUBGRAPH_STARTED",
  "TRIGGERED_SUBGRAPH_COMPLETED",
  "TRIGGERED_SUBGRAPH_FAILED",
  "VARIABLE_CHANGED",
  "USER_INTERACTION_REQUESTED",
  "USER_INTERACTION_RESPONDED",
  "USER_INTERACTION_PROCESSED",
  "USER_INTERACTION_FAILED",
  "TOOL_APPROVAL_REQUESTED",
  "TOOL_APPROVAL_RESPONDED",
  "TOOL_APPROVAL_FAILED",
  "FOLLOWUP_QUESTION_REQUESTED",
  "FOLLOWUP_QUESTION_RESPONDED",
  "FOLLOWUP_QUESTION_FAILED",
  "LLM_STREAM_ABORTED",
  "LLM_STREAM_ERROR",
  "DYNAMIC_WORKFLOW_EXECUTION_SUBMITTED",
  "DYNAMIC_WORKFLOW_EXECUTION_COMPLETED",
  "DYNAMIC_WORKFLOW_EXECUTION_FAILED",
  "DYNAMIC_WORKFLOW_EXECUTION_CANCELLED",
] as const;

export const TRIGGER_TEMPLATE_ACTION_TYPES = [
  "stop_workflow_execution",
  "pause_workflow_execution",
  "resume_workflow_execution",
  "skip_node",
  "set_variable",
  "send_notification",
  "execute_triggered_subworkflow",
  "execute_script",
  "apply_message_operation",
] as const;

export function validateTriggerTemplate(template: TriggerTemplate): void {
  if (!template.name || typeof template.name !== "string") {
    throw new ConfigurationValidationError(
      "Trigger template name is required and must be a string",
      {
        configType: "trigger",
        configPath: "template.name",
      },
    );
  }

  if (!template.condition) {
    throw new ConfigurationValidationError("Trigger template condition is required", {
      configType: "trigger",
      configPath: "template.condition",
    });
  }

  if (!template.action) {
    throw new ConfigurationValidationError("Trigger template action is required", {
      configType: "trigger",
      configPath: "template.action",
    });
  }

  if (!template.condition.eventType) {
    throw new ConfigurationValidationError("Trigger template condition eventType is required", {
      configType: "trigger",
      configPath: "template.condition.eventType",
    });
  }

  if (!TRIGGER_TEMPLATE_EVENT_TYPES.includes(template.condition.eventType as any)) {
    throw new ConfigurationValidationError(
      `Invalid event type: ${template.condition.eventType}`,
      {
        configType: "trigger",
        configPath: "template.condition.eventType",
      },
    );
  }

  if (!template.action.type) {
    throw new ConfigurationValidationError("Trigger template action type is required", {
      configType: "trigger",
      configPath: "template.action.type",
    });
  }

  if (!TRIGGER_TEMPLATE_ACTION_TYPES.includes(template.action.type as any)) {
    throw new ConfigurationValidationError(`Invalid action type: ${template.action.type}`, {
      configType: "trigger",
      configPath: "template.action.type",
    });
  }
}
