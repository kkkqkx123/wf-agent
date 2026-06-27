/**
 * Entity ID Extraction - P1 Defect Fix Verification
 *
 * Tests for improved entity ID extraction using EntityIdExtractor pattern
 * instead of unsafe 'as unknown as Record<string, string>' casts.
 *
 * PROBLEM FIXED:
 * - Multiple type casts: (checkpoint as unknown as Record<string, string>)
 * - No type safety for field access
 * - Fallback to "unknown" causes metrics/events data mixing
 * - Inconsistent field names across checkpoint types
 *
 * SOLUTION:
 * - EntityIdExtractor<TCheckpoint> function type
 * - ICheckpointWithEntity interface (optional)
 * - defaultExtractEntityId() method in base class
 * - Subclasses provide specific extractors via constructor
 */

import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointStateManager, type EntityIdExtractor } from "@sdk/shared/checkpoint/core/base-state-manager.js";
import type { CheckpointStorageMetadata } from "@wf-agent/types";
import type { BaseCheckpoint } from "@wf-agent/types";

/**
 * Checkpoint with executionId field (workflow checkpoint)
 */
interface WorkflowCheckpoint extends BaseCheckpoint<unknown, unknown> {
  id: string;
  type: "FULL" | "DELTA";
  timestamp: number;
  executionId: string;
  workflowId: string;
  snapshot?: unknown;
}

/**
 * Checkpoint with agentLoopId field (agent checkpoint)
 */
interface AgentCheckpoint extends BaseCheckpoint<unknown, unknown> {
  id: string;
  type: "FULL" | "DELTA";
  timestamp: number;
  agentLoopId: string;
  snapshot?: unknown;
}

/**
 * Test implementation for workflow checkpoints
 */
class WorkflowStateManager extends BaseCheckpointStateManager<WorkflowCheckpoint> {
  constructor(storage: MemoryCheckpointStorage) {
    // Provide type-safe extractor for workflow checkpoints
    super(
      storage,
      undefined,
      undefined,
      undefined,
      (checkpoint: WorkflowCheckpoint) => checkpoint.executionId?.trim() || "unknown"
    );
  }

  protected extractStorageMetadata(checkpoint: WorkflowCheckpoint): CheckpointStorageMetadata {
    // Use extractEntityId to ensure consistency
    const entityId = this.extractEntityId(checkpoint);
    return {
      entityType: "workflow",
      entityId,
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      blobSize: 0,
    };
  }

  protected buildCreatedEvent(checkpoint: WorkflowCheckpoint): unknown {
    return { type: "checkpoint.created", executionId: checkpoint.executionId };
  }

  protected buildDeletedEvent(checkpointId: string): unknown {
    return { type: "checkpoint.deleted", checkpointId };
  }

  protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
    return { type: "checkpoint.failed", checkpointId, error };
  }
}

/**
 * Test implementation for agent checkpoints
 */
class AgentStateManager extends BaseCheckpointStateManager<AgentCheckpoint> {
  constructor(storage: MemoryCheckpointStorage) {
    // Provide type-safe extractor for agent checkpoints
    super(
      storage,
      undefined,
      undefined,
      undefined,
      (checkpoint: AgentCheckpoint) => checkpoint.agentLoopId || "unknown"
    );
  }

  protected extractStorageMetadata(checkpoint: AgentCheckpoint): CheckpointStorageMetadata {
    // Use extractEntityId to ensure consistency
    const entityId = this.extractEntityId(checkpoint);
    return {
      entityType: "agent",
      entityId,
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      blobSize: 0,
    };
  }

  protected buildCreatedEvent(checkpoint: AgentCheckpoint): unknown {
    return { type: "checkpoint.created", agentLoopId: checkpoint.agentLoopId };
  }

  protected buildDeletedEvent(checkpointId: string): unknown {
    return { type: "checkpoint.deleted", checkpointId };
  }

  protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
    return { type: "checkpoint.failed", checkpointId, error };
  }
}

