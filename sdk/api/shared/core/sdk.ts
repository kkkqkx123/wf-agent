/**
 * SDK Main Module - Multi-Instance Architecture
 *
 * This module provides the recommended SDK API with full multi-instance support:
 * - Each SDK instance has its own DI container
 * - Each SDK instance has its own GlobalContext
 * - Each SDK instance has isolated storage adapters
 * - No global singleton state
 *
 * Architecture Principles:
 * - SDK provides the mechanism for initialization
 * - Apps provide the policy through configuration options
 * - Sensible defaults with opt-out capability for presets
 * - Full isolation between SDK instances
 */

import { SDKInstance } from "./sdk-instance.js";
import type { SDKOptions } from "../types/core-types.js";

/**
 * Create a new SDK instance with fully isolated configuration
 *
 * Each instance has:
 * - Independent DI container
 * - Independent GlobalContext
 * - Independent storage adapters
 * - Independent configuration
 * - Independent lifecycle management
 *
 * Usage Pattern:
 * ```typescript
 * // Create isolated instance for production
 * const sdk = createSDK({
 *   workflowStorageAdapter: productionAdapter,
 *   presets: { predefinedTools: { enabled: true } }
 * });
 * await sdk.waitForReady();
 *
 * // Create another isolated instance for testing
 * const testSdk = createSDK({
 *   workflowStorageAdapter: testAdapter,
 *   presets: { predefinedTools: { enabled: false } }
 * });
 * await testSdk.waitForReady();
 *
 * // Both instances are fully isolated
 * console.log(sdk === testSdk); // false
 *
 * // Shutdown when done
 * await sdk.destroy();
 * await testSdk.destroy();
 * ```
 *
 * @param options SDK configuration options
 * @returns New SDK instance
 */
export function createSDK(options: SDKOptions): SDKInstance {
  return new SDKInstance(options);
}
