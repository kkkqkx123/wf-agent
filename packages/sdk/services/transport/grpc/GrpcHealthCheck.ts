/**
 * gRPC Health Check
 *
 * Periodically checks gRPC service health
 */

import type { GrpcHealthCheckConfig } from "./types.js";

export class GrpcHealthCheck {
  private config: GrpcHealthCheckConfig;
  private checkFunctions: Map<string, () => Promise<boolean>> = new Map();
  private healthStates: Map<string, boolean> = new Map();
  private intervalHandles: NodeJS.Timeout[] = [];
  private running = false;

  constructor(config: GrpcHealthCheckConfig) {
    this.config = config;
  }

  /**
   * Register a service health check function
   */
  register(serviceName: string, checkFn: () => Promise<boolean>): void {
    this.checkFunctions.set(serviceName, checkFn);
    this.healthStates.set(serviceName, true);
  }

  /**
   * Start health check loop
   */
  start(): void {
    if (this.running) {
      return;
    }

    this.running = true;

    // Run initial check
    this.runChecks();

    // Schedule periodic checks
    const intervalHandle = setInterval(() => {
      this.runChecks();
    }, this.config.checkInterval);

    this.intervalHandles.push(intervalHandle);
  }

  /**
   * Stop health check loop
   */
  stop(): void {
    if (!this.running) {
      return;
    }

    this.running = false;

    for (const handle of this.intervalHandles) {
      clearInterval(handle);
    }

    this.intervalHandles = [];
  }

  /**
   * Get health status of a service
   */
  isHealthy(serviceName: string): boolean {
    return this.healthStates.get(serviceName) ?? true;
  }

  /**
   * Run all health checks
   */
  private async runChecks(): Promise<void> {
    for (const [serviceName, checkFn] of this.checkFunctions.entries()) {
      try {
        const isHealthy = await checkFn();
        const wasHealthy = this.healthStates.get(serviceName);

        if (isHealthy && !wasHealthy) {
          // Transition to healthy
          this.healthStates.set(serviceName, true);
          this.config.onHealthy();
        } else if (!isHealthy && wasHealthy) {
          // Transition to unhealthy
          this.healthStates.set(serviceName, false);
          this.config.onUnhealthy();
        }
      } catch {
        // Check failed, mark as unhealthy
        const wasHealthy = this.healthStates.get(serviceName);
        if (wasHealthy) {
          this.healthStates.set(serviceName, false);
          this.config.onUnhealthy();
        }
      }
    }
  }
}
