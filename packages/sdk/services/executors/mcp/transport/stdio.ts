/**
 * Stdio Transport Implementation
 * Handles MCP communication over stdio (stdin/stdout)
 */

import { spawn, ChildProcess } from "child_process";
import type { IMcpTransport, TransportEventHandlers, StdioTransportConfig } from "./types.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MCPStdioTransport" });

/**
 * Default environment variables for stdio transport
 */
function getDefaultEnvironment(): Record<string, string> {
  const env: Record<string, string> = {
    ...(process.env as Record<string, string>),
  };
  env["NODE_ENV"] = process.env["NODE_ENV"] || "production";
  return env;
}

/**
 * Stdio Transport
 * Communicates with MCP server via stdin/stdout
 */
export class StdioTransport implements IMcpTransport {
  readonly type = "stdio" as const;

  private process: ChildProcess | null = null;
  private handlers: TransportEventHandlers = {};
  private _isConnected = false;
  private config: StdioTransportConfig;

  constructor(config: StdioTransportConfig) {
    this.config = config;
  }

  get isConnected(): boolean {
    return this._isConnected && this.process !== null;
  }

  /**
   * Start the transport by spawning the MCP server process
   */
  async start(): Promise<void> {
    if (this.process) {
      return; // Already started
    }

    const env = {
      ...getDefaultEnvironment(),
      ...(this.config.env || {}),
    };

    // Handle Windows platform
    const isWindows = process.platform === "win32";
    const isAlreadyWrapped =
      this.config.command.toLowerCase() === "cmd.exe" ||
      this.config.command.toLowerCase() === "cmd";

    const command = isWindows && !isAlreadyWrapped ? "cmd.exe" : this.config.command;
    const args =
      isWindows && !isAlreadyWrapped
        ? ["/c", this.config.command, ...(this.config.args || [])]
        : this.config.args || [];

    this.process = spawn(command, args, {
      cwd: this.config.cwd || process.cwd(),
      env,
      stdio: ["pipe", "pipe", "pipe"],
    });

    // Handle process errors
    this.process.on("error", error => {
      this._isConnected = false;
      this.handlers.onError?.(error);
    });

    // Handle process exit
    this.process.on("exit", (code, signal) => {
      this._isConnected = false;
      if (code !== 0 && code !== null) {
        this.handlers.onError?.(new Error(`Process exited with code ${code}, signal ${signal}`));
      }
      this.handlers.onClose?.();
    });

    // Handle stderr for logging
    if (this.process.stderr) {
      this.process.stderr.on("data", (data: Buffer) => {
        const output = data.toString();
        const isInfoLog = /INFO/i.test(output);

        if (isInfoLog) {
          logger.info(`[MCP Server] ${output}`);
        } else {
          logger.error(`[MCP Server stderr] ${output}`);
          this.handlers.onError?.(new Error(output));
        }
      });
    }

    // Handle stdout for responses
    if (this.process.stdout) {
      this.process.stdout.on("data", (data: Buffer) => {
        try {
          const message = JSON.parse(data.toString());
          this.handlers.onData?.(message);
        } catch {
          // Non-JSON data, ignore or log
          logger.debug(`[MCP Server stdout] ${data.toString()}`);
        }
      });
    }

    this._isConnected = true;
  }

  /**
   * Close the transport
   */
  async close(): Promise<void> {
    if (!this.process) {
      return;
    }

    const isWindows = process.platform === "win32";

    return new Promise(resolve => {
      if (this.process) {
        const cleanup = () => {
          this.process = null;
          this._isConnected = false;
          resolve();
        };

        this.process.on("exit", cleanup);

        if (isWindows) {
          // On Windows, SIGTERM triggers TerminateProcess (no graceful shutdown).
          // Instead, write 'exit' to stdin and wait for graceful shutdown.
          try {
            this.process.stdin?.write("exit\n");
          } catch {
            // stdin may already be closed, fall through to force-kill
          }

          // Wait for graceful exit, then force-kill if still alive
          setTimeout(() => {
            if (this.process) {
              this.process.removeListener("exit", cleanup);
              try {
                this.process.kill();
              } catch {
                // process may already be dead
              }
              // Give kill a moment, then finalize
              setImmediate(() => {
                this.process = null;
                this._isConnected = false;
                resolve();
              });
            }
          }, 5000);
        } else {
          // Gracefully terminate the process (SIGTERM)
          this.process.kill("SIGTERM");

          // Force kill after timeout (SIGKILL)
          setTimeout(() => {
            if (this.process) {
              this.process.removeListener("exit", cleanup);
              try {
                this.process.kill("SIGKILL");
              } catch {
                // process may already be dead
              }
              setImmediate(() => {
                this.process = null;
                this._isConnected = false;
                resolve();
              });
            }
          }, 5000);
        }
      } else {
        resolve();
      }
    });
  }

  /**
   * Send a message to the MCP server
   */
  async send(message: unknown): Promise<void> {
    if (!this.process?.stdin) {
      throw new Error("Transport not connected");
    }

    const data = JSON.stringify(message) + "\n";
    return new Promise((resolve, reject) => {
      this.process!.stdin!.write(data, error => {
        if (error) {
          reject(error);
        } else {
          resolve();
        }
      });
    });
  }

  /**
   * Set event handlers
   */
  setHandlers(handlers: TransportEventHandlers): void {
    this.handlers = handlers;
  }

  /**
   * Get the stderr stream for error monitoring
   */
  getStderr(): NodeJS.ReadableStream | null {
    return this.process?.stderr || null;
  }
}
