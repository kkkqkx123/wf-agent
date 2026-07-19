/**
 * Runtime Lifecycle Module
 * Shared graceful shutdown and lifecycle management for applications.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";

/**
 * Gracefully shut down an SDK instance and associated resources.
 *
 * @param sdk The SDK instance to shut down
 * @param onBeforeShutdown Optional callback before shutdown (e.g. close servers)
 * @param timeoutMs Timeout for graceful shutdown in milliseconds
 */
export async function gracefulShutdown(
  sdk: SDKInstance,
  onBeforeShutdown?: () => Promise<void>,
  _timeoutMs: number = 15000,
): Promise<void> {
  if (onBeforeShutdown) {
    await onBeforeShutdown();
  }

  // Use SDK's destroy which triggers the onDestroy hook (closes storage)
  await sdk.destroy();
}

/**
 * Register signal handlers for graceful shutdown.
 *
 * @param shutdownFn The shutdown function to call on SIGINT/SIGTERM
 */
export function registerShutdownHandlers(shutdownFn: () => Promise<void>): void {
  process.on("SIGINT", async () => {
    await shutdownFn();
  });

  process.on("SIGTERM", async () => {
    await shutdownFn();
  });
}