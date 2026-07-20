/**
 * Global SDK instance holder
 *
 * Provides a single source of truth for the SDK instance, avoiding
 * circular dependencies between index.ts and adapter modules.
 *
 * index.ts imports setSDKInstance to store the instance after bootstrap.
 * Adapter modules import getSDKInstance to retrieve it lazily.
 */

import type { SDKInstance } from "@wf-agent/sdk/api";

let sdkInstance: SDKInstance | null = null;

export function getSDKInstance(): SDKInstance | null {
  return sdkInstance;
}

export function setSDKInstance(instance: SDKInstance): void {
  sdkInstance = instance;
}