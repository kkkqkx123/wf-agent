/**
 * Zod Schemas for Workflow Boundary Configuration
 * Provides runtime validation schemas for workflow boundary data passing
 */

import { z } from "zod";

/**
 * Variable input mapping schema
 * Defines how external variables are mapped into the workflow's internal scope
 */
export const WorkflowVariableInputSchema = z.object({
  externalName: z.string().min(1, "External name (caller variable) is required"),
  internalName: z.string().min(1, "Internal name (workflow variable) is required"),
  required: z.boolean().optional(),
  defaultValue: z.any().optional(),
  description: z.string().optional(),
});

/**
 * Variable output mapping schema
 * Defines how internal variables are returned to the caller
 */
export const WorkflowVariableOutputSchema = z.object({
  internalName: z.string().min(1, "Internal name (workflow variable) is required"),
  externalName: z.string().min(1, "External name (caller variable) is required"),
  description: z.string().optional(),
});

/**
 * Message context input schema
 * Defines how named message contexts are passed into the workflow
 */
export const WorkflowMessageInputSchema = z.object({
  externalName: z.string().min(1, "External context ID is required"),
  internalName: z.string().min(1, "Internal context ID is required"),
  required: z.boolean().optional(),
  defaultMessages: z.array(z.any()).optional(),
  description: z.string().optional(),
});

/**
 * Message context output schema
 * Defines how named message contexts are returned from the workflow
 */
export const WorkflowMessageOutputSchema = z.object({
  internalName: z.string().min(1, "Internal context ID is required"),
  externalName: z.string().min(1, "External context ID is required"),
  description: z.string().optional(),
});

/**
 * Data input mapping schema
 * Defines how execution input data fields are mapped to internal variables
 */
export const WorkflowDataInputSchema = z.object({
  parentField: z.string().min(1, "Parent input field name is required"),
  internalName: z.string().min(1, "Internal variable name is required"),
  required: z.boolean().optional(),
  defaultValue: z.any().optional(),
  description: z.string().optional(),
});

/**
 * Data output mapping schema
 * Defines how internal variables are mapped to execution output keys
 */
export const WorkflowDataOutputSchema = z.object({
  internalName: z.string().min(1, "Internal variable name is required"),
  outputKey: z.string().min(1, "Output key is required"),
  description: z.string().optional(),
});

/**
 * Workflow start configuration schema
 * Used by: START, SUBGRAPH_START, START_FROM_TRIGGER nodes
 */
export const WorkflowStartConfigSchema = z.object({
  variableInputs: z.array(WorkflowVariableInputSchema).optional(),
  messageInputs: z.array(WorkflowMessageInputSchema).optional(),
  dataInputs: z.array(WorkflowDataInputSchema).optional(),
});

/**
 * Workflow end configuration schema
 * Used by: END, SUBGRAPH_END, CONTINUE_FROM_TRIGGER nodes
 */
export const WorkflowEndConfigSchema = z.object({
  variableOutputs: z.array(WorkflowVariableOutputSchema).optional(),
  messageOutputs: z.array(WorkflowMessageOutputSchema).optional(),
  dataOutputs: z.array(WorkflowDataOutputSchema).optional(),
});

/**
 * Type guards
 */
export const isWorkflowVariableInput = (config: unknown): config is z.infer<typeof WorkflowVariableInputSchema> => {
  return WorkflowVariableInputSchema.safeParse(config).success;
};

export const isWorkflowVariableOutput = (config: unknown): config is z.infer<typeof WorkflowVariableOutputSchema> => {
  return WorkflowVariableOutputSchema.safeParse(config).success;
};

export const isWorkflowMessageInput = (config: unknown): config is z.infer<typeof WorkflowMessageInputSchema> => {
  return WorkflowMessageInputSchema.safeParse(config).success;
};

export const isWorkflowMessageOutput = (config: unknown): config is z.infer<typeof WorkflowMessageOutputSchema> => {
  return WorkflowMessageOutputSchema.safeParse(config).success;
};

export const isWorkflowStartConfig = (config: unknown): config is z.infer<typeof WorkflowStartConfigSchema> => {
  return WorkflowStartConfigSchema.safeParse(config).success;
};

export const isWorkflowEndConfig = (config: unknown): config is z.infer<typeof WorkflowEndConfigSchema> => {
  return WorkflowEndConfigSchema.safeParse(config).success;
};

export const isWorkflowDataInput = (config: unknown): config is z.infer<typeof WorkflowDataInputSchema> => {
  return WorkflowDataInputSchema.safeParse(config).success;
};

export const isWorkflowDataOutput = (config: unknown): config is z.infer<typeof WorkflowDataOutputSchema> => {
  return WorkflowDataOutputSchema.safeParse(config).success;
};
