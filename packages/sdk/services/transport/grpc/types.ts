/**
 * gRPC Client Types
 */

/** gRPC client options */
export interface GrpcClientOptions {
  address: string;
  serviceName: string; // Service name from proto definition (e.g., "package.Service")
  protoPath: string; // Path to proto file
  protoIncludeDirs?: string[]; // Additional include directories for proto compilation
  useTls?: boolean;
  rootCert?: Buffer;
  defaultTimeout?: number; // Default call timeout in milliseconds
  enableHealthCheck?: boolean;
  healthCheckInterval?: number; // Health check interval in milliseconds
}

/** gRPC client connection state */
export type GrpcClientState = "disconnected" | "connecting" | "connected" | "closing" | "closed" | "error";

/** gRPC health check config */
export interface GrpcHealthCheckConfig {
  checkInterval: number; // Check interval in milliseconds
  onUnhealthy: () => void;
  onHealthy: () => void;
}
