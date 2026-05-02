/**
 * Configure module type definitions
 * Define types related to the parsing and conversion of configuration files
 *
 * Design principles:
 * - Fully reuse the core type definitions from sdk/types
 * - The format of the configuration files must be exactly consistent with the corresponding type definitions
 * - All fields are statically defined; no fields should be removed
 * - Avoid duplicate definitions to ensure type safety
 *
 * Explanation:
 * - WorkflowDefinition represents the static definition after parsing the configuration file
 * - The configuration files are directly mapped to the corresponding types, with exact consistency
 * - No type conversion or field removal is required
 * - Multiple configuration types are supported: workflows, node templates, trigger templates, scripts, and Agent Loop
 */

import type { Node } from "@wf-agent/types";
import type { Edge } from "@wf-agent/types";
import type { WorkflowDefinition } from "@wf-agent/types";
import type { NodeTemplate } from "@wf-agent/types";
import type { TriggerTemplate } from "@wf-agent/types";
import type { Script } from "@wf-agent/types";
import type { LLMProfile } from "@wf-agent/types";
import type { AgentLoopConfigFile } from "@wf-agent/types";
import type { Tool } from "@wf-agent/types";

/**
 * Configuration Format
 */
export type ConfigFormat = "toml" /** TOML format */ | "json"; /** JSON format */

/**
 * Node Configuration File Format
 *
 * Description: Directly reuse the Node type; it's completely identical.
 */
export type NodeConfigFile = Node;

/**
 * Edge Configuration File Format
 *
 * Note: Simply reuse the Edge type; it is completely consistent.
 */
export type EdgeConfigFile = Edge;

/**
 * Workflow Configuration File Format
 *
 * Description: Directly reuse the WorkflowDefinition type; it is completely identical.
 */
export type WorkflowConfigFile = WorkflowDefinition;

/**
 * Node Template Configuration File Format
 *
 * Description: Directly reuse the NodeTemplate type; it is completely identical.
 */
export type NodeTemplateConfigFile = NodeTemplate;

/**
 * Trigger Template Configuration File Format
 *
 * Description: Directly reuse the TriggerTemplate type; it is completely identical.
 */
export type TriggerTemplateConfigFile = TriggerTemplate;

/**
 * Script Configuration File Format
 *
 * Note: Simply reuse the Script type; it is completely identical.
 */
export type ScriptConfigFile = Script;

/**
 * LLM Profile Format
 *
 * Description: Directly reuse the LLMProfile type, which is completely consistent.
 */
export type LLMProfileConfigFile = LLMProfile;

/**
 * Agent Loop Configuration File Format
 *
 * Note: Simply reuse the AgentLoopConfigFile type; it is exactly the same.
 */
export type AgentLoopProfileConfigFile = AgentLoopConfigFile;

/**
 * Tool Configuration File Format
 *
 * Description: Reuse the Tool type, but without runtime functions (execute, factory).
 * Tool configuration files only contain metadata (id, name, type, description, parameters, config, metadata).
 * Runtime functions must be injected separately in code.
 */
export type ToolConfigFile = Omit<Tool, "config"> & {
  config?: unknown;
};

/**
 * Tip Word Template Configuration File Format
 */
export interface PromptTemplateConfigFile {
  /** Template ID */
  id: string;
  /** Template Name */
  name?: string;
  /** Template Description */
  description?: string;
  /** Template Category */
  category?: "system" | "rules" | "user-command" | "tools" | "composite";
  /** Template content (overriding the default template) */
  content?: string;
  /** Variable definition (merged into the default template) */
  variables?: Array<{
    name: string;
    type: "string" | "number" | "boolean" | "object";
    required: boolean;
    description?: string;
    defaultValue?: unknown;
  }>;
  /** List of referenced snippet IDs (merged into the default template) */
  fragments?: string[];
}

/**
 * Configuration Type
 */
