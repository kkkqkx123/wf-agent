/**
 * Shell Analyzer — Base Types
 *
 * Common interface and result types for all shell-specific analyzers.
 * ShellType hierarchy comes from @wf-agent/types (script-executor.ts).
 */

import type { ShellPolicy } from "@wf-agent/types";
import type { ScriptShellType } from "@wf-agent/types";

export type { ScriptShellType as ShellType, ShellPolicy };

export interface ShellAnalysisResult {
  /** Whether the command passed static analysis */
  allowed: boolean;
  /** Reason for denial, if any */
  reason?: string;
  /** Sanitized command (e.g., with path restrictions) */
  command: string;
  /** Shell type that was identified */
  shellType: ScriptShellType;
}

export interface ShellAnalysisContext {
  command: string;
  policy: ShellPolicy;
}

export interface ShellAnalyzer {
  readonly shellType: ScriptShellType;
  analyze(ctx: ShellAnalysisContext): ShellAnalysisResult;
}