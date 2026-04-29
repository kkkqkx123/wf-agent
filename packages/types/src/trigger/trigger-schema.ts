/**
 * Zod Schemas for Trigger Configuration
 * Provides runtime validation schemas that are synchronized with TypeScript type definitions
 */

import { z } from "zod";
import type { EventType } from "../events/index.js";
import type { TriggerActionType } from "./config.js";
// MessageRole import removed - not used in this file

// ============================================================================
// Helper Schemas
// ============================================================================

/**
 * Event Type Schema
 */
const eventTypeSchema = z.custom<EventType>((val): val is EventType =>
  [
    "WORKFLOW_EXECUTION_STARTED",
    "WORKFLOW_EXECUTION_COMPLETED",
    "WORKFLOW_EXECUTION_FAILED",
    "WORKFLOW_EXECUTION_PAUSED",
    "WORKFLOW_EXECUTION_RESUMED",
    "WORKFLOW_EXECUTION_CANCELLED",
    "WORKFLOW_EXECUTION_STATE_CHANGED",
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
    "HUMAN_RELAY_REQUESTED",
    "HUMAN_RELAY_RESPONDED",
    "HUMAN_RELAY_PROCESSED",
    "HUMAN_RELAY_FAILED",
    "LLM_STREAM_ABORTED",
    "LLM_STREAM_ERROR",
  ].includes(val as EventType),
);

/**
 * Trigger Action Type Schema
 */
const triggerActionTypeSchema = z.custom<TriggerActionType>((val): val is TriggerActionType =>
  [
    "stop_thread",
    "pause_thread",
    "resume_thread",
    "skip_node",
    "set_variable",
    "send_notification",
    "custom",
    "apply_message_operation",
    "execute_triggered_subgraph",
    "execute_script",
  ].includes(val as TriggerActionType),
);

// ============================================================================
// Trigger Condition Schema
// ============================================================================

/**
 * Trigger Condition Schema
 */
export const TriggerConditionSchema = z
  .object({
    eventType: eventTypeSchema,
    eventName: z.string().optional(),
    metadata: z.record(z.string(), z.any()).optional(),
  })
  .refine(
    data => {
      // When eventType is NODE_CUSTOM_EVENT, eventName is required.
      if (data.eventType === "NODE_CUSTOM_EVENT" && !data.eventName) {
        return false;
      }
      return true;
    },
    {
      message: "eventName is required when eventType is NODE_CUSTOM_EVENT",
      path: ["eventName"],
    },
  );

// ============================================================================
// Conversation History Options Schema
// ============================================================================

/**
 * Conversation History Options Schema
 */
export const ConversationHistoryOptionsSchema = z
  .object({
    lastN: z.number().int().positive("lastN must be a positive integer").optional(),
    lastNByRole: z
      .object({
        role: z.enum(["system", "user", "assistant", "tool"]),
        count: z.number().int().positive("count must be a positive integer"),
      })
      .optional(),
    byRole: z.enum(["system", "user", "assistant", "tool"]).optional(),
    range: z
      .object({
        start: z.number().int().min(0, "start must be a non-negative integer"),
        end: z.number().int().positive("end must be a positive integer"),
      })
      .refine(data => data.start < data.end, {
        message: "start must be less than end",
        path: ["start"],
      })
      .optional(),
    rangeByRole: z
      .object({
        role: z.enum(["system", "user", "assistant", "tool"]),
        start: z.number().int().min(0, "start must be a non-negative integer"),
        end: z.number().int().positive("end must be a positive integer"),
      })
      .refine(data => data.start < data.end, {
        message: "start must be less than end",
        path: ["start"],
      })
      .optional(),
  })
  .refine(
    data => {
      // At least one option must be specified.
      const hasOption =
        data.lastN !== undefined ||
        data.lastNByRole !== undefined ||
        data.byRole !== undefined ||
        data.range !== undefined ||
        data.rangeByRole !== undefined;
      return hasOption;
    },
    {
      message: "At least one conversation history option must be specified",
      path: [],
    },
  );

// ============================================================================
// Action Parameters Schemas
// ============================================================================

/**
 * Stop Thread Action Parameters Schema
 */
export const StopThreadActionParametersSchema = z.object({
  executionId: z.string().min(1, "Execution ID is required"),
  force: z.boolean().optional(),
});

