/**
 * Definition of backend-shell tool status types
 */

import type { ChildProcess } from "child_process";

/**
 * Backend Shell Status
 */
export interface BackendShell {
  shellId: string;
  command: string;
  process: ChildProcess;
  startTime: number;
  outputLines: string[];
  lastReadIndex: number;
  status: "running" | "completed" | "failed" | "terminated" | "error";
  exitCode: number | null;
}

/**
 * Shell output results
 */
export interface ShellOutputResult {
  success: boolean;
  content: string;
  error?: string;
  stdout: string;
  stderr: string;
  exitCode: number;
  shellId?: string;
}
