/**
 * gRPC Client Manager
 *
 * Manages multiple gRPC client connections
 * Singleton instance shared across the SDK
 */

import { GrpcClient } from "./GrpcClient.js";
import type { GrpcClientOptions } from "./types.js";

export class GrpcClientManager {
  private static instance: GrpcClientManager;
  private clients: Map<string, GrpcClient> = new Map();
  private healthChangeCallbacks: Array<(serviceName: string, healthy: boolean) => void> = [];

  private constructor() {}

  /**
   * Get singleton instance
   */
  static getInstance(): GrpcClientManager {
    if (!GrpcClientManager.instance) {
      GrpcClientManager.instance = new GrpcClientManager();
    }
    return GrpcClientManager.instance;
  }

  /**
   * Get or create a client
   */
  getClient(options: GrpcClientOptions): GrpcClient {
    const key = this.getKey(options.address, options.serviceName);

    if (!this.clients.has(key)) {
      const client = new GrpcClient(options);

      // Setup health change callbacks
      client.on("healthy", () => {
        this.notifyHealthChange(options.serviceName, true);
      });
      client.on("unhealthy", () => {
        this.notifyHealthChange(options.serviceName, false);
      });

      this.clients.set(key, client);
    }

    return this.clients.get(key)!;
  }

  /**
   * Get a client for a specific service
   */
  getServiceClient(serviceName: string): GrpcClient | undefined {
    for (const [key] of this.clients.entries()) {
      if (key.includes(serviceName)) {
        return this.clients.get(key);
      }
    }
    return undefined;
  }

  /**
   * Close a specific client
   */
  async closeClient(address: string, serviceName: string): Promise<void> {
    const key = this.getKey(address, serviceName);
    const client = this.clients.get(key);

    if (client) {
      await client.close();
      this.clients.delete(key);
    }
  }

  /**
   * Shutdown all clients
   */
  async shutdown(): Promise<void> {
    const promises: Promise<void>[] = [];

    for (const client of this.clients.values()) {
      promises.push(client.close().catch(() => {
        // Ignore errors during shutdown
      }));
    }

    await Promise.all(promises);
    this.clients.clear();
  }

  /**
   * Get connection status overview
   */
  getStatus(): Map<string, "connected" | "disconnected" | "error"> {
    const status = new Map<string, "connected" | "disconnected" | "error">();

    for (const [key, client] of this.clients.entries()) {
      if (client.isConnected()) {
        status.set(key, "connected");
      } else {
        const state = client.getState();
        status.set(key, state === "error" ? "error" : "disconnected");
      }
    }

    return status;
  }

  /**
   * Register health change callback
   */
  onHealthChange(callback: (serviceName: string, healthy: boolean) => void): void {
    this.healthChangeCallbacks.push(callback);
  }

  /**
   * Get key for a client
   */
  private getKey(address: string, serviceName: string): string {
    return `${address}:${serviceName}`;
  }

  /**
   * Notify health change
   */
  private notifyHealthChange(serviceName: string, healthy: boolean): void {
    for (const callback of this.healthChangeCallbacks) {
      try {
        callback(serviceName, healthy);
      } catch {
        // Ignore callback errors
      }
    }
  }
}