/**
 * Pause Thread Action Parameters Schema
 */
export const PauseThreadActionParametersSchema = z.object({
  executionId: z.string().min(1, "Execution ID is required"),
  reason: z.string().optional(),
});

/**
 * Resume Thread Action Parameters Schema
 */
export const ResumeThreadActionParametersSchema = z.object({
  executionId: z.string().min(1, "Execution ID is required"),
});

/**
 * Skip Node Action Parameters Schema
 */
export const SkipNodeActionParametersSchema = z.object({
  executionId: z.string().min(1, "Execution ID is required"),
  nodeId: z.string().min(1, "Node ID is required"),
});

/**
 * Set Variable Action Parameters Schema
 */
export const SetVariableActionParametersSchema = z.object({
  executionId: z.string().min(1, "Execution ID is required"),
  variables: z
    .record(z.string(), z.any())
    .refine(vars => Object.keys(vars).length > 0, "At least one variable must be specified"),
  scope: z.enum(["global", "thread", "local", "loop"]).optional(),
});

/**
 * Send Notification Action Parameters Schema
 */
export const SendNotificationActionParametersSchema = z.object({
  message: z.string().min(1, "Message is required"),
  recipients: z.array(z.string()).optional(),
  level: z.enum(["info", "warning", "error", "success"]).optional(),
  channel: z.enum(["email", "sms", "push", "webhook", "in_app"]).optional(),
});

/**
 * Custom Action Parameters Schema
 */
export const CustomActionParametersSchema = z.object({
  handlerName: z.string().min(1, "Handler name is required"),
  data: z.record(z.string(), z.any()).optional(),
});

/**
 * Apply Message Operation Action Parameters Schema
 */
export const ApplyMessageOperationActionParametersSchema = z.object({
  executionId: z.string().min(1, "Execution ID is required"),
  operationType: z.enum(["compress", "truncate", "summarize", "mark", "unmark"]),
  config: z.record(z.string(), z.any()).optional(),
});

/**
 * Execute Triggered Subgraph Action Config Schema
 */
export const ExecuteTriggeredSubgraphActionConfigSchema = z.object({
  triggeredWorkflowId: z.string().min(1, "Triggered workflow ID is required"),
  waitForCompletion: z.boolean().optional(),
  mergeOptions: z
    .object({
      includeVariables: z.array(z.string()).optional(),
      includeConversationHistory: ConversationHistoryOptionsSchema.optional(),
    })
    .optional(),
});

/**
 * Execute Script Action Config Schema
 */
export const ExecuteScriptActionConfigSchema = z.object({
  scriptName: z.string().min(1, "Script name is required"),
  parameters: z.record(z.string(), z.any()).optional(),
  timeout: z.number().int().positive("Timeout must be a positive integer").optional(),
  ignoreError: z.boolean().optional(),
  validateExistence: z.boolean().optional(),
});

// ============================================================================
// Trigger Action Schema
// ============================================================================

/**
 * Trigger Action Schema - Implement type safety using discriminatedUnion
 */
