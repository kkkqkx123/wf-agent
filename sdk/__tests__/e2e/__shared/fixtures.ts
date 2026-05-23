import { createSDK } from "../../api/index.js";
import type { SDKInstance } from "../../api/index.js";
import { createMemoryStorageBackends, initializeStorageBackends, destroyStorageBackends } from "./storage-setup.js";
import type { StorageBackends } from "./storage-setup.js";

export interface SDKFixture {
  sdk: SDKInstance;
  backends: StorageBackends;
}

export async function createSDKFixture(options?: {
  debug?: boolean;
  enableCheckpoints?: boolean;
}): Promise<SDKFixture> {
  const backends = createMemoryStorageBackends();
  await initializeStorageBackends(backends);

  const sdk = createSDK({
    debug: options?.debug ?? false,
    enableCheckpoints: options?.enableCheckpoints ?? true,
    checkpointStorageAdapter: backends.checkpoint,
    workflowStorageAdapter: backends.workflow,
    taskStorageAdapter: backends.task,
    enableValidation: false,
    presets: {
      contextCompression: { enabled: false },
      predefinedTools: { enabled: false },
      predefinedPrompts: { enabled: false },
    },
    mcp: {
      enabled: false,
    },
  });

  await sdk.waitForReady();

  return { sdk, backends };
}

export async function destroySDKFixture(fixture: SDKFixture): Promise<void> {
  await fixture.sdk.shutdown();
  await destroyStorageBackends(fixture.backends);
}