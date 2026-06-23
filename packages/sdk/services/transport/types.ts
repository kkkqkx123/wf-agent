/**
 * Transport Layer Common Types
 *
 * Unified types for all transport protocols (HTTP, gRPC, WebSocket)
 */

/** Transport protocol type */
export type TransportProtocol = "http" | "grpc" | "websocket";

/** Transport layer connection configuration */
export interface TransportConnectionConfig {
  address: string;
  protocol: TransportProtocol;
  useTls?: boolean;
  timeout?: number;
  metadata?: Record<string, string>;
}

/** Transport layer call result */
export interface TransportCallResult<TResp = unknown> {
  success: boolean;
  data?: TResp;
  error?: TransportError;
  duration: number;
}

/** Transport layer error */
export interface TransportError {
  code: string;
  message: string;
  details?: unknown;
  isRetryable: boolean;
}

/** Transport layer client interface (all transport implementations must implement) */
export interface ITransportClient {
  connect(config: TransportConnectionConfig): Promise<void>;
  disconnect(): Promise<void>;
  call<TReq, TResp>(
    method: string,
    request: TReq,
    options?: CallOptions,
  ): Promise<TResp>;
  isConnected(): boolean;
}

/** Call options */
export interface CallOptions {
  timeout?: number;
  metadata?: Record<string, string>;
}
