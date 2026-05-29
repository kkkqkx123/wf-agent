/**
 * Checkpoint Integration Test Fixtures
 *
 * Provides factory functions for creating BaseCheckpointCoordinator instances
 * with mock dependencies for integration testing.
 */

import { MemoryCheckpointStorage } from "@wf-agent/storage";

// =============================================================================
// Types
// =============================================================================

export interface CheckpointTestFixture {
  storage: MemoryCheckpointStorage;
}

// =============================================================================
// Helpers
// =============================================================================

/**
 * Create a minimal checkpoint integration fixture
 */
export async function createCheckpointFixture(): Promise<CheckpointTestFixture> {
  const storage = new MemoryCheckpointStorage();
  await storage.initialize();
  return { storage };
}
