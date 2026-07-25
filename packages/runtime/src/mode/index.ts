/**
 * Runtime Mode Module Exports
 */

export type { ExecutionMode, OutputFormat } from "./types.js";
export { ExecutionModeEnvVars } from "./types.js";
export {
  getMode,
  getOutputFormat,
  isJsonMode,
  isSilentMode,
  isHeadless,
  isInteractive,
  isTest,
  invalidateModeCache,
} from "./detector.js";
// isProgrammatic is intentionally excluded — it has been merged into isHeadless.
export type { ModeDetectionResult } from "./detector.js";