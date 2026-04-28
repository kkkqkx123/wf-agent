/**
 * PowerShell Executor
 * Invokes a PowerShell process to execute a PowerShell script
 */

import { CommandLineExecutor } from "../core/base/CommandLineExecutor.js";
import type { Script } from "@wf-agent/types";
import type { ExecutorConfig } from "../core/types.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("powershell-executor");

/**
 * PowerShell Executor
 */
export class PowerShellExecutor extends CommandLineExecutor<"POWERSHELL"> {
  private powerShellCommand: string;

  constructor(config?: Omit<ExecutorConfig, "type"> & { type: "POWERSHELL" }) {
    super({
      ...config,
      type: "POWERSHELL",
    });
    // By default, `pwsh` (PowerShell Core) is used; if it's not available, `powershell` (Windows PowerShell) is used instead.
    this.powerShellCommand = "pwsh";
    logger.debug("PowerShell executor initialized", { powerShellCommand: this.powerShellCommand });
  }

  /**
   * Set the PowerShell command
   * @param command The PowerShell command (e.g., 'pwsh', 'powershell', '/usr/bin/pwsh')
   */
  setPowerShellCommand(command: string): void {
    this.powerShellCommand = command;
    logger.debug("PowerShell command updated", { powerShellCommand: command });
  }

  /**
   * Get command line configuration
   * @param script Script definition
   * @returns Command line configuration
   */
  protected getCommandLineConfig(script: Script) {
    return {
      command: this.powerShellCommand,
      args: ["-Command", script.content || ""],
      shell: true,
    };
  }
}
