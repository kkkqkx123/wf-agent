/**
 * CLI Mode Detector
 * Thin wrapper around @wf-agent/runtime/mode for backward compatibility.
 *
 * The core mode detection logic has been moved to @wf-agent/runtime/mode
 * to eliminate duplication between cli-app and server.
 */

export {
  getMode,
  getOutputFormat,
  isJsonMode,
  isSilentMode,
  isHeadless,
  isProgrammatic,
  isInteractive,
  invalidateModeCache,
} from "@wf-agent/runtime/mode";

export type { ModeDetectionResult } from "@wf-agent/runtime/mode";
