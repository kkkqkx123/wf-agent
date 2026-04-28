/**
 * JavaScript 执行器
 * 使用 Node.js 执行 JavaScript 脚本
 */

import { CommandLineExecutor } from "../core/base/CommandLineExecutor.js";
import type { Script } from "@wf-agent/types";
import type { ExecutorConfig } from "../core/types.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("javascript-executor");

/**
 * JavaScript Executor
 */
export class JavaScriptExecutor extends CommandLineExecutor<"JAVASCRIPT"> {
  private nodeCommand: string;

  constructor(config?: Omit<ExecutorConfig, "type"> & { type: "JAVASCRIPT" }) {
    super({
      ...config,
      type: "JAVASCRIPT",
    });
    // The default is to use node
    this.nodeCommand = "node";
    logger.debug("JavaScript executor initialized", { nodeCommand: this.nodeCommand });
  }

  /**
   * 设置 Node.js 命令
   * @param command Node.js 命令（如 'node', 'nodejs', '/usr/bin/node'）
   */
  setNodeCommand(command: string): void {
    this.nodeCommand = command;
    logger.debug("Node command updated", { nodeCommand: command });
  }

  /**
   * Get command line configuration
   * @param script Script definition
   * @returns Command line configuration
   */
  protected getCommandLineConfig(script: Script) {
    return {
      command: this.nodeCommand,
      args: ["-e", script.content || ""],
      shell: false,
    };
  }
}
