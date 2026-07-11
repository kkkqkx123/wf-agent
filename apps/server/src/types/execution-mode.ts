/**
 * Execution Mode Types
 *
 * Simplified version for Server (no CLI-specific execution modes needed)
 */

export const ExecutionModeEnvVars = {
  OUTPUT_FORMAT: "OUTPUT_FORMAT",
} as const;

export type ExecutionModeEnvVarsType = typeof ExecutionModeEnvVars;
