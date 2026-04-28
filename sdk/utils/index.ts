/**
 * Global utility functions for the SDK module
 */

// Token Encoding Tool
export {
  encodeText,
  encodeObject,
  StreamingTokenCounter,
  countMessageTokens,
} from "./token-encoder.js";

// Token Estimator
export { TokenEstimator, defaultEstimator, estimateTokens } from "./token-estimator.js";

// TOML parser
export { TomlParserManager } from "./toml-parser-manager.js";

// Log tool
export {
  sdkLogger,
  graphLogger,
  agentLogger,
  createSDKModuleLogger,
  createGraphModuleLogger,
  createAgentModuleLogger,
  configureSDKLogger,
  initializeSDKLogger,
  initializeGraphLogger,
  initializeAgentLogger,
} from "./logger.js";
export { ContextualLogger, createContextualLogger } from "./contextual-logger.js";

// Metadata tools
export { getMetadata, hasMetadata, mergeMetadata } from "./metadata-utils.js";

// ID Tool
export {
  generateId,
  isValidId,
  validateId,
  generateSubgraphNamespace,
  generateNamespacedNodeId,
  generateNamespacedEdgeId,
  extractOriginalId,
  isNamespacedId,
} from "./id-utils.js";

// Versioning Tools
export {
  initialVersion,
  parseVersion,
  nextMajorVersion,
  nextMinorVersion,
  nextPatchVersion,
  compareVersion,
  autoIncrementVersion,
  parseFullVersion,
} from "./version-utils.js";

// Tool Utilities
export * from "./tool-utils.js";
