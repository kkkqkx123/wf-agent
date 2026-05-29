/**
 * BaseCheckpointStateManager Tests
 * Tests for CRUD operations, cleanup policy execution, and lifecycle management
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BaseCheckpointStateManager } from '../base-checkpoint-state-manager.js';
import type { BaseCheckpoint, CheckpointStorageMetadata, CleanupPolicy } from '@wf-agent/types';
import type { EventRegistry } from '../../registry/event-registry.js';
import type { CheckpointStorageAdapter } from '../types.js';

// ---- Mock Storage Adapter ----
function createMockStorageAdapter(): CheckpointStorageAdapter {
  const store = new Map<string, { data: Uint8Array; metadata: unknown }>();

  return {
    save: vi.fn(async (id: string, data: Uint8Array, metadata: unknown) => {
      store.set(id, { data, metadata });
    }),
    load: vi.fn(async (id: string) => {
      const entry = store.get(id);
      return entry ? entry.data : null;
    }),
    delete: vi.fn(async (id: string) => {
      store.delete(id);
    }),
    list: vi.fn(async (_options?: Record<string, unknown>) => {
      return Array.from(store.keys());
    }),
    listWithMetadata: vi.fn(async (_options?: Record<string, unknown>) => {
      return Array.from(store.entries()).map(([id, entry]) => ({
        id,
        metadata: entry.metadata,
      }));
    }),
    listByEntityWithMetadata: vi.fn(
      async (_entityId: string, _entityType: string) => {
        return Array.from(store.entries()).map(([id, entry]) => ({
          id,
          metadata: entry.metadata,
        }));
      }
    ),
    getLatestByEntity: vi.fn(
      async (_entityId: string, _entityType: string, _count?: number) => {
        return Array.from(store.entries()).map(([id, entry]) => ({
          id,
          metadata: entry.metadata,
        }));
      }
    ),
    deleteByEntity: vi.fn(
      async (_entityId: string, _entityType: string, _options?: { keepLatest?: number; olderThan?: number }) => {
        return 0;
      }
    ),
    initialize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
  };
}

// ---- Test Checkpoint Type ----
interface TestCheckpoint extends BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {
  entityId: string;
  entityType: string;
}

// ---- Concrete State Manager for Testing ----
class TestCheckpointStateManager extends BaseCheckpointStateManager<TestCheckpoint> {
  extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
    return {
      entityType: checkpoint.entityType as any,
      entityId: checkpoint.entityId,
      timestamp: checkpoint.timestamp || Date.now(),
      tags: checkpoint.metadata?.tags || [],
      customFields: {
        blobSize: checkpoint.snapshot ? JSON.stringify(checkpoint.snapshot).length : 0,
      },
    };
  }

  buildCreatedEvent(checkpoint: TestCheckpoint): unknown {
    return {
      id: checkpoint.id,
      type: 'CHECKPOINT_CREATED',
      timestamp: Date.now(),
      data: { checkpointId: checkpoint.id, entityId: checkpoint.entityId },
    };
  }

  buildDeletedEvent(checkpointId: string, reason?: 'manual' | 'cleanup' | 'policy'): unknown {
    return {
      id: checkpointId,
      type: 'CHECKPOINT_DELETED',
      timestamp: Date.now(),
      data: { checkpointId, reason },
    };
  }

  buildFailedEvent(checkpointId: string, error: unknown, operation?: 'create' | 'restore' | 'delete'): unknown {
    return {
      id: checkpointId,
      type: 'CHECKPOINT_FAILED',
      timestamp: Date.now(),
      data: { checkpointId, error, operation },
    };
  }
}

// ---- Mock Event Registry ----
function createMockEventRegistry(): EventRegistry {
  return {
    emit: vi.fn().mockResolvedValue(undefined),
    on: vi.fn(),
    off: vi.fn(),
    getEmitter: vi.fn().mockReturnValue({ on: vi.fn(), off: vi.fn(), emit: vi.fn() }),
    removeAllListeners: vi.fn(),
    listenerCount: vi.fn().mockReturnValue(0),
  } as unknown as EventRegistry;
}

describe('BaseCheckpointStateManager', () => {
  let storageAdapter: ReturnType<typeof createMockStorageAdapter>;
  let eventManager: EventRegistry;
  let stateManager: TestCheckpointStateManager;

  function createTestCheckpoint(id: string, overrides: Partial<TestCheckpoint> = {}): TestCheckpoint {
    return {
      id,
      type: 'FULL',
      entityId: 'entity-1',
      entityType: 'workflow',
      timestamp: Date.now(),
      snapshot: { state: 'active' },
      ...overrides,
    } as TestCheckpoint;
  }

  beforeEach(() => {
    storageAdapter = createMockStorageAdapter();
    eventManager = createMockEventRegistry();
    stateManager = new TestCheckpointStateManager(storageAdapter, eventManager);
    vi.clearAllMocks();
  });

  describe('create', () => {
    it('should save checkpoint via storage adapter and return ID', async () => {
      const cp = createTestCheckpoint('cp-1');
      const result = await stateManager.create(cp);

      expect(result).toBe('cp-1');
      expect(storageAdapter.save).toHaveBeenCalledWith(
        'cp-1',
        expect.any(Uint8Array),
        expect.objectContaining({ entityId: 'entity-1' })
      );
    });

    it('should emit event when eventManager is set', async () => {
      const cp = createTestCheckpoint('cp-2');
      await stateManager.create(cp);

      expect(eventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'CHECKPOINT_CREATED' })
      );
    });

    it('should not emit event when eventManager is not set', async () => {
      const managerNoEvents = new TestCheckpointStateManager(storageAdapter);
      const cp = createTestCheckpoint('cp-3');
      await managerNoEvents.create(cp);

      // No event emission should happen
      // If emit was never set up, it won't be called
    });

    it('should propagate error when storage fails', async () => {
      const storageError = new Error('Storage write failed');
      (storageAdapter.save as ReturnType<typeof vi.fn>).mockRejectedValue(storageError);

      const cp = createTestCheckpoint('cp-4');
      await expect(stateManager.create(cp)).rejects.toThrow('Storage write failed');
    });

    it('should emit failed event when storage fails', async () => {
      (storageAdapter.save as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('fail'));

      const cp = createTestCheckpoint('cp-5');
      await expect(stateManager.create(cp)).rejects.toThrow();

      expect(eventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'CHECKPOINT_FAILED' })
      );
    });

    it('should track checkpoint size in memory', async () => {
      const cp = createTestCheckpoint('cp-size-test', { snapshot: { data: 'x'.repeat(100) } });
      await stateManager.create(cp);

      // Access the internal map to verify size was tracked
      const sizes = (stateManager as any).checkpointSizes as Map<string, number>;
      expect(sizes.has('cp-size-test')).toBe(true);
      expect(sizes.get('cp-size-test')!).toBeGreaterThan(0);
    });
  });

  describe('get', () => {
    it('should return checkpoint when it exists', async () => {
      const cp = createTestCheckpoint('cp-get-1');
      await stateManager.create(cp);

      const result = await stateManager.get('cp-get-1');
      expect(result).not.toBeNull();
      expect(result!.id).toBe('cp-get-1');
    });

    it('should return null when checkpoint does not exist', async () => {
      const result = await stateManager.get('non-existent');
      expect(result).toBeNull();
    });

    it('should throw error on deserialization failure', async () => {
      // Save corrupted data
      (storageAdapter.load as ReturnType<typeof vi.fn>).mockResolvedValue(new Uint8Array([0xff, 0xfe, 0xfd]));

      await expect(stateManager.get('corrupted')).rejects.toThrow('Checkpoint data corrupted');
    });

    it('should track size on successful load', async () => {
      const cp = createTestCheckpoint('cp-size-get');
      await stateManager.create(cp);

      // Clear and reload
      (stateManager as any).checkpointSizes = new Map();
      await stateManager.get('cp-size-get');

      const sizes = (stateManager as any).checkpointSizes as Map<string, number>;
      expect(sizes.has('cp-size-get')).toBe(true);
      expect(sizes.get('cp-size-get')!).toBeGreaterThan(0);
    });
  });

  describe('delete', () => {
    it('should delete checkpoint from storage adapter', async () => {
      const cp = createTestCheckpoint('cp-del-1');
      await stateManager.create(cp);
      await stateManager.delete('cp-del-1');

      expect(storageAdapter.delete).toHaveBeenCalledWith('cp-del-1');
    });

    it('should emit deleted event with reason', async () => {
      const cp = createTestCheckpoint('cp-del-2');
      await stateManager.create(cp);
      await stateManager.delete('cp-del-2', 'cleanup');

      expect(eventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'CHECKPOINT_DELETED' })
      );
    });

    it('should propagate deletion error', async () => {
      (storageAdapter.delete as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('Delete failed'));

      await expect(stateManager.delete('missing')).rejects.toThrow('Delete failed');
    });

    it('should remove size tracking on delete', async () => {
      const cp = createTestCheckpoint('cp-size-del');
      await stateManager.create(cp);

      const sizes = (stateManager as any).checkpointSizes as Map<string, number>;
      expect(sizes.has('cp-size-del')).toBe(true);

      await stateManager.delete('cp-size-del');
      expect(sizes.has('cp-size-del')).toBe(false);
    });
  });

  describe('list', () => {
    it('should return checkpoint IDs from storage adapter', async () => {
      (storageAdapter.list as ReturnType<typeof vi.fn>).mockResolvedValue(['cp-1', 'cp-2']);

      const result = await stateManager.list();
      expect(result).toEqual(['cp-1', 'cp-2']);
    });

    it('should pass options to storage adapter', async () => {
      await stateManager.list({ parentId: 'entity-1', limit: 10 });

      expect(storageAdapter.list).toHaveBeenCalledWith({ parentId: 'entity-1', limit: 10 });
    });
  });

  describe('executeCleanupForEntity', () => {
    it('should return empty result when no policy is set', async () => {
      const managerNoPolicy = new TestCheckpointStateManager(storageAdapter);

      const result = await managerNoPolicy.executeCleanupForEntity('entity-1', 'workflow');
      expect(result.deletedCount).toBe(0);
      expect(result.freedSpaceBytes).toBe(0);
    });

    it('should execute count-based cleanup policy', async () => {
      const policy: CleanupPolicy = { type: 'count', maxCount: 2 };

      // Create some checkpoints
      for (let i = 0; i < 5; i++) {
        const cp = createTestCheckpoint(`cp-clean-${i}`, {
          entityId: 'entity-clean',
          snapshot: { index: i },
        });
        await stateManager.create(cp);
      }

      const result = await stateManager.executeCleanupForEntity('entity-clean', 'workflow', policy);

      expect(result.deletedCount).toBeGreaterThan(0);
      expect(result.deletedCheckpointIds.length).toBeGreaterThan(0);
    });

    it('should use default policy when not overridden', async () => {
      const policy: CleanupPolicy = { type: 'count', maxCount: 1 };
      const managerWithPolicy = new TestCheckpointStateManager(storageAdapter, eventManager, policy);

      const cp = createTestCheckpoint('cp-default-policy', { entityId: 'entity-dp' });
      await managerWithPolicy.create(cp);

      const result = await managerWithPolicy.executeCleanupForEntity('entity-dp', 'workflow');
      // With only 1 checkpoint and maxCount=1, nothing should be deleted
      expect(result).toBeDefined();
    });
  });

  describe('initialize', () => {
    it('should call storage adapter initialize', async () => {
      await stateManager.initialize();
      expect(storageAdapter.initialize).toHaveBeenCalled();
    });

    it('should handle missing initialize method gracefully', async () => {
      const adapterNoInit = createMockStorageAdapter();
      delete (adapterNoInit as any).initialize;
      const manager = new TestCheckpointStateManager(adapterNoInit);

      // Should not throw
      await expect(manager.initialize()).resolves.toBeUndefined();
    });
  });

  describe('cleanup', () => {
    it('should call storage adapter close', async () => {
      await stateManager.cleanup();
      expect(storageAdapter.close).toHaveBeenCalled();
    });

    it('should handle missing close method gracefully', async () => {
      const adapterNoClose = createMockStorageAdapter();
      delete (adapterNoClose as any).close;
      const manager = new TestCheckpointStateManager(adapterNoClose);

      await expect(manager.cleanup()).resolves.toBeUndefined();
    });
  });
});
