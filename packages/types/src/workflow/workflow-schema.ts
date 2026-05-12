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
  CheckpointConfig,
  TriggeredSubworkflowConfig,
  WorkflowConfig,
  WorkflowMetadata,
} from "./index.js";
import type { VariableDefinition } from "../workflow-execution/variables.js";
import { ToolApprovalOptionsSchema } from "../tool/tool-schema.js";

/**
 * Variable Scope Schema
 */
export const variableScopeSchema: z.ZodType<VariableScope> = z.enum([
  "global",
  "execution",
  "subgraph",
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
 * Variable Definition Schema (NEW - Unified)
 * Replaces WorkflowVariable with VariableDefinition
 */
export const VariableDefinitionSchema: z.ZodType<VariableDefinition> = z.object({
  name: z.string().min(1, "Variable name is required"),
  type: variableValueTypeSchema,
  value: z.any(),
  scope: variableScopeSchema,
  readonly: z.boolean(),
  metadata: z.object({
    description: z.string().optional(),
    required: z.boolean().optional(),
  }).optional(),
});

/**
 * Legacy: Workflow Variable Schema
 * @deprecated Use VariableDefinitionSchema instead
 */
export const WorkflowVariableSchema: z.ZodType<any> = z.object({
  name: z.string().min(1, "Variable name is required"),
  type: variableValueTypeSchema,
  defaultValue: z.any().optional(),
  description: z.string().optional(),
  required: z.boolean().optional(),
  readonly: z.boolean().optional(),
  scope: variableScopeSchema.optional(),
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
  toolApproval: ToolApprovalOptionsSchema.optional(),
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
export const WorkflowTemplateBasicSchema = z.object({
  id: z.string().min(1, "Workflow ID is required"),
  name: z.string().min(1, "Workflow name is required"),
  description: z.string().optional(),
  version: z.string().min(1, "Workflow version is required"),
  createdAt: z.number(),
  updatedAt: z.number(),
});

/**
 * Complete Workflow Definition Schema
 * Validates the entire workflow structure including nodes, edges, and configuration.
 * This is a lightweight schema validation - deep business logic validation should be done by WorkflowValidator.
 */
export const WorkflowTemplateSchema = z.object({
  id: z.string().min(1, "Workflow ID is required"),
  name: z.string().min(1, "Workflow name is required"),
  type: z.enum(["STANDALONE", "TRIGGERED_SUBWORKFLOW", "DEPENDENT"]).optional(),
  description: z.string().optional(),
  nodes: z.array(z.any()).min(1, "Workflow must have at least one node"),
  edges: z.array(z.any()),
  variables: z.array(VariableDefinitionSchema).optional(),
  triggers: z.array(z.any()).optional(),
  triggeredSubworkflowConfig: TriggeredSubworkflowConfigSchema.optional(),
  config: WorkflowConfigSchema.optional(),
  metadata: WorkflowMetadataSchema.optional(),
  version: z.string().min(1, "Workflow version is required"),
  createdAt: z.number(),
  updatedAt: z.number(),
  availableTools: z
    .object({
      initial: z.array(z.string()),
      dynamic: z.set(z.string()).optional(),
      filterMode: z.enum(['none', 'allowlist', 'blocklist']).optional(),
      allowList: z.array(z.string()).optional(),
      blockList: z.array(z.string()).optional(),
    })
    .optional(),
});

/**
 * Type Guards
 */

/**
 * Check if a value is a valid VariableDefinition
 */
export function isVariableDefinition(value: unknown): value is VariableDefinition {
  try {
    VariableDefinitionSchema.parse(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Legacy: Check if a value is a valid WorkflowVariable
 * @deprecated Use isVariableDefinition instead
 */
export function isWorkflowVariable(value: unknown): value is any {
  try {
    WorkflowVariableSchema.parse(value);
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
