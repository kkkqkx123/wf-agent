/**
 * MCP Transport Module
 * Factory and exports for transport implementations
 */

import type { IMcpTransport, TransportConfig } from "./types.js";
import { StdioTransport } from "./stdio.js";
import { SseTransport } from "./sse.js";
import { StreamableHttpTransport } from "./streamable-http.js";

export type {
  IMcpTransport,
  TransportConfig,
  TransportEventHandlers,
  TransportOptions,
} from "./types.js";
export { StdioTransport } from "./stdio.js";
export { SseTransport } from "./sse.js";
export { StreamableHttpTransport } from "./streamable-http.js";

/**
 * Create a transport instance based on configuration
 *
 * @param config - Transport configuration
 * @returns Transport instance
 */
export function createTransport(config: TransportConfig): IMcpTransport {
  switch (config.type) {
    case "stdio":
      return new StdioTransport(config);

    case "sse":
      return new SseTransport(config);

    case "streamable-http":
      return new StreamableHttpTransport(config);

    default: {
      // This should never happen due to TypeScript's exhaustiveness checking
      const _exhaustiveCheck: never = config;
      throw new Error(`Unknown transport type: ${(_exhaustiveCheck as TransportConfig).type}`);
    }
  }
}

/**
 * Check if a transport type is supported
 *
 * @param type - Transport type to check
 * @returns True if supported
 */
export function isTransportTypeSupported(type: string): boolean {
  return ["stdio", "sse", "streamable-http"].includes(type);
}
