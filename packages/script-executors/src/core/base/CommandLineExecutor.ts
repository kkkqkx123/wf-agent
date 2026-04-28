/**
 * Command Line Executor Abstract Base Class
 * Provides general command-line execution logic, encapsulating spawn calls, environment variables, working directories, output collection, etc.
 * Suitable for all types of scripts that execute commands via child processes (Shell, CMD, PowerShell, Python, etc.)
 */

import { spawn } from "child_process";
import { BaseScriptExecutor } from "./BaseScriptExecutor.js";
import type { Script, ScriptType } from "@wf-agent/types";
import type { ExecutionContext, ExecutionOutput, ExecutorConfig, ExecutorType } from "../types.js";
import { createModuleLogger } from "../../logger.js";

const logger = createModuleLogger("cmd-executor");

/**
 * Command Line Execution Configuration
 */
export interface CommandLineConfig {
  /** 命令名称（如 'sh', 'cmd.exe', 'pwsh', 'python3'） */
  command: string;
  /** Command parameters (e.g.['-c', scriptContent]) */
  args: string[];
  /** Whether to use shell mode */
  shell?: boolean;
  /** Windows-specific options */
  windowsHide?: boolean;
}

/**
 * Command Line Executor Abstract Base Class
 * All executors that execute commands via child processes should inherit from this class.
 *
 * @template T - Executor type, which must be one of ExecutorType
 */
export abstract class CommandLineExecutor<T extends ExecutorType> extends BaseScriptExecutor {
  constructor(config?: Omit<ExecutorConfig, "type"> & { type: T }) {
    super(config);
  }

  /**
   * Get command line configuration (implemented by subclass)
   * @param script script definition
   * @returns command line configuration
   */
  protected abstract getCommandLineConfig(script: Script): CommandLineConfig;

  /**
   * Specific implementation of executing the script
   * @param script: Definition of the script
   * @param context: Execution context
   * @returns: Execution output
   */
  protected async doExecute(script: Script, context?: ExecutionContext): Promise<ExecutionOutput> {
    return new Promise((resolve, reject) => {
      // Getting Script Content
      const scriptContent = script.content || "";
      if (!scriptContent) {
        logger.error("Script content is empty", { scriptName: script.name });
        reject(new Error("Script content is empty"));
        return;
      }

      // Obtain command line configuration
      const config = this.getCommandLineConfig(script);

      // Preparing Environment Variables
      const env = {
        ...process.env,
        ...script.options.environment,
        ...context?.environment,
      };

      // Preparation Work Directory
      const cwd = context?.workingDirectory || script.options.workingDirectory || process.cwd();

      logger.debug("Spawning command process", {
        scriptName: script.name,
        command: config.command,
        args: config.args,
        cwd,
      });

      // execute a command
      const child = spawn(config.command, config.args, {
        env,
        cwd,
        stdio: ["pipe", "pipe", "pipe"],
        shell: config.shell ?? false,
        windowsHide: config.windowsHide ?? false,
      });

      let stdout = "";
      let stderr = "";

      // Collect standard output
      child.stdout?.on("data", data => {
        stdout += data.toString();
      });

      // Collection of standard errors
      child.stderr?.on("data", data => {
        stderr += data.toString();
      });

      // Handling process exit
      child.on("close", code => {
        logger.debug("Command process closed", {
          scriptName: script.name,
          exitCode: code,
          stdoutLength: stdout.length,
          stderrLength: stderr.length,
        });

        resolve({
          stdout,
          stderr,
          exitCode: code ?? 1,
        });
      });

      // process error
      child.on("error", error => {
        logger.error("Command process error", {
          scriptName: script.name,
          command: config.command,
          error: error.message,
        });
        reject(error);
      });

      // Handle termination signals
      if (context?.signal) {
        const abortHandler = () => {
          logger.info("Command process aborted", {
            scriptName: script.name,
            command: config.command,
          });
          child.kill("SIGTERM");
        };
        context.signal.addEventListener("abort", abortHandler, { once: true });
      }
    });
  }

  /**
   * Get supported script types
   * @returns Array of supported script types
   */
  getSupportedTypes(): ScriptType[] {
    // The default return is the script type corresponding to the executor type.
    return [this.config.type as ScriptType];
  }

  /**
   * Getting the Actuator Type
   * @returns the executor type string
   */
  getExecutorType(): T {
    return this.config.type as T;
  }
}
