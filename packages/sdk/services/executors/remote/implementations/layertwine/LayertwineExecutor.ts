/**
 * Layertwine gRPC Executor
 *
 * Encapsulates all communication details with the Layertwine gRPC service
 * Supports embedded (SDK auto-starts binary) and remote (pre-deployed) modes
 */

import { BaseRemoteExecutor } from "../../BaseRemoteExecutor.js";
import type { RemoteConnectionConfig, RemoteExecutorStatus } from "../../types.js";
import { GrpcClient } from "../../../../transport/grpc/GrpcClient.js";
import { LayertwineProcessManager } from "./layertwine-process.js";
import { createContextualLogger } from "../../../../../utils/contextual-logger.js";
import type {
  LayertwineInitRequest,
  LayertwineInitResponse,
  LayertwineEditRequest,
  LayertwineEditResponse,
  LayertwineStatusResponse,
  LayertwineCommitRequest,
  LayertwineCommitResponse,
  LayertwineLogRequest,
  LayertwineLogResponse,
  LayertwineBranchCreateRequest,
  LayertwineBranchCreateResponse,
  LayertwineBranchSwitchRequest,
  LayertwineBranchSwitchResponse,
  LayertwineBranchListResponse,
  LayertwineAgentEditRequest,
  LayertwineAgentEditResponse,
  LayertwineAgentSubmitRequest,
  LayertwineAgentSubmitResponse,
  LayertwineApproveRequest,
  LayertwineApproveResponse,
  LayertwineBackupRequest,
  LayertwineBackupResponse,
  LayertwineRestoreRequest,
  LayertwineRestoreResponse,
  LayertwineSelectiveRestoreRequest,
  LayertwineSelectiveRestoreResponse,
  LayertwineRestoreByTimeRequest,
  LayertwineRestoreByTimeResponse,
  LayertwineDiffRequest,
  LayertwineDiffResponse,
  LayertwineGetSnapshotRequest,
  LayertwineGetSnapshotResponse,
} from "./types.js";

const logger = createContextualLogger({ component: "LayertwineExecutor" });

export type LayertwineDeployMode = "embedded" | "remote";

export interface LayertwineExecutorConfig {
  deployMode: LayertwineDeployMode;
  // Remote mode
  address?: string;
  // Embedded mode
  binaryPath?: string;
  dbPath?: string;
  protoPath?: string;
}

/**
 * Layertwine gRPC Executor
 */
export class LayertwineExecutor extends BaseRemoteExecutor {
  private grpcClient: GrpcClient | null = null;
  private config: LayertwineExecutorConfig;
  private mode: LayertwineDeployMode;
  private processManager: LayertwineProcessManager | null = null;
  private lastConnectionConfig: RemoteConnectionConfig | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelayMs = 1000;

  constructor(config: LayertwineExecutorConfig) {
    super();
    this.config = config;
    this.mode = config.deployMode;

    // Validate configuration early to catch errors at instantiation time
    this.validateConfig();
  }

  /**
   * Validate configuration is complete and valid
   * Throws error if required fields are missing
   */
  private validateConfig(): void {
    if (!this.config.protoPath) {
      throw new Error(
        "LayertwineExecutor configuration error: protoPath is required for all deployment modes"
      );
    }

    if (this.mode === "embedded") {
      if (!this.config.binaryPath) {
        throw new Error(
          "LayertwineExecutor configuration error: binaryPath is required for embedded mode"
        );
      }
      if (!this.config.dbPath) {
        throw new Error(
          "LayertwineExecutor configuration error: dbPath is required for embedded mode"
        );
      }
    }
  }