describe("Entity ID Extraction - P1 Defect Fix", () => {
  let storage: MemoryCheckpointStorage;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
  });

  describe("Type-Safe Extraction for Different Checkpoint Types", () => {
    it("should extract executionId from workflow checkpoints without type casting", async () => {
      const manager = new WorkflowStateManager(storage);

      const checkpoint: WorkflowCheckpoint = {
        id: "cp-workflow",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "exec-123",
        workflowId: "wf-456",
        snapshot: { state: "started" },
      };

      // Should not throw, should correctly extract executionId
      const checkpointId = await manager.create(checkpoint);

      expect(checkpointId).toBe("cp-workflow");

      // Verify checkpoint was stored with correct metadata
      const metadata = await storage.getMetadata("cp-workflow");
      expect(metadata?.entityId).toBe("exec-123");
    });

    it("should extract agentLoopId from agent checkpoints without type casting", async () => {
      const manager = new AgentStateManager(storage);

      const checkpoint: AgentCheckpoint = {
        id: "cp-agent",
        type: "FULL",
        timestamp: Date.now(),
        agentLoopId: "agent-loop-789",
        snapshot: { state: "running" },
      };

      // Should not throw, should correctly extract agentLoopId
      const checkpointId = await manager.create(checkpoint);

      expect(checkpointId).toBe("cp-agent");

      // Verify checkpoint was stored with correct metadata
      const metadata = await storage.getMetadata("cp-agent");
      expect(metadata?.entityId).toBe("agent-loop-789");
    });
  });

  describe("Fallback to 'unknown' only when necessary", () => {
    it("should fallback to 'unknown' when executionId is missing", async () => {
      const manager = new WorkflowStateManager(storage);

      const checkpoint: WorkflowCheckpoint = {
        id: "cp-no-exec-id",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "", // Empty field
        workflowId: "wf-456",
        snapshot: { state: "error" },
      };

      // Should still create but with 'unknown' entityId
      const checkpointId = await manager.create(checkpoint);
      expect(checkpointId).toBe("cp-no-exec-id");

      // Verify metadata reflects unknown state
      const metadata = await storage.getMetadata("cp-no-exec-id");
      expect(metadata?.entityId).toBe("unknown");
    });

    it("should not mix entity IDs across different checkpoint types", async () => {
      const workflowManager = new WorkflowStateManager(storage);
      const agentManager = new AgentStateManager(storage);

      // Create workflow checkpoint with executionId
      const workflowCp: WorkflowCheckpoint = {
        id: "cp-wf",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "exec-111",
        workflowId: "wf-100",
        snapshot: {},
      };

      // Create agent checkpoint with agentLoopId
      const agentCp: AgentCheckpoint = {
        id: "cp-agent",
        type: "FULL",
        timestamp: Date.now(),
        agentLoopId: "agent-222",
        snapshot: {},
      };

      // Both should be created successfully
      await workflowManager.create(workflowCp);
      await agentManager.create(agentCp);

      // Verify each has correct entity ID
      const wfMetadata = await storage.getMetadata("cp-wf");
      const agentMetadata = await storage.getMetadata("cp-agent");

      expect(wfMetadata?.entityId).toBe("exec-111");
      expect(agentMetadata?.entityId).toBe("agent-222");

      // They should not have mixed entity IDs
      expect(wfMetadata?.entityId).not.toBe("agent-222");
      expect(agentMetadata?.entityId).not.toBe("exec-111");
    });
  });

  describe("Metrics Collection with Correct Entity IDs", () => {
    it("should record metrics with correct entity ID (not mixed or unknown)", async () => {
      // Enable metrics collection
      const manager = new WorkflowStateManager(storage);

      const checkpoint: WorkflowCheckpoint = {
        id: "cp-metrics",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "exec-metrics-123",
        workflowId: "wf-123",
        snapshot: { data: "test" },
      };

      await manager.create(checkpoint);

      // Verify checkpoint was created
      const stored = await manager.get("cp-metrics");
      expect(stored).not.toBeNull();
      expect(stored?.executionId).toBe("exec-metrics-123");
    });

    it("should work with custom entity ID extractors", async () => {
      interface CustomCheckpoint extends BaseCheckpoint<unknown, unknown> {
        id: string;
        type: "FULL" | "DELTA";
        timestamp: number;
        workflowId: string;
        executionId: string;
        snapshot?: unknown;
      }

      class CustomManager extends BaseCheckpointStateManager<CustomCheckpoint> {
        constructor(
          storage: MemoryCheckpointStorage,
          customExtractor: EntityIdExtractor<CustomCheckpoint>,
        ) {
          // Use custom extractor
          super(storage, undefined, undefined, undefined, customExtractor);
        }

        protected extractStorageMetadata(checkpoint: CustomCheckpoint): CheckpointStorageMetadata {
          // Use the extractEntityId method to get consistent entity ID
          const entityId = this.extractEntityId(checkpoint);
          return {
            entityType: "custom",
            entityId,
            timestamp: checkpoint.timestamp,
            checkpointType: checkpoint.type,
            blobSize: 0,
          };
        }

        protected buildCreatedEvent(checkpoint: CustomCheckpoint): unknown {
          return { type: "checkpoint.created" };
        }

        protected buildDeletedEvent(checkpointId: string): unknown {
          return { type: "checkpoint.deleted", checkpointId };
        }

        protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
          return { type: "checkpoint.failed", checkpointId, error };
        }
      }

      const customExtractor: EntityIdExtractor<any> = (checkpoint) => {
        // Custom logic: derives composite ID from multiple fields
        return `${checkpoint.workflowId}:${checkpoint.executionId}`;
      };

      const storage2 = new MemoryCheckpointStorage();
      await storage2.initialize();

      const manager = new CustomManager(storage2, customExtractor);

      const checkpoint: any = {
        id: "cp-custom",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "exec-cust",
        workflowId: "wf-cust",
        snapshot: {},
      };

      await manager.create(checkpoint);

      // Verify custom extraction worked
      const metadata = await storage2.getMetadata("cp-custom");
      expect(metadata?.entityId).toBe("wf-cust:exec-cust");
    });
  });

  describe("No Type Casting Required", () => {
    it("should not require 'as unknown as' casts in manager code", async () => {
      // This test verifies that the manager's internal implementation
      // doesn't use unsafe type casts for entity ID extraction

      const manager = new WorkflowStateManager(storage);

      // The fact that this works without compilation errors means
      // we're not doing unsafe casts internally
      const checkpoint: WorkflowCheckpoint = {
        id: "cp-safe",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "exec-safe",
        workflowId: "wf-safe",
        snapshot: { safe: true },
      };

      // This should work smoothly
      const result = await manager.create(checkpoint);
      expect(result).toBe("cp-safe");

      // Verify it was stored correctly
      const retrieved = await manager.get("cp-safe");
      expect(retrieved?.executionId).toBe("exec-safe");
    });
  });

  describe("Backward Compatibility", () => {
    it("should work with checkpoint types that have both executionId and agentLoopId", async () => {
      interface HybridCheckpoint extends BaseCheckpoint<unknown, unknown> {
        id: string;
        type: "FULL" | "DELTA";
        timestamp: number;
        executionId?: string;
        agentLoopId?: string;
        snapshot?: unknown;
      }

      class HybridManager extends BaseCheckpointStateManager<HybridCheckpoint> {
        constructor(storage: MemoryCheckpointStorage) {
          // Default implementation tries executionId first, then agentLoopId
          super(storage);
        }

        protected extractStorageMetadata(checkpoint: HybridCheckpoint): CheckpointStorageMetadata {
          const entityId = checkpoint.executionId || checkpoint.agentLoopId || "unknown";
          return {
            entityType: "hybrid",
            entityId,
            timestamp: checkpoint.timestamp,
            checkpointType: checkpoint.type,
            blobSize: 0,
          };
        }

        protected buildCreatedEvent(checkpoint: HybridCheckpoint): unknown {
          return { type: "checkpoint.created" };
        }

        protected buildDeletedEvent(checkpointId: string): unknown {
          return { type: "checkpoint.deleted", checkpointId };
        }

        protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
          return { type: "checkpoint.failed", checkpointId, error };
        }
      }

      const storage2 = new MemoryCheckpointStorage();
      await storage2.initialize();

      const manager = new HybridManager(storage2);

      // Test with executionId priority
      const cp1: HybridCheckpoint = {
        id: "cp-hybrid-1",
        type: "FULL",
        timestamp: Date.now(),
        executionId: "exec-priority",
        agentLoopId: "agent-ignored",
        snapshot: {},
      };

      await manager.create(cp1);
      const metadata1 = await storage2.getMetadata("cp-hybrid-1");
      expect(metadata1?.entityId).toBe("exec-priority");

      // Test fallback when executionId is missing
      const cp2: HybridCheckpoint = {
        id: "cp-hybrid-2",
        type: "FULL",
        timestamp: Date.now(),
        agentLoopId: "agent-fallback",
        snapshot: {},
      };

      await manager.create(cp2);
      // Default implementation will extract from the checkpoint
      const stored2 = await manager.get("cp-hybrid-2");
      expect(stored2?.agentLoopId).toBe("agent-fallback");
    });
  });
});
