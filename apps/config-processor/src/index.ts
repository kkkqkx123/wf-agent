/**
 * Config Processor — File I/O orchestration for SDK configuration loading.
 *
 * This package sits at the application layer and provides the glue between:
 *   - raw config files on disk
 *   - the SDK's pure parsers (sdk/api/shared/config/parsers)
 *   - the SDK's pure processors (sdk/api/shared/config/processors)
 *
 * Each exported function reads a config file (or tries multiple paths), delegates
 * to the SDK for parsing and merging, and returns a fully-typed config object.
 *
 * Usage (in DI containers, CLI bootstrapping, etc.):
 * ```ts
 * import { loadMetricsConfig, loadTimeoutConfig, tryLoadConfigFile } from "@wf-agent/config-processor";
 *
 * const loaded = await tryLoadConfigFile("./configs/metrics.toml");
 * const metrics = await loadMetricsConfig(["./configs/metrics.toml"]);
 * ```
 */

// Generic file I/O utilities (re-exported from local copy)
export {
  readConfigFile,
  loadConfigFile,
  tryLoadConfigFile,
} from "./config-file-loader.js";

// Domain-specific config loaders
export {
  loadMetricsConfig,
  loadTimeoutConfig,
  loadFileCheckpointConfig,
  loadStorageConfig,
  loadOutputConfig,
  loadPresetsConfig,
  loadInfrastructureConfigs,
  loadDefaultInfrastructureConfigs,
  DEFAULT_INFRA_PRESET,
  loadAgentLoopConfig,
  loadLLMProfileConfig,
  loadNodeTemplateConfig,
  loadTriggerTemplateConfig,
  loadHookTemplateConfig,
  loadScriptConfig,
  loadPromptTemplateConfig,
} from "./loader-orchestrator.js";

// MCP settings file I/O
export {
  DEFAULT_MCP_SETTINGS_FILE,
  PROJECT_MCP_FILE,
  PROJECT_WF_MCP_FILE,
  fileExists,
  getGlobalMcpSettingsPath,
  getProjectMcpPath,
  getProjectWfMcpPath,
  getProjectMcpPaths,
  loadMcpSettings,
  writeMcpSettings,
  ensureMcpSettingsFile,
  loadAndMergeMcpSettings,
} from "./mcp-settings-loader.js";

// Skill settings file I/O (follows same global/project pattern as MCP)
export {
  DEFAULT_SKILL_SETTINGS_FILE,
  PROJECT_SKILL_FILE,
  PROJECT_WF_SKILL_FILE,
  fileExists as skillFileExists,
  createDefaultSkillConfig,
  getGlobalSkillSettingsPath,
  getProjectSkillPath,
  getProjectWfSkillPath,
  getProjectSkillPaths,
  loadSkillConfig,
  writeSkillConfig,
  ensureSkillConfigFile,
  mergeSkillConfigs,
  loadAndMergeSkillConfig,
} from "./skill-settings-loader.js";

// Configuration index loaders
export {
  loadIndexFile,
  tryLoadIndexFile,
  indexFileExists,
  resolveLLMProfileIndex,
  resolveWorkflowIndex,
  resolveNodeTemplateIndex,
  resolveScriptIndex,
  resolvePromptTemplateIndex,
  resolveAgentLoopIndex,
  filterByTags,
  filterByCategory,
  findEntryById,
} from "./config-index-loader.js";