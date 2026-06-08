/**
 * Export from the Processors module
 * Export all configuration processing functions (pure functions)
 */

// Workflow processing function
export { validateWorkflow, transformWorkflow, exportWorkflow, parseWorkflow } from "./workflow.js";

// NodeTemplate processing function
export {
  validateNodeTemplate,
  transformNodeTemplate,
  exportNodeTemplate,
  parseNodeTemplate,
} from "./node-template.js";

// Script processing function
export { validateScript, transformScript, exportScript, parseScript } from "./script.js";

// Executor processing function
export {
  validateExecutorConfig,
  transformExecutorConfig,
  exportExecutorConfig,
} from "./script-executor.js";

// TriggerTemplate processing function
export {
  validateTriggerTemplate,
  transformTriggerTemplate,
  exportTriggerTemplate,
  parseTriggerTemplate,
} from "./trigger-template.js";

// HookTemplate processing function
export {
  validateHookTemplate,
  transformHookTemplate,
  exportHookTemplate,
  parseHookTemplate,
} from "./hook-template.js";

// LLM Profile processing function
export { validateLLMProfile, transformLLMProfile, exportLLMProfile, parseLLMProfile } from "./llm-profile.js";

// PromptTemplate processing function
export {
  parsePromptTemplateConfig,
  validatePromptTemplate,
  mergePromptTemplateConfig,
  loadAndMergePromptTemplate,
  transformPromptTemplate,
  exportPromptTemplate,
} from "./prompt-template.js";

// Agent Loop processing function
export {
  transformToAgentLoopConfig,
  exportAgentLoopConfig,
} from "./agent-loop.js";

// Metrics configuration processing function
export {
  mergeMetricsWithDefaults,
  getMetricsEnvironmentDefaults,
} from "./metrics.js";

// Timeout configuration processing function
export {
  mergeTimeoutWithDefaults,
  getTimeoutEnvironmentDefaults,
  validateTimeout,
  isWaitForever,
  toActualTimeout,
  WAIT_FOREVER,
} from "./timeout.js";

// Script Flow processing function
export {
  validateScriptFlow,
  transformScriptFlow,
  exportScriptFlow,
} from "./script-flow.js";

// Tools configuration processing function
export {
  validateReadFileConfig,
  transformReadFileConfig,
  exportReadFileConfig,
} from "./tools/index.js";

// Interactive Script configuration processing function
export {
  validateInteractiveScript,
  transformInteractiveScript,
  exportInteractiveScript,
} from "./script-interactive.js";

// File Checkpoint configuration processing function
export {
  mergeFileCheckpointConfig,
  toFileCheckpointManagerConfig,
} from "./file-checkpoint.js";

// Storage configuration processing function
export {
  mergeStorageWithDefaults,
  getStorageEnvironmentDefaults,
} from "./storage.js";

// Output configuration processing function
export {
  mergeOutputWithDefaults,
  getOutputEnvironmentDefaults,
} from "./output.js";

// Presets configuration processing function
export {
  mergePresetsWithDefaults,
  getPresetsEnvironmentDefaults,
} from "./presets.js";

// MCP connection configuration processing function
export {
  loadServerConfigs,
  createDefaultMcpSettings,
  mergeServerConfigs,
} from "../../../../services/mcp/mcp-connection-processor.js";

// Checkpoint Config (base class for multi-level checkpoint config resolution)
export {
  CheckpointConfigResolver,
  shouldCreateCheckpoint,
  getCheckpointDescription,
} from "./checkpoint-config.js";

export type {
  ConfigLayer,
  ConfigResolverOptions,
} from "./checkpoint-config.js";
