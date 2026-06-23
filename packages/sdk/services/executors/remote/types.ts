/**
 * Remote Service Executor Types
 *
 * Types for remote service executors (gRPC, HTTP, etc.)
 */

/** Remote connection configuration */
export interface RemoteConnectionConfig {
  address: string; // Service address (host:port)
  useTls?: boolean;
  timeout?: number;
  reconnectPolicy?: {
    maxRetries: number;
    baseDelay: number;
    maxDelay: number;
  };
}

/** Remote executor status */
export type RemoteExecutorStatus = "disconnected" | "connecting" | "connected" | "unhealthy" | "error";

/** Remote execution result */
export interface RemoteExecutionResult<TResp = unknown> {
  success: boolean;
  data?: TResp;
  error?: {
    code: string;
    message: string;
    details?: unknown;
  };
  metrics: {
    callDuration: number; // Single call duration in milliseconds
    retryCount: number;
  };
}

/** Remote executor configuration */
export interface RemoteExecutorConfig {
  name: string;
  description?: string;
}
