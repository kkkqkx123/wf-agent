/**
 * Python Executor
 * Calling the System Python Interpreter to Execute Python Scripts
 */

import { CommandLineExecutor } from "../core/base/CommandLineExecutor.js";
import type { Script } from "@wf-agent/types";
import type { ExecutorConfig } from "../core/types.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("python-executor");

/**
 * Python executor
 */
export class PythonExecutor extends CommandLineExecutor<"PYTHON"> {
  private pythonCommand: string;

  constructor(config?: Omit<ExecutorConfig, "type"> & { type: "PYTHON" }) {
    super({
      ...config,
      type: "PYTHON",
    });
    // Python3 is used by default, or python if it doesn't exist.
    this.pythonCommand = "python3";
    logger.debug("Python executor initialized", { pythonCommand: this.pythonCommand });
  }

  /**
   * Setting up Python commands
   * @param command Python command (e.g. 'python3', 'python', '/usr/bin/python3')
   */
  setPythonCommand(command: string): void {
    this.pythonCommand = command;
    logger.debug("Python command updated", { pythonCommand: command });
  }

  /**
   * Get command line configuration
   * @param script Script definition
   * @returns Command line configuration
   */
  protected getCommandLineConfig(script: Script) {
    return {
      command: this.pythonCommand,
      args: ["-c", script.content || ""],
      shell: false,
    };
  }
}
