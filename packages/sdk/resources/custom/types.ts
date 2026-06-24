/**
 * Custom Resources Type Definitions
 *
 * Defines types for custom resources loaded from configuration files.
 * Custom resources are user-defined tools, triggers, and prompts that extend
 * the predefined resources provided by the SDK.
 */

import type { ToolParameterSchema, ToolType } from "@wf-agent/types";

/**
 * Custom Tool Definition
 *
 * Represents a user-defined tool that can be loaded from configuration files.
 */
export interface CustomToolDefinition {
  /** Tool ID (must be unique) */
  id: string;
  /** Tool type (STATELESS or STATEFUL) */
  type: ToolType;
  /** Tool description */
  description: {
    summary: string;
    details?: string;
    examples?: string[];
  };
  /** Parameter schema (JSON Schema) */
  schema: ToolParameterSchema;
  /** Handler configuration */
  handler: {
    type: "file" | "inline" | "rpc";
    path?: string; // For file type
    code?: string; // For inline type
    endpoint?: string; // For rpc type
  };
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Custom Trigger Definition
 *
 * Represents a user-defined trigger that can be loaded from configuration files.
 */
export interface CustomTriggerDefinition {
  /** Trigger name (must be unique) */
  name: string;
  /** Trigger description */
  description: string;
  /** Trigger condition definition */
  condition: {
    type: "event" | "schedule" | "webhook";
    /** Event type or schedule expression or webhook path */
    value: string;
  };
  /** Optional trigger configuration */
  config?: Record<string, unknown>;
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Custom Prompt Definition
 *
 * Represents a user-defined prompt template that can be loaded from configuration files.
 */
export interface CustomPromptDefinition {
  /** Prompt ID (must be unique) */
  id: string;
  /** Prompt name */
  name: string;
  /** Prompt content/template */
  content: string;
  /** Prompt type (system, user, assistant) */
  type: "system" | "user" | "assistant";
  /** Optional variables that can be substituted in the prompt */
  variables?: string[];
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Loaded Custom Resources
 *
 * Collection of custom resources loaded from configuration files.
 */
export interface CustomResources {
  /** Loaded custom tools */
  tools: CustomToolDefinition[];
  /** Loaded custom triggers */
  triggers: CustomTriggerDefinition[];
  /** Loaded custom prompts */
  prompts: CustomPromptDefinition[];
  /** Loading errors encountered during load (non-fatal) */
  errors: Array<{
    path: string;
    error: string;
  }>;
}

/**
 * Custom Resources Preset Configuration
 *
 * Configuration for loading custom resources from files.
 */
export interface CustomResourcesPresetConfig {
  /** Enable/disable custom resources */
  enabled: boolean;
  /** Path to custom tools definition file (relative or absolute) */
  toolsPath?: string;
  /** Path to custom triggers definition file (relative or absolute) */
  triggersPath?: string;
  /** Path to custom prompts definition file (relative or absolute) */
  promptsPath?: string;
  /** Validation level for loaded resources */
  validationLevel?: "strict" | "lenient";
}
