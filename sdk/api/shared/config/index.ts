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
  ConfigFile,
  ParsedConfig,
  ParsedWorkflowConfig,
  ParsedNodeTemplateConfig,
  ParsedTriggerTemplateConfig,
  ParsedScriptConfig,
  ParsedLLMProfileConfig,
  ParsedPromptTemplateConfig,
  IConfigParser,
  IConfigTransformer,
} from "./types.js";

// analyzer
export { ConfigParser } from "./config-parser.js";

// Configuration Tool Functions
export { detectConfigFormat, readConfigFile, loadConfigContent } from "./config-utils.js";

// JSON Parsing Functions
export { parseJson, stringifyJson, validateJsonSyntax } from "./json-parser.js";

// TOML parsing function
export { parseToml, validateTomlSyntax } from "./toml-parser.js";

// resolver
export { ConfigTransformer } from "./config-transformer.js";

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
} from "./parsers.js";

// Validator
// Validation Tool Function Export
export {
  validateRequiredFields,
  validateStringField,
  validateNumberField,
  validateBooleanField,
  validateArrayField,
  validateObjectField,
  validateEnumField,
} from "./validators/base-validator.js";

// Configuration Validation Function Export
export { validateWorkflowConfig } from "./validators/workflow-validator.js";
export { validateNodeTemplateConfig } from "./validators/node-template-validator.js";
export { validateTriggerTemplateConfig } from "./validators/trigger-template-validator.js";
export { validateScriptConfig } from "./validators/script-validator.js";
export { validateLLMProfileConfig } from "./validators/llm-profile-validator.js";
export { validatePromptTemplateConfig } from "./validators/prompt-template-validator.js";

// Batch Validation Function Export
// Note: The return type is Result<T[], ValidationError[][]>, a 2D array when errors occur, each config's error list
export {
  validateBatchWorkflows,
  validateBatchNodeTemplates,
  validateBatchTriggerTemplates,
  validateBatchScripts,
} from "./validators/batch-validators.js";

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
  loadAndTransformPromptTemplate,
  // Agent Loop
  parseAgentLoopConfig,
  parseAndValidateAgentLoopConfig,
  transformToAgentLoopConfig,
  loadAgentLoopConfig,
} from "./processors/index.js";

// Cue word template loader export
export {
  loadPromptTemplateConfig,
  mergePromptTemplateConfig,
  loadAndMergePromptTemplate,
} from "./prompt-template-loader.js";
