/**
 * Layertwine Process Manager
 *
 * Manages the lifecycle of a Layertwine gRPC service process
 * Used in embedded deployment mode
 */

import * as childProcess from "child_process";
import { EventEmitter } from "events";

interface LayertwineProcessConfig {
  binaryPath: string;
  dbPath: string;
  grpcAddr: string;
  maxRestarts?: number;
  restartDelay?: number;
}

export class LayertwineProcessManager extends EventEmitter {
  private config: LayertwineProcessConfig;
  private process: childProcess.ChildProcess | null = null;
  private running = false;
  private restartCount = 0;
  private readonly maxRestarts: number;
  private readonly restartDelay: number;

  constructor(config: LayertwineProcessConfig) {
    super();
    this.config = config;
    this.maxRestarts = config.maxRestarts ?? 3;
    this.restartDelay = config.restartDelay ?? 1000;
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
   * Spawn the Layertwine process
   */
  private async spawn(): Promise<void> {
    try {
      const args = [
        `--db-path=${this.config.dbPath}`,
        `--grpc-addr=${this.config.grpcAddr}`,
      ];

      // Spawn with stdio ignored for output (not using pipes)
      this.process = childProcess.spawn(this.config.binaryPath, args, {
        stdio: ["ignore", "ignore", "ignore"],
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

      this.emit("ready");
    } catch (error) {
      this.running = false;
      this.emit("error", error);
      throw error;
    }
  }
}
