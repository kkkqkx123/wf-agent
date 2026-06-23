/**
 * Definition of backend-shell tool status types
 */

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
