/**
 * Layertwine Process Manager
 *
 * Manages the lifecycle of a Layertwine gRPC service process
 * Used in embedded deployment mode
 *
 * Layertwine gRPC mode reads configuration from environment variables:
 *   LAYERTWINE_MODE=grpc
 *   LAYERTWINE_GRPC_ADDR=<host:port>
 *   LAYERTWINE_DB_PATH=<path>
 */

import * as childProcess from "child_process";
import * as net from "net";
import { EventEmitter } from "events";

interface LayertwineProcessConfig {
  binaryPath: string;
  dbPath: string;
  grpcAddr: string;
  maxRestarts?: number;
  restartDelay?: number;
  /** Max time (ms) to wait for the gRPC port to become available */
  connectTimeout?: number;
  /** Poll interval (ms) for port readiness check */
  portPollInterval?: number;
}

export class LayertwineProcessManager extends EventEmitter {
  private config: LayertwineProcessConfig;
  private process: childProcess.ChildProcess | null = null;
  private running = false;
  private restartCount = 0;
  private readonly maxRestarts: number;
  private readonly restartDelay: number;
  private readonly connectTimeout: number;
  private readonly portPollInterval: number;

  constructor(config: LayertwineProcessConfig) {
    super();
    this.config = config;
    this.maxRestarts = config.maxRestarts ?? 3;
    this.restartDelay = config.restartDelay ?? 1000;
    this.connectTimeout = config.connectTimeout ?? 10_000;
    this.portPollInterval = config.portPollInterval ?? 200;
  }

  /**
   * Start the Layertwine process
   */
  async start(): Promise<void> {
    if (this.running) {
      return;
    }

    this.running = true;
    await this.spawn();
  }

  /**
   * Stop the Layertwine process
   */
  async stop(): Promise<void> {
    if (!this.running) {
      return;
    }

    this.running = false;

    if (this.process) {
      return new Promise(resolve => {
        if (!this.process) {
          resolve();
          return;
        }

        const timeoutHandle = setTimeout(() => {
          if (this.process) {
            this.process.kill("SIGKILL");
          }
          resolve();
        }, 5000);

        this.process.on("exit", () => {
          clearTimeout(timeoutHandle);
          resolve();
        });

        this.process.kill("SIGTERM");
      });
    }
  }

  /**
   * Check if process is running
   */
  isRunning(): boolean {
    return this.running && this.process !== null && !this.process.killed;
  }

  /**
   * Parse host and port from grpcAddr (host:port)
   */
  private parseAddr(): { host: string; port: number } {
    const [host, portStr] = this.config.grpcAddr.split(":");
    return {
      host: host || "127.0.0.1",
      port: portStr ? parseInt(portStr, 10) : 50051,
    };
  }

  /**
   * Poll the gRPC port until it is accepting connections or timeout expires
   */
  private waitForPort(): Promise<void> {
    const { host, port } = this.parseAddr();
    const deadline = Date.now() + this.connectTimeout;

    return new Promise((resolve, reject) => {
      const poll = (): void => {
        if (Date.now() > deadline) {
          reject(new Error(
            `Timed out after ${this.connectTimeout}ms waiting for gRPC server on ${host}:${port}`
          ));
          return;
        }

        const socket = new net.Socket();
        socket.setTimeout(500);

        socket.on("connect", () => {
          socket.destroy();
          resolve();
        });

        socket.on("error", () => {
          socket.destroy();
          setTimeout(poll, this.portPollInterval);
        });

        socket.on("timeout", () => {
          socket.destroy();
          setTimeout(poll, this.portPollInterval);
        });

        socket.connect(port, host);
      };

      poll();
    });
  }

  /**
   * Spawn the Layertwine process
   *
   * gRPC mode is configured via environment variables only — no CLI args.
   */
  private async spawn(): Promise<void> {
    try {
      // Layertwine gRPC mode reads config from environment variables
      const env: NodeJS.ProcessEnv = {
        ...process.env,
        LAYERTWINE_MODE: "grpc",
        LAYERTWINE_GRPC_ADDR: this.config.grpcAddr,
        LAYERTWINE_DB_PATH: this.config.dbPath,
      };

      // No CLI args — layertwine gRPC mode does not read --db-path / --grpc-addr
      this.process = childProcess.spawn(this.config.binaryPath, [], {
        stdio: ["ignore", "ignore", "ignore"],
        env,
      });

      // Handle process exit
      this.process.on("exit", (code: number | null) => {
        this.process = null;

        if (!this.running) {
          this.emit("exit", code);
          return;
        }

        // Auto-restart if needed
        if (this.restartCount < this.maxRestarts) {
          this.restartCount++;
          setTimeout(() => {
            if (this.running) {
              this.spawn();
            }
          }, this.restartDelay);
        } else {
          this.running = false;
          this.emit("maxRestartsReached");
        }
      });

      // Handle process error
      this.process.on("error", (error: Error) => {
        this.emit("error", error);
      });

      // Wait for the gRPC port to actually start listening before signalling ready
      await this.waitForPort();
      this.emit("ready");
    } catch (error) {
      this.running = false;
      this.emit("error", error);
      throw error;
    }
  }
}
