/**
 * Workflow Definition Zod Schemas
 *
 * Provides runtime validation for workflow configuration and definitions
 * following the established architectural pattern where types and schemas
 * co-exist in the types package.
 */

import { z } from "zod";
import type { VariableScope } from "../workflow-execution/scopes.js";
import type { VariableValueType } from "../workflow-execution/variables.js";

// Import the types to ensure schema stays in sync
import type {
  WorkflowVariable,
  ToolApprovalConfig,
  CheckpointConfig,
  TriggeredSubworkflowConfig,
  WorkflowConfig,
  WorkflowMetadata,
} from "./index.js";

/**
 * Variable Scope Schema
 */
export const variableScopeSchema: z.ZodType<VariableScope> = z.enum([
  "global",
  "thread",
  "local",
  "loop",
]);

/**
 * Variable Value Type Schema
 */
export const variableValueTypeSchema: z.ZodType<VariableValueType> = z.enum([
  "number",
  "string",
  "boolean",
  "array",
  "object",
]);

/**
 * Workflow Variable Schema
 */
export const WorkflowVariableSchema: z.ZodType<WorkflowVariable> = z.object({
  name: z.string().min(1, "Variable name is required"),
  type: variableValueTypeSchema,
  defaultValue: z.any().optional(),
  description: z.string().optional(),
  required: z.boolean().optional(),
  readonly: z.boolean().optional(),
  scope: variableScopeSchema.optional(),
});

/**
 * Tool Approval Configuration Schema
 */
export const ToolApprovalConfigSchema: z.ZodType<ToolApprovalConfig> = z.object({
  autoApprovedTools: z.array(z.string()),
});

/**
 * Checkpoint Configuration Schema
 */
export const CheckpointConfigSchema: z.ZodType<CheckpointConfig> = z.object({
  enabled: z.boolean().optional(),
  checkpointBeforeNode: z.boolean().optional(),
  checkpointAfterNode: z.boolean().optional(),
});

/**
 * Triggered Subworkflow Configuration Schema
 */
export const TriggeredSubworkflowConfigSchema: z.ZodType<TriggeredSubworkflowConfig> = z.object({
  enableCheckpoints: z.boolean().optional(),
  checkpointConfig: CheckpointConfigSchema.optional(),
  timeout: z.number().optional(),
  maxRetries: z.number().optional(),
});

/**
 * Retry Policy Schema
 */
export const RetryPolicySchema = z.object({
  maxRetries: z.number().min(0, "Max retries must be non-negative").optional(),
  retryDelay: z.number().min(0, "Retry delay must be non-negative").optional(),
  backoffMultiplier: z.number().positive().optional(),
});

/**
 * Workflow Configuration Schema
 */
export const WorkflowConfigSchema: z.ZodType<WorkflowConfig> = z.object({
  timeout: z.number().min(0, "Timeout must be non-negative").optional(),
  maxSteps: z.number().min(0, "Max steps must be non-negative").optional(),
  enableCheckpoints: z.boolean().optional(),
  checkpointConfig: CheckpointConfigSchema.optional(),
  retryPolicy: RetryPolicySchema.optional(),
  toolApproval: ToolApprovalConfigSchema.optional(),
});

/**
 * Workflow Metadata Schema
 */
export const WorkflowMetadataSchema: z.ZodType<WorkflowMetadata> = z.object({
  author: z.string().optional(),
  tags: z.array(z.string()).optional(),
  category: z.string().optional(),
  customFields: z.record(z.string(), z.any()).optional(),
});

/**
 * Workflow Definition Basic Schema
 * Validates the basic information of a workflow definition
 */
export const WorkflowDefinitionBasicSchema = z.object({
  id: z.string().min(1, "Workflow ID is required"),
  name: z.string().min(1, "Workflow name is required"),
  description: z.string().optional(),
  version: z.string().min(1, "Workflow version is required"),
  createdAt: z.number(),
  updatedAt: z.number(),
});

/**
 * Type Guards
 */

/**
 * Check if a value is a valid WorkflowVariable
 */
export function isWorkflowVariable(value: unknown): value is WorkflowVariable {
  try {
    WorkflowVariableSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Check if a value is a valid ToolApprovalConfig
 */
export function isToolApprovalConfig(value: unknown): value is ToolApprovalConfig {
  try {
    ToolApprovalConfigSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Check if a value is a valid CheckpointConfig
 */
export function isCheckpointConfig(value: unknown): value is CheckpointConfig {
  try {
    CheckpointConfigSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Check if a value is a valid TriggeredSubworkflowConfig
 */
export function isTriggeredSubworkflowConfig(value: unknown): value is TriggeredSubworkflowConfig {
  try {
    TriggeredSubworkflowConfigSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Check if a value is a valid WorkflowConfig
 */
export function isWorkflowConfig(value: unknown): value is WorkflowConfig {
  try {
    WorkflowConfigSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Check if a value is a valid WorkflowMetadata
 */
export function isWorkflowMetadata(value: unknown): value is WorkflowMetadata {
  try {
    WorkflowMetadataSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}
