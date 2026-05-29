/**
 * Integration Test: Memory Checkpoint Storage Basic Operations
 *
 * Verifies MemoryCheckpointStorage basic CRUD operations.
 * Tests CP-INT-01 through CP-INT-03 covering:
 * - Checkpoint save and retrieval
 * - FULL checkpoint creation
 * - Storage adapter integration
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createCheckpointFixture } from "./__shared/fixtures.js";
import type { CheckpointTestFixture } from "./__shared/fixtures.js";

describe("Checkpoint Storage Integration", () => {
  let fixture: CheckpointTestFixture;

  beforeEach(async () => {
    fixture = await createCheckpointFixture();
  });

  afterEach(async () => {
    await fixture.storage.clear();
  });

  describe("Basic CRUD (CP-INT-01)", () => {
    it("should save and retrieve a checkpoint", async () => {
      const entityId = "test-entity-1";
      const data = new TextEncoder().encode(JSON.stringify({ key: "value", count: 42 }));

      // Save checkpoint (returns void)
      await fixture.storage.save("cp-1", data, {
        id: "cp-1",
        entityId,
        entityType: "AGENT_LOOP",
        type: "FULL",
        timestamp: Date.now(),
        version: 1,
        tags: [],
      });

      // Load returns Uint8Array | null
      const loadedData = await fixture.storage.load("cp-1");
      expect(loadedData).not.toBeNull();
      const decoded = JSON.parse(new TextDecoder().decode(loadedData!));
      expect(decoded.count).toBe(42);
      expect(decoded.key).toBe("value");
    });

    it("should return null for non-existent checkpoint", async () => {
      const loaded = await fixture.storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should list checkpoints with entity filter", async () => {
      const entityId = "test-entity-list";
      const encoder = new TextEncoder();
      const data1 = encoder.encode(JSON.stringify({ iteration: 1 }));
      const data2 = encoder.encode(JSON.stringify({ iteration: 2 }));

      await fixture.storage.save("cp-1", data1, {
        id: "cp-1", entityId, entityType: "AGENT_LOOP", type: "FULL", timestamp: Date.now(), version: 1, tags: [],
      });
      await fixture.storage.save("cp-2", data2, {
        id: "cp-2", entityId, entityType: "AGENT_LOOP", type: "FULL", timestamp: Date.now() + 100, version: 2, tags: [],
      });

      // Use listByEntityWithMetadata for entity-based filtering
      const items = await fixture.storage.listByEntityWithMetadata(entityId, "AGENT_LOOP");
      expect(items.length).toBe(2);
      // Sorted by timestamp descending, so first item should be cp-2 (version 2)
      expect(items[0]!.metadata.version).toBe(2);
    });
  });

  describe("FULL Checkpoint (CP-INT-02)", () => {
    it("should create and restore a FULL checkpoint", async () => {
      const entityId = "full-test-entity";
      const stateData = { messages: ["hello", "world"], counter: 5 };
      const data = new TextEncoder().encode(JSON.stringify(stateData));

      await fixture.storage.save("full-cp-1", data, {
        id: "full-cp-1",
        entityId,
        entityType: "WORKFLOW",
        type: "FULL",
        timestamp: Date.now(),
        version: 1,
        tags: [],
      });

      // Verify metadata
      const metadata = await fixture.storage.getMetadata("full-cp-1");
      expect(metadata).not.toBeNull();
      expect(metadata!.type).toBe("FULL");

      // Verify data
      const loadedData = await fixture.storage.load("full-cp-1");
      expect(loadedData).not.toBeNull();
      const decoded = JSON.parse(new TextDecoder().decode(loadedData!));
      expect(decoded.counter).toBe(5);
      expect(decoded.messages).toHaveLength(2);
    });
  });

  describe("Storage Adapter (CP-INT-03)", () => {
    it("should clear all data", async () => {
      const data = new TextEncoder().encode(JSON.stringify({}));
      await fixture.storage.save("cp-1", data, {
        id: "cp-1", entityId: "e1", entityType: "AGENT_LOOP", type: "FULL", timestamp: Date.now(), version: 1, tags: [],
      });

      await fixture.storage.clear();

      const loaded = await fixture.storage.load("cp-1");
      expect(loaded).toBeNull();
    });

    it("should report stats", async () => {
      const stats = await fixture.storage.getStats();
      expect(stats).toBeDefined();
      expect(stats.count).toBe(0);
      expect(stats.totalSize).toBe(0);
    });
  });
});
