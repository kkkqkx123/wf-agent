/**
 * Configuration module entry file
 * Export all configuration parsing related classes and types
 *
 * Design principles:
 * - Stateless design, all functions are pure functions.
 * - Configuration validation uses the validator in sdk/core/validation.
 * - This module is only responsible for parsing and transforming configuration content.
 * - This module is only responsible for parsing and converting configuration contents.
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
  ParsedTriggerTemplateConfig,
  ParsedScriptConfig,
  ParsedLLMProfileConfig,
  ParsedPromptTemplateConfig,
  ParsedAgentLoopConfig,
  IConfigParser,
  IConfigTransformer,
} from "./types.js";

// analyzer
export { ConfigParser } from "./config-parser.js";

// Configuration Tool Functions (re-exported from utils for backward compatibility)
export { loadAgentLoopConfig } from "./utils/config-utils.js";

// Parameter Substitution Utility
export { substituteParameters } from "./utils/config-utils.js";

// Configuration File Loader (File I/O operations) - from loaders/
export {
  readConfigFile,
  getConfigFormatFromPath,
  loadConfigFile,
  tryLoadConfigFile,
  createConfigFileLoader,
  // Backward compatibility aliases
  getConfigFormatFromPath as detectConfigFormat,
  loadConfigFile as loadConfigContent,
} from "./loaders/index.js";

// JSON Parsing Functions - from parsers/
export { parseJson, stringifyJson, validateJsonSyntax } from "./parsers/index.js";

// TOML parsing function - from parsers/
export {
  initializeTomlParser,
  isTomlParserInitialized,
  parseToml,
  validateTomlSyntax,
} from "./parsers/index.js";

// resolver - from utils/
export { ConfigTransformer } from "./utils/index.js";

// Configure parsing functions (recommended)
export {
  parseWorkflow,
  validateWorkflowByContent,
  parseWorkflowConfig,
  parseBatchWorkflows,
  parseNodeTemplate,
  parseBatchNodeTemplates,
  parseTriggerTemplate,
  parseBatchTriggerTemplates,
  parseScript,
  parseBatchScripts,
  parseLLMProfile,
  parseBatchLLMProfiles,
} from "./parse-functions.js";

// Configuration handler export (pure function)
export {
  // Workflow
  validateWorkflow,
  transformWorkflow,
  exportWorkflow,
  // NodeTemplate
  validateNodeTemplate,
  transformNodeTemplate,
  exportNodeTemplate,
  // Script
  validateScript,
  transformScript,
  exportScript,
  // TriggerTemplate
  validateTriggerTemplate,
  transformTriggerTemplate,
  exportTriggerTemplate,
  // LLM Profile
  validateLLMProfile,
  transformLLMProfile,
  exportLLMProfile,
  // PromptTemplate
  validatePromptTemplate,
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
} from "./processors/index.js";

// Cue word template loader export - from loaders/
export {
  loadPromptTemplateConfig,
  mergePromptTemplateConfig,
  loadAndMergePromptTemplate,
} from "./loaders/prompt-template-config-loader.js";

// Metrics configuration loader export (with file I/O) - from loaders/
export { loadMetricsConfigFromFile } from "./loaders/metrics-config-loader.js";

// Timeout configuration loader export (with file I/O) - from loaders/
export { loadTimeoutConfigFromFile } from "./loaders/timeout-config-loader.js";

// File checkpoint configuration loader
export { loadFileCheckpointConfigFromFile } from "./loaders/file-checkpoint-config-loader.js";

// MCP Configuration — consolidated under api/shared/config as the single config entry point
export {
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
} from "./processors/index.js";
export {
  DEFAULT_MCP_SETTINGS_FILE,
  PROJECT_MCP_FILE,
  loadMcpSettings,
  fileExists,
  getGlobalMcpSettingsPath,
  getProjectMcpPath,
  writeMcpSettings,
  ensureMcpSettingsFile,
} from "./loaders/index.js";