  /**
   * Connect to Layertwine service
   */
  async connect(connectionConfig: RemoteConnectionConfig): Promise<void> {
    if (this.connected) {
      return;
    }

    // Store connection config for potential reconnection attempts
    this.lastConnectionConfig = connectionConfig;

    try {
      // For embedded mode: start the Layertwine process
      if (this.mode === "embedded") {
        await this.startEmbeddedProcess(connectionConfig);
      }

      // Create gRPC client and connect
      this.grpcClient = new GrpcClient({
        address: connectionConfig.address,
        serviceName: "layertwine.Layertwine",
        protoPath: this.config.protoPath!,
        useTls: connectionConfig.useTls,
        defaultTimeout: connectionConfig.timeout,
        enableHealthCheck: true,
        healthCheckInterval: 5000,
      });

      await this.grpcClient.connect();
      this.connected = true;
      this.reconnectAttempts = 0;

      logger.info("Successfully connected to Layertwine", {
        mode: this.mode,
        address: connectionConfig.address,
        timestamp: new Date().toISOString(),
      });
    } catch (error) {
      this.connected = false;

      // Clean up resources on connection failure
      if (this.processManager) {
        try {
          await this.processManager.stop();
        } catch (stopError) {
          logger.warn("Error stopping process during cleanup", {
            error: stopError instanceof Error ? stopError.message : String(stopError),
          });
        }
      }

      const errorMsg = error instanceof Error ? error.message : String(error);
      logger.error("Failed to connect to Layertwine", {
        mode: this.mode,
        address: connectionConfig.address,
        error: errorMsg,
        stack: error instanceof Error ? error.stack : undefined,
      });

      throw new Error(`Layertwine connection failed: ${errorMsg}`);
    }
  }

  /**
   * Start the embedded Layertwine process with timeout and error handling
   */
  private async startEmbeddedProcess(connectionConfig: RemoteConnectionConfig): Promise<void> {
    return new Promise((resolve, reject) => {
      if (!this.config.binaryPath || !this.config.dbPath) {
        reject(
          new Error(
            "Embedded mode requires binaryPath and dbPath (should have been validated in constructor)"
          )
        );
        return;
      }

      this.processManager = new LayertwineProcessManager({
        binaryPath: this.config.binaryPath,
        dbPath: this.config.dbPath,
        grpcAddr: connectionConfig.address,
      });

      // Set up timeout for process startup (10 seconds)
      const timeout = setTimeout(() => {
        reject(
          new Error(
            "Layertwine process startup timeout (exceeded 10 seconds). Check process logs and ensure sufficient system resources."
          )
        );
      }, 10000);

      const cleanup = () => clearTimeout(timeout);

      this.processManager.once("ready", () => {
        cleanup();
        logger.debug("Layertwine process started successfully");
        resolve();
      });

      this.processManager.once("error", (error) => {
        cleanup();
        const errorMsg = error instanceof Error ? error.message : String(error);
        logger.error("Layertwine process failed to start", { error: errorMsg });
        reject(new Error(`Layertwine process startup failed: ${errorMsg}`));
      });

      try {
        this.processManager.start();
      } catch (error) {
        cleanup();
        const errorMsg = error instanceof Error ? error.message : String(error);
        reject(new Error(`Failed to start Layertwine process: ${errorMsg}`));
      }
    });
  }

