/**
 * Configuration module entry file
 * Export all configuration parsing related classes and types
 *
 * Design principles:
 * - Stateless design, all functions are pure functions.
 * - Configuration validation uses the validator in sdk/core/validation.
 * - This module is only responsible for parsing and converting configuration content.
 * - No direct operation of the registry, configuration registration is the responsibility of the application layer.
 * - Supports multiple configuration types: workflows, node templates, trigger templates, scripts, LLM Profiles.
 */

// type definition
export {
  ConfigFormat,
  ConfigType,
  NodeConfigFile,
  EdgeConfigFile,
  WorkflowConfigFile,
  NodeTemplateConfigFile,
  HookTemplateConfigFile,
  TriggerTemplateConfigFile,
  ScriptConfigFile,
  LLMProfileConfigFile,
  PromptTemplateConfigFile,
  AgentLoopConfigFile,
  AgentHookConfigFile,
  AgentTriggerConfigFile,
  ConfigFile,
  ParsedConfig,
  ParsedWorkflowConfig,
  ParsedNodeTemplateConfig,
  ParsedHookTemplateConfig,
  ParsedTriggerTemplateConfig,
  ParsedScriptConfig,
  ParsedLLMProfileConfig,
  ParsedPromptTemplateConfig,
  ParsedAgentLoopConfig,
} from "./types.js";

// Parameter Substitution Utility
export { substituteParameters } from "./config-utils.js";

// JSON Parsing Functions - from parsers/
export { parseJson, stringifyJson, validateJsonSyntax } from "./parsers/index.js";

// TOML parsing function - from parsers/
export {
  initializeTomlParser,
  isTomlParserInitialized,
  parseToml,
  validateTomlSyntax,
} from "./parsers/index.js";

// Format detection utility (pure string operation)
export { getConfigFormatFromPath } from "./parsers/index.js";

// Configuration handler export (pure function)
export {
  // Workflow
  parseWorkflow,
  validateWorkflow,
  transformWorkflow,
  exportWorkflow,
  // NodeTemplate
  parseNodeTemplate,
  validateNodeTemplate,
  transformNodeTemplate,
  exportNodeTemplate,
  // Script
  parseScript,
  validateScript,
  transformScript,
  exportScript,
  // HookTemplate
  parseHookTemplate,
  validateHookTemplate,
  transformHookTemplate,
  exportHookTemplate,
  // TriggerTemplate
  parseTriggerTemplate,
  validateTriggerTemplate,
  transformTriggerTemplate,
  exportTriggerTemplate,
  // LLM Profile
  parseLLMProfile,
  validateLLMProfile,
  transformLLMProfile,
  exportLLMProfile,
  // PromptTemplate
  parsePromptTemplateConfig,
  validatePromptTemplate,
  mergePromptTemplateConfig,
  loadAndMergePromptTemplate,
  transformPromptTemplate,
  exportPromptTemplate,
  // Agent Loop
  transformToAgentLoopConfig,
  exportAgentLoopConfig,
  // Metrics
  mergeMetricsWithDefaults,
  getMetricsEnvironmentDefaults,
  // Timeout
  mergeTimeoutWithDefaults,
  getTimeoutEnvironmentDefaults,
  // File Checkpoint
  mergeFileCheckpointConfig,
  toFileCheckpointManagerConfig,
  // Storage
  mergeStorageWithDefaults,
  getStorageEnvironmentDefaults,
  // Output
  mergeOutputWithDefaults,
  getOutputEnvironmentDefaults,
  // Presets
  getPresetsEnvironmentDefaults,
  validatePresetsConfig,
  transformPresetsConfig,
  exportPresetsConfig,
  // Sandbox
  mergeSandboxWithDefaults,
  validateSandboxConfig,
  transformSandboxConfig,
  exportSandboxConfig,
} from "./processors/index.js";

// Tool processors
export {
  validateReadFileConfig,
  transformReadFileConfig,
  exportReadFileConfig,
} from "./processors/index.js";

// MCP Configuration — consolidated under api/shared/config as the single config entry point
export {
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
} from "./processors/index.js";