export const TriggerActionSchema = z.discriminatedUnion("type", [
  // stop_workflow_execution
  z.object({
    type: z.literal("stop_workflow_execution"),
    parameters: StopThreadActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // pause_workflow_execution
  z.object({
    type: z.literal("pause_workflow_execution"),
    parameters: PauseThreadActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // resume_workflow_execution
  z.object({
    type: z.literal("resume_workflow_execution"),
    parameters: ResumeThreadActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // skip_node
  z.object({
    type: z.literal("skip_node"),
    parameters: SkipNodeActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // set_variable
  z.object({
    type: z.literal("set_variable"),
    parameters: SetVariableActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // send_notification
  z.object({
    type: z.literal("send_notification"),
    parameters: SendNotificationActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // custom
  z.object({
    type: z.literal("custom"),
    parameters: CustomActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // apply_message_operation
  z.object({
    type: z.literal("apply_message_operation"),
    parameters: ApplyMessageOperationActionParametersSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // execute_triggered_subgraph
  z.object({
    type: z.literal("execute_triggered_subgraph"),
    parameters: ExecuteTriggeredSubgraphActionConfigSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
  // execute_script
  z.object({
    type: z.literal("execute_script"),
    parameters: ExecuteScriptActionConfigSchema,
    metadata: z.record(z.string(), z.any()).optional(),
  }),
]);

// ============================================================================
// WorkflowTrigger Schema
// ============================================================================

/**
 * WorkflowTrigger Schema
 */
export const WorkflowTriggerSchema = z.object({
  id: z.string().min(1, "Trigger ID is required"),
  name: z.string().min(1, "Trigger name is required"),
  description: z.string().optional(),
  condition: TriggerConditionSchema,
  action: TriggerActionSchema,
  enabled: z.boolean().optional(),
  maxTriggers: z.number().int().min(0, "Max triggers must be a non-negative integer").optional(),
  metadata: z.record(z.string(), z.any()).optional(),
  createCheckpoint: z.boolean().optional(),
  checkpointDescription: z.string().optional(),
});

// ============================================================================
// TriggerConfigOverride Schema
// ============================================================================

/**
 * TriggerConfigOverride Schema
 */
export const TriggerConfigOverrideSchema = z.object({
  condition: z
    .object({
      eventType: eventTypeSchema.optional(),
      eventName: z.string().optional(),
      metadata: z.record(z.string(), z.any()).optional(),
    })
    .optional(),
  action: z
    .object({
      type: triggerActionTypeSchema.optional(),
      parameters: z.record(z.string(), z.any()).optional(),
      metadata: z.record(z.string(), z.any()).optional(),
    })
    .optional(),
  enabled: z.boolean().optional(),
  maxTriggers: z.number().int().min(0, "Max triggers must be a non-negative integer").optional(),
});

// ============================================================================
// TriggerReference Schema
// ============================================================================

/**
 * TriggerReference Schema
 */
export const TriggerReferenceSchema = z.object({
  templateName: z.string().min(1, "Template name is required"),
  triggerId: z.string().min(1, "Trigger ID is required"),
  triggerName: z.string().optional(),
  configOverride: TriggerConfigOverrideSchema.optional(),
});

// ============================================================================
// TriggerTemplate Schema
// ============================================================================

/**
 * TriggerTemplate Schema
 */
export const TriggerTemplateSchema = z.object({
  name: z.string().min(1, "Trigger template name is required"),
  description: z.string().optional(),
  condition: TriggerConditionSchema,
  action: TriggerActionSchema,
  enabled: z.boolean().optional(),
  maxTriggers: z.number().int().min(0, "Max triggers must be a non-negative integer").optional(),
  metadata: z.record(z.string(), z.any()).optional(),
  createdAt: z.number().int().min(0, "CreatedAt must be a non-negative integer"),
  updatedAt: z.number().int().min(0, "UpdatedAt must be a non-negative integer"),
  createCheckpoint: z.boolean().optional(),
  checkpointDescriptionTemplate: z.string().optional(),
});

// ============================================================================
// Type Guards
// ============================================================================

/**
 * Type guard for TriggerCondition
 */
export const isTriggerCondition = (
  config: unknown,
): config is z.infer<typeof TriggerConditionSchema> => {
  return TriggerConditionSchema.safeParse(config).success;
};

/**
 * Type guard for TriggerAction
 */
export const isTriggerAction = (config: unknown): config is z.infer<typeof TriggerActionSchema> => {
  return TriggerActionSchema.safeParse(config).success;
};

/**
 * Type guard for WorkflowTrigger
 */
export const isWorkflowTrigger = (
  config: unknown,
): config is z.infer<typeof WorkflowTriggerSchema> => {
  return WorkflowTriggerSchema.safeParse(config).success;
};

/**
 * Type guard for TriggerReference
 */
export const isTriggerReference = (
  config: unknown,
): config is z.infer<typeof TriggerReferenceSchema> => {
  return TriggerReferenceSchema.safeParse(config).success;
};

/**
 * Type guard for TriggerTemplate
 */
export const isTriggerTemplate = (
  config: unknown,
): config is z.infer<typeof TriggerTemplateSchema> => {
  return TriggerTemplateSchema.safeParse(config).success;
};

/**
 * Type guard for TriggerConfigOverride
 */
export const isTriggerConfigOverride = (
  config: unknown,
): config is z.infer<typeof TriggerConfigOverrideSchema> => {
  return TriggerConfigOverrideSchema.safeParse(config).success;
};
