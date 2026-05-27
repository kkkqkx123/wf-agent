/**
 * Sandbox Python Executor
 *
 * Executes Python scripts through the sandbox runtime with policy enforcement.
 * Delegates to DirectExecutor when sandbox is disabled.
 */

import type { ScriptExecutionResult } from "@wf-agent/types";
import { BaseExecutor, type BaseExecuteOptions } from "./base-executor.js";
import { DirectExecutor } from "./direct-executor.js";
import { getSandboxRuntime } from "../../../services/sandbox/sandbox-runtime.js";
import { getTerminalService, type TerminalService } from "../../../services/terminal/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "SandboxPythonExecutor" });

export class SandboxPythonExecutor extends BaseExecutor {
  private directExecutor: DirectExecutor;

  constructor(terminalService?: TerminalService) {
    super();
    this.directExecutor = new DirectExecutor(terminalService || getTerminalService());
  }

  async execute(options: BaseExecuteOptions): Promise<ScriptExecutionResult> {
    const startTime = Date.now();

    const runtime = getSandboxRuntime();
    const config = options.sandboxConfig;

    if (!runtime.isEnabled(config)) {
      logger.debug("Sandbox disabled, delegating to DirectExecutor");
      return this.directExecutor.execute(options);
    }

    const result = await runtime.createRuntime(
      "python",
      {
        command: options.command,
        cwd: options.cwd,
        env: options.env,
        timeout: options.timeout ?? config?.policy?.resource?.timeoutLimit,
      },
      config,
    );

    if (!result.strategy) {
      logger.debug("No sandbox strategy available, delegating to DirectExecutor");
      return this.directExecutor.execute(options);
    }

    try {
      const executionResult = await result.strategy.execute(
        {
          command: options.command,
          cwd: options.cwd,
          env: options.env,
          timeout: options.timeout,
        },
        result.policy,
      );

      return {
        ...executionResult,
        scriptName: options.sandboxConfig?.profile
          ? `sandbox-python:${options.sandboxConfig.profile}`
          : "sandbox-python",
      };
    } catch (error) {
      return {
        success: false,
        scriptName: "sandbox-python",
        executionTime: Date.now() - startTime,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async cleanup(): Promise<void> {
    await this.directExecutor.cleanup();
  }
}