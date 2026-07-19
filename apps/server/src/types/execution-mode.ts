/**
 * Server Execution Mode Types
 * Thin re-export wrapper around @wf-agent/runtime/mode for backward compatibility.
 *
 * The core execution mode types have been moved to @wf-agent/runtime/mode
 * to eliminate duplication between cli-app and server.
 */

export type { ExecutionMode, OutputFormat } from "@wf-agent/runtime/mode";
export { ExecutionModeEnvVars } from "@wf-agent/runtime/mode";