export type ConfigType =
  | "workflow" /** Workflow Configuration */
  | "node_template" /** Node Template Configuration */
  | "trigger_template" /** Trigger Template Configuration */
  | "script" /** Script Configuration */
  | "llm_profile" /** LLM Profile Placement */
  | "prompt_template" /** Cue word template configuration */
  | "agent_loop" /** Agent Loop Configuration */
  | "tool"; /** Tool Configuration */

/**
 * Common configuration file types
 */
export type ConfigFile =
  | WorkflowConfigFile
  | NodeTemplateConfigFile
  | TriggerTemplateConfigFile
  | ScriptConfigFile
  | LLMProfileConfigFile
  | PromptTemplateConfigFile
  | AgentLoopProfileConfigFile
  | ToolConfigFile;

/**
 * Type mapping from ConfigType to corresponding config file type
 * Uses a mapped type for better readability and maintainability
 */
type ConfigTypeToFileMap = {
  workflow: WorkflowConfigFile;
  node_template: NodeTemplateConfigFile;
  trigger_template: TriggerTemplateConfigFile;
  script: ScriptConfigFile;
  llm_profile: LLMProfileConfigFile;
  prompt_template: PromptTemplateConfigFile;
  agent_loop: AgentLoopProfileConfigFile;
  tool: ToolConfigFile;
};

/**
 * The parsed configuration object (universal version)
 */
export interface ParsedConfig<T extends ConfigType = ConfigType> {
  /** Configuration type */
  configType: T;
  /** Configuration Format */
  format: ConfigFormat;
  /** Configuration file content - type-safe mapping based on configType */
  config: ConfigTypeToFileMap[T];
  /** Raw configuration file content (original string) */
  rawContent: string;
}

// Backward-compatible type aliases
export type ParsedWorkflowConfig = ParsedConfig<"workflow">;
export type ParsedNodeTemplateConfig = ParsedConfig<"node_template">;
export type ParsedTriggerTemplateConfig = ParsedConfig<"trigger_template">;
export type ParsedScriptConfig = ParsedConfig<"script">;
export type ParsedLLMProfileConfig = ParsedConfig<"llm_profile">;
export type ParsedPromptTemplateConfig = ParsedConfig<"prompt_template">;
export type ParsedAgentLoopConfig = ParsedConfig<"agent_loop">;

/**
 * Configure the parser interface
 *
 * Design principles:
 * - It is only responsible for parsing and validating configuration content, not involving file I/O operations.
 * - File reading and other I/O operations are the responsibility of the application layer.
 */
export interface IConfigParser {
  /**
   * Parse the content of the configuration file
   * @param content: The content of the configuration file
   * @param format: The format of the configuration
   * @param configType: The type of configuration (optional, default is WORKFLOW)
   * @returns: The parsed configuration object
   */
  parse<T extends ConfigType = "workflow">(
    content: string,
    format: ConfigFormat,
    configType?: T,
  ): ParsedConfig<T>;

  /**
   * Verify the validity of the configuration
   * @param config The parsed configuration
   * @returns The verification result
   */
  validate<T extends ConfigType>(config: ParsedConfig<T>): unknown;

  /**
   * Parse and validate the configuration (universal method)
   * @param content: The content of the configuration file
   * @param format: The format of the configuration
   * @param configType: The type of the configuration
   * @returns: The validated configuration object
   */
  parseAndValidate<T extends ConfigType>(
    content: string,
    format: ConfigFormat,
    configType: T,
  ): ParsedConfig<T>;
}

/**
 * Configure the converter interface
 */
export interface IConfigTransformer {
  /**
   * Convert the configuration file format to WorkflowDefinition
   * @param configFile The parsed configuration file
   * @param parameters Runtime parameters (used for template replacement)
   * @returns WorkflowDefinition
   */
  transformToWorkflow(
    configFile: WorkflowConfigFile,
    parameters?: Record<string, unknown>,
  ): WorkflowDefinition;
}
