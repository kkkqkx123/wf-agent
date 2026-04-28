/**
 * CMD 执行器
 * 使用 Windows cmd.exe 执行批处理脚本
 */

import { CommandLineExecutor } from "../core/base/CommandLineExecutor.js";
import type { Script } from "@wf-agent/types";
import type { ExecutorConfig } from "../core/types.js";
import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("cmd-executor");

/**
 * CMD Actuator
 */
export class CmdExecutor extends CommandLineExecutor<"CMD"> {
  constructor(config?: ExecutorConfig) {
    super({
      ...config,
      type: "CMD",
    });
    logger.debug("CMD executor initialized");
  }

  /**
   * Get command line configuration
   * @param script Script definition
   * @returns Command line configuration
   */
  protected getCommandLineConfig(script: Script) {
    return {
      command: "cmd.exe",
      args: ["/c", script.content || ""],
      shell: true,
      windowsHide: true,
    };
  }
}
