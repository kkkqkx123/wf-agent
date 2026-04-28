/**
 * Shell 执行器
 * 使用 Node.js child_process 执行 shell 脚本
 */

import { CommandLineExecutor } from "../core/base/CommandLineExecutor.js";
import type { Script } from "@wf-agent/types";
import type { ExecutorConfig } from "../core/types.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("shell-executor");

/**
 * Shell Executor
 */
export class ShellExecutor extends CommandLineExecutor<"SHELL"> {
  constructor(config?: ExecutorConfig) {
    super({
      ...config,
      type: "SHELL",
    });
    logger.debug("Shell executor initialized");
  }

  /**
   * Get command-line configuration
   * @param script Script definition
   * @returns Command-line configuration
   */
  protected getCommandLineConfig(script: Script) {
    return {
      command: "sh",
      args: ["-c", script.content || ""],
      shell: false,
    };
  }
}
