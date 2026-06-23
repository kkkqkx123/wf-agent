/**
 * Base Remote Executor
 *
 * Abstract base class for remote service executors (gRPC, HTTP, etc.)
 *
 * Key differences from CLI executors:
 * - Stateful connection: requires connect/disconnect management
 * - Connection reuse: multiple calls over single connection
 * - Health checks: continuous connection state monitoring
 * - Reconnection strategy: automatic recovery on disconnection
 * - Async communication: all calls are async
 */

import type { RemoteConnectionConfig, RemoteExecutorStatus, RemoteExecutionResult } from "./types.js";

export abstract class BaseRemoteExecutor {
  protected connected = false;

  /**
   * Connect to remote service
   */
  abstract connect(config: RemoteConnectionConfig): Promise<void>;

  /**
   * Disconnect from remote service
   */
  abstract disconnect(): Promise<void>;

  /**
   * Execute a remote call
   */
  abstract call<TReq, TResp>(method: string, request: TReq): Promise<TResp>;

  /**
   * Check if connected
   */
  abstract isConnected(): boolean;

  /**
   * Get executor status
   */
  abstract getStatus(): RemoteExecutorStatus;

  /**
   * Wrap a result with metrics
   */
  protected wrapResult<T>(
    success: boolean,
    data: T | undefined,
    error: Record<string, unknown> | undefined,
    callDuration: number,
    retryCount: number = 0,
  ): RemoteExecutionResult<T> {
    return {
      success,
      data,
      error: error
        ? {
            code: (error['code'] as string) || "UNKNOWN_ERROR",
            message: (error['message'] as string) || "Unknown error",
            details: error['details'] as string,
          }
        : undefined,
      metrics: {
        callDuration,
        retryCount,
      },
    };
  }
}
