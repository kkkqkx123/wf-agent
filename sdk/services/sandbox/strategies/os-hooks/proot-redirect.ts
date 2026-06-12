/**
 * Proot-like Path Redirect Strategy
 *
 * Uses the proot binary for lightweight filesystem path redirection,
 * enabling sandboxed access to specific directories only.
 *
 * Falls back to passthrough when proot is not installed.
 */

import type { SandboxPolicy, ScriptExecutionResult, StrategyExecuteOptions } from "@wf-agent/types";
import type { StrategyImplementation } from "../../types.js";
import { getTerminalService, type TerminalService } from "../../../terminal/index.js";
import { execSync } from "child_process";
import { existsSync } from "fs";
import { executePassthrough } from "./base.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ProotLikeRedirectStrategy" });

export class ProotLikeRedirectStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "proot-redirect";
  name = "Proot-like Redirect (OS Hook)";
  description = "Filesystem path redirection via proot (requires proot binary on PATH)";
  priority = 40;

  private terminalService: TerminalService;

  constructor(terminalService?: TerminalService) {
    this.terminalService = terminalService || getTerminalService();
  }

  isAvailable(): boolean {
    const prootPath = this.findProotBinary();
    return prootPath !== null;
  }

  async execute(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    // Try to use proot for path redirection.
    const prootPath = this.findProotBinary();
    if (!prootPath) {
      logger.warn("proot binary not found, falling back to passthrough (no filesystem isolation)");
      return executePassthrough(this.terminalService, options, startTime);
    }

    // Build the proot redirect command from policy
    const redirectCommand = this.buildRedirectCommand(options, policy, prootPath);

    try {
      const result = await this.terminalService.executeOneOff(redirectCommand, {
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout,
      });

      return {
        success: result.success,
        scriptName: "sandbox-os-hook",
        stdout: result.stdout,
        stderr: result.stderr,
        exitCode: result.exitCode,
        executionTime: Date.now() - startTime,
        error: result.error,
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-os-hook",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Find the proot binary on the system.
   * Checks PATH first, then common installation paths.
   */
  private findProotBinary(): string | null {
    try {
      const which = execSync("which proot 2>/dev/null || where proot 2>nul", {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
        timeout: 1000,
      })
        .trim()
        .split("\n")[0];
      if (which) return which;
    } catch {
      // Not found on PATH
    }

    // Check common installation paths
    const candidates = [
      "/usr/bin/proot",
      "/usr/local/bin/proot",
      "/opt/proot/bin/proot",
      "/bin/proot",
    ];
    for (const p of candidates) {
      if (existsSync(p)) return p;
    }

    return null;
  }

  /**
   * Build a proot command that applies path redirections from the policy.
   *
   * Format:
   *   proot [-b <host>:<guest> ...] [-w <cwd>] -- <original-command>
   */
  private buildRedirectCommand(
    options: StrategyExecuteOptions,
    policy: SandboxPolicy,
    prootPath: string,
  ): string {
    const args: string[] = [prootPath];

    // Add bind mounts from filesystem policy
    const allowedReadPaths = policy.filesystem?.allowedReadPaths ?? [];
    const allowedWritePaths = policy.filesystem?.allowedWritePaths ?? [];

    for (const path of allowedReadPaths) {
      args.push("-b", path);
    }

    for (const path of allowedWritePaths) {
      args.push("-b", path);
    }

    // Set working directory
    if (options.cwd) {
      args.push("-w", options.cwd);
    }

    // Add the original command after --
    args.push("--", options.command);

    return args.join(" ");
  }
}
