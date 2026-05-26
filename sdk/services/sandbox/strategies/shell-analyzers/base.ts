/**
 * Shell Analyzer — Base Types
 *
 * Common interface and result types for all shell-specific analyzers.
 */

import type { ShellPolicy } from "@wf-agent/types";

export type ShellType = "powershell" | "bash" | "cmd";

export interface ShellAnalysisResult {
  /** Whether the command passed static analysis */
  allowed: boolean;
  /** Reason for denial, if any */
  reason?: string;
  /** Sanitized command (e.g., with path restrictions) */
  command: string;
  /** Shell type that was identified */
  shellType: ShellType;
}

export interface ShellAnalysisContext {
  command: string;
  policy: ShellPolicy;
}

export interface ShellAnalyzer {
  readonly shellType: ShellType;
  analyze(ctx: ShellAnalysisContext): ShellAnalysisResult;
}