  /**
   * Disconnect from Layertwine service
   */
  async disconnect(): Promise<void> {
    if (!this.connected) {
      return;
    }

    try {
      logger.debug("Disconnecting from Layertwine");

      if (this.grpcClient) {
        try {
          await this.grpcClient.close();
        } catch (error) {
          logger.warn("Error closing gRPC client", {
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }

      if (this.processManager) {
        try {
          await this.processManager.stop();
        } catch (error) {
          logger.warn("Error stopping process manager", {
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    } finally {
      this.grpcClient = null;
      this.processManager = null;
      this.connected = false;
      logger.debug("Disconnected from Layertwine");
    }
  }

  /**
   * Generic RPC call with automatic reconnection and detailed logging
   */
  async call<TReq, TResp>(method: string, request: TReq): Promise<TResp> {
    // Ensure we have an active connection, attempt reconnection if needed
    if (!this.grpcClient?.isConnected()) {
      logger.warn("Connection lost, attempting to reconnect", {
        method,
        reconnectAttempts: this.reconnectAttempts,
      });
      await this.ensureConnected();
    }

    if (!this.grpcClient?.isConnected()) {
      throw new Error(
        "Layertwine executor not connected and reconnection failed. Please call connect() first."
      );
    }

    const startTime = performance.now();
    const requestSize = JSON.stringify(request).length;

    try {
      logger.debug(`Layertwine RPC call: ${method}`, {
        method,
        requestSize,
        timestamp: new Date().toISOString(),
      });

      const response = await this.grpcClient.call<TReq, TResp>(method, request);

      const duration = performance.now() - startTime;
      const responseSize = JSON.stringify(response).length;

      logger.debug(`Layertwine RPC completed: ${method}`, {
        method,
        duration: Math.round(duration),
        requestSize,
        responseSize,
        timestamp: new Date().toISOString(),
      });

      return response;
    } catch (error) {
      const duration = performance.now() - startTime;
      const errorMsg = error instanceof Error ? error.message : String(error);

      logger.error(`Layertwine RPC failed: ${method}`, {
        method,
        duration: Math.round(duration),
        error: errorMsg,
        stack: error instanceof Error ? error.stack : undefined,
        requestSize,
      });

      // If it's a connection error, trigger reconnection on next call
      if (
        errorMsg.includes("connection") ||
        errorMsg.includes("closed") ||
        errorMsg.includes("timeout")
      ) {
        this.connected = false;
      }

      throw error;
    }
  }

  /**
   * Ensure connection is active, attempt reconnection if needed
   */
  private async ensureConnected(): Promise<void> {
    if (this.connected && this.grpcClient?.isConnected()) {
      return;
    }

    if (!this.lastConnectionConfig) {
      throw new Error(
        "No previous connection configuration available. Please call connect() first."
      );
    }

    const maxAttempts = this.maxReconnectAttempts;
    let lastError: Error | null = null;

    for (let i = 0; i < maxAttempts; i++) {
      try {
        this.reconnectAttempts = i + 1;

        logger.warn("Reconnection attempt", {
          attempt: this.reconnectAttempts,
          maxAttempts,
          delayMs: this.reconnectDelayMs * Math.pow(2, i),
        });

        // Disconnect first if there's an existing connection
        if (this.connected || this.grpcClient) {
          try {
            await this.disconnect();
          } catch (disconnectError) {
            logger.debug("Error during disconnect cleanup", {
              error:
                disconnectError instanceof Error
                  ? disconnectError.message
                  : String(disconnectError),
            });
          }
        }

        // Attempt to reconnect
        await this.connect(this.lastConnectionConfig);

        this.reconnectAttempts = 0;
        logger.info("Reconnection successful");
        return;
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        const delay = this.reconnectDelayMs * Math.pow(2, i);
        const errorMsg = lastError.message;

        if (i < maxAttempts - 1) {
          logger.warn(`Reconnection attempt ${i + 1}/${maxAttempts} failed`, {
            error: errorMsg,
            nextRetryIn: delay,
          });

          // Wait before retrying with exponential backoff
          await new Promise((resolve) => setTimeout(resolve, delay));
        } else {
          logger.error(`All ${maxAttempts} reconnection attempts failed`, {
            error: errorMsg,
          });
        }
      }
    }

    throw new Error(
      `Failed to reconnect to Layertwine after ${maxAttempts} attempts. Last error: ${lastError?.message || "Unknown error"}`
    );
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connected && this.grpcClient?.isConnected() === true;
  }

  /**
   * Get executor status
   */
  getStatus(): RemoteExecutorStatus {
    if (!this.grpcClient) {
      return "disconnected";
    }
    if (!this.grpcClient.isConnected()) {
      return "disconnected";
    }
    return "connected";
  }

  /**
   * Get executor type
   */
  getExecutorType(): string {
    return "layertwine-grpc";
  }

  // ══════════════════════════════════════════════════════════════
  // Layertwine Convenience Methods
  // ══════════════════════════════════════════════════════════════

  /**
   * Initialize repository
   */
  async init(request: LayertwineInitRequest): Promise<LayertwineInitResponse> {
    return this.call<LayertwineInitRequest, LayertwineInitResponse>("Init", request);
  }

  /**
   * Edit a file
   */
  async edit(request: LayertwineEditRequest): Promise<LayertwineEditResponse> {
    return this.call<LayertwineEditRequest, LayertwineEditResponse>("Edit", request);
  }

  /**
   * Get repository status
   */
  async status(): Promise<LayertwineStatusResponse> {
    return this.call<Record<string, never>, LayertwineStatusResponse>("Status", {});
  }

  /**
   * Create a checkpoint
   */
  async commit(request: LayertwineCommitRequest): Promise<LayertwineCommitResponse> {
    return this.call<LayertwineCommitRequest, LayertwineCommitResponse>("Commit", request);
  }

  /**
   * Query history
   */
  async log(request: LayertwineLogRequest): Promise<LayertwineLogResponse> {
    return this.call<LayertwineLogRequest, LayertwineLogResponse>("Log", request);
  }

  /**
   * List branches
   */
  async branchList(): Promise<LayertwineBranchListResponse> {
    return this.call<Record<string, never>, LayertwineBranchListResponse>("BranchList", {});
  }

  /**
   * Create a new branch
   */
  async branchCreate(request: LayertwineBranchCreateRequest): Promise<LayertwineBranchCreateResponse> {
    return this.call<LayertwineBranchCreateRequest, LayertwineBranchCreateResponse>(
      "BranchCreate",
      request
    );
  }

  /**
   * Switch to a branch
   */
  async branchSwitch(request: LayertwineBranchSwitchRequest): Promise<LayertwineBranchSwitchResponse> {
    return this.call<LayertwineBranchSwitchRequest, LayertwineBranchSwitchResponse>(
      "BranchSwitch",
      request
    );
  }

  /**
   * Agent edit
   */
  async agentEdit(request: LayertwineAgentEditRequest): Promise<LayertwineAgentEditResponse> {
    return this.call<LayertwineAgentEditRequest, LayertwineAgentEditResponse>("AgentEdit", request);
  }

  /**
   * Agent submit
   */
  async agentSubmit(request: LayertwineAgentSubmitRequest): Promise<LayertwineAgentSubmitResponse> {
    return this.call<LayertwineAgentSubmitRequest, LayertwineAgentSubmitResponse>("AgentSubmit", request);
  }

  /**
   * Approve changes
   */
  async approve(request: LayertwineApproveRequest): Promise<LayertwineApproveResponse> {
    return this.call<LayertwineApproveRequest, LayertwineApproveResponse>("Approve", request);
  }

  /**
   * Backup repository
   */
  async backup(request: LayertwineBackupRequest): Promise<LayertwineBackupResponse> {
    return this.call<LayertwineBackupRequest, LayertwineBackupResponse>("Backup", request);
  }

  /**
   * Restore full checkpoint
   */
  async restoreCheckpoint(request: LayertwineRestoreRequest): Promise<LayertwineRestoreResponse> {
    return this.call<LayertwineRestoreRequest, LayertwineRestoreResponse>("RestoreCheckpoint", request);
  }

  /**
   * Restore checkpoint selectively by source pattern
   */
  async restoreSelectiveCheckpoint(
    request: LayertwineSelectiveRestoreRequest
  ): Promise<LayertwineSelectiveRestoreResponse> {
    return this.call<LayertwineSelectiveRestoreRequest, LayertwineSelectiveRestoreResponse>(
      "RestoreSelectiveCheckpoint",
      request
    );
  }

  /**
   * Restore checkpoint at a specific timestamp
   */
  async restoreCheckpointByTime(
    request: LayertwineRestoreByTimeRequest
  ): Promise<LayertwineRestoreByTimeResponse> {
    return this.call<LayertwineRestoreByTimeRequest, LayertwineRestoreByTimeResponse>(
      "RestoreCheckpointByTime",
      request
    );
  }

  /**
   * Diff two checkpoints
   */
  async diffCheckpoints(request: LayertwineDiffRequest): Promise<LayertwineDiffResponse> {
    return this.call<LayertwineDiffRequest, LayertwineDiffResponse>("DiffCheckpoints", request);
  }

  /**
   * Get snapshot content
   */
  async getSnapshot(request: LayertwineGetSnapshotRequest): Promise<LayertwineGetSnapshotResponse> {
    return this.call<LayertwineGetSnapshotRequest, LayertwineGetSnapshotResponse>("GetSnapshot", request);
  }
}
