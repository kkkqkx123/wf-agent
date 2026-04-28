/**
 * Base Executor Class
 *
 * Provides common functionality for external binary executors.
 */

import * as childProcess from "child_process";
import * as readline from "readline";
import * as fs from "fs/promises";
import type {
  ExecutorConfig,
  ExecutorInfo,
  ExecutorStatus,
  ExecutionOptions,
  ExecutionResult,
} from "./types.js";

/**
 * Platform detection
 */
const isWindows = process.platform.startsWith("win");

/**
 * Check if a file exists
 */
async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

/**
 * Find binary in PATH
 */
async function findInPath(binaryName: string): Promise<string | undefined> {
  try {
    const whichCommand = isWindows ? "where" : "which";
    const result = await new Promise<string>((resolve, reject) => {
      childProcess.exec(`${whichCommand} ${binaryName}`, (error, stdout) => {
        if (error) {
          reject(error);
        } else {
          resolve(stdout.trim().split("\n")[0] ?? "");
        }
      });
    });
    if (result && await fileExists(result)) {
      return result;
    }
  } catch {
    // Not in PATH
  }
  return undefined;
}

/**
 * Base Executor class
 */
export abstract class BaseExecutor {
  protected config: ExecutorConfig;
  protected binaryPath: string | undefined;
  protected initialized = false;

  constructor(config: ExecutorConfig) {
    this.config = config;
  }

  /**
   * Get default search paths for the binary
   */
  protected abstract getDefaultPaths(): string[];

  /**
   * Initialize the executor by finding the binary
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    // Try custom path first
    if (this.config.customPath && await fileExists(this.config.customPath)) {
      this.binaryPath = this.config.customPath;
      this.initialized = true;
      return;
    }

    // Try to find in PATH
    const pathResult = await findInPath(this.config.binaryName);
    if (pathResult) {
      this.binaryPath = pathResult;
      this.initialized = true;
      return;
    }

    // Try default paths
    const defaultPaths = this.getDefaultPaths();
    for (const testPath of defaultPaths) {
      if (await fileExists(testPath)) {
        this.binaryPath = testPath;
        this.initialized = true;
        return;
      }
    }

    // Try additional paths from config
    if (this.config.additionalPaths) {
      for (const testPath of this.config.additionalPaths) {
        if (await fileExists(testPath)) {
          this.binaryPath = testPath;
          this.initialized = true;
          return;
        }
      }
    }

    throw new Error(
      `Could not find ${this.config.name} binary. Please install it or provide a custom path.`
    );
  }

  /**
   * Ensure the executor is initialized
   */
  protected async ensureInitialized(): Promise<string> {
    if (!this.initialized) {
      await this.initialize();
    }

    if (!this.binaryPath) {
      throw new Error(`${this.config.name} binary not found`);
    }

    return this.binaryPath;
  }

  /**
   * Get executor info
   */
  async getInfo(): Promise<ExecutorInfo> {
    try {
      const binaryPath = await this.ensureInitialized();
      return {
        name: this.config.name,
        binaryPath,
        status: "available",
      };
    } catch (error) {
      return {
        name: this.config.name,
        binaryPath: "",
        status: "unavailable",
      };
    }
  }

  /**
   * Execute the binary with given options
   */
  async execute(options: ExecutionOptions): Promise<ExecutionResult> {
    const bin = await this.ensureInitialized();

    return new Promise((resolve) => {
      const proc = childProcess.spawn(bin, options.args, {
        cwd: options.cwd,
        env: options.env ? { ...process.env, ...options.env } : process.env,
      });

      let stdout = "";
      let stderr = "";
      let lineCount = 0;
      const maxLines = options.maxLines ?? Infinity;

      // Handle stdout with line limiting
      const rl = readline.createInterface({
        input: proc.stdout,
        crlfDelay: Infinity,
      });

      rl.on("line", (line) => {
        if (lineCount < maxLines) {
          stdout += line + "\n";
          lineCount++;
        } else {
          rl.close();
          proc.kill();
        }
      });

      // Handle stderr
      proc.stderr.on("data", (data) => {
        stderr += data.toString();
      });

      // Handle completion
      rl.on("close", () => {
        const exitCode = proc.exitCode ?? 0;
        resolve({
          stdout,
          stderr,
          exitCode,
          success: exitCode === 0,
        });
      });

      proc.on("error", (error) => {
        resolve({
          stdout: "",
          stderr: error.message,
          exitCode: 1,
          success: false,
        });
      });

      // Handle timeout
      if (options.timeout) {
        setTimeout(() => {
          proc.kill();
          resolve({
            stdout,
            stderr: "Execution timeout",
            exitCode: 1,
            success: false,
          });
        }, options.timeout);
      }
    });
  }

  /**
   * Check if the executor is available
   */
  async isAvailable(): Promise<boolean> {
    try {
      await this.ensureInitialized();
      return true;
    } catch {
      return false;
    }
  }
}
