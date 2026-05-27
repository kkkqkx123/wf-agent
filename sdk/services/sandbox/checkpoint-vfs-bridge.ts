/**
 * Checkpoint VFS Bridge
 *
 * Connects checkpoint lifecycle events (CHECKPOINT_CREATED, CHECKPOINT_RESTORED)
 * to CheckpointAwareVFS snapshots.
 *
 * Architecture reference: docs/infra/sandbox/strategies/vfs-overlay.md
 *
 * Usage:
 *   const bridge = new CheckpointVFSBridge(eventManager);
 *   bridge.attach(vfs, workspaceRoot);
 *   // When checkpoint events fire, VFS snapshots are automatically managed.
 */

import type { EventRegistry } from "../../core/registry/event-registry.js";
import type { CheckpointAwareVFS } from "../vfs/checkpoint-vfs.js";
import type { CheckpointCreatedEvent, CheckpointRestoredEvent, CheckpointDeletedEvent } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "CheckpointVFSBridge" });

interface VFSBinding {
  vfs: CheckpointAwareVFS;
  workspaceRoot: string;
}

export class CheckpointVFSBridge {
  private eventManager: EventRegistry;
  private bindings = new Map<string, VFSBinding>();
  private unsubscribers: Array<() => void> = [];
  private attached = false;

  constructor(eventManager: EventRegistry) {
    this.eventManager = eventManager;
  }

  /**
   * Attach a CheckpointAwareVFS to the bridge for a workspace root.
   */
  attach(vfs: CheckpointAwareVFS, workspaceRoot: string): void {
    this.bindings.set(workspaceRoot, { vfs, workspaceRoot });

    if (!this.attached) {
      this.subscribe();
      this.attached = true;
    }

    logger.debug("CheckpointAwareVFS attached to bridge", { workspaceRoot });
  }

  /**
   * Detach a CheckpointAwareVFS by workspace root.
   */
  detach(workspaceRoot: string): void {
    this.bindings.delete(workspaceRoot);

    if (this.bindings.size === 0 && this.attached) {
      this.unsubscribe();
      this.attached = false;
    }
  }

  /**
   * Remove all bindings and unsubscribe from events.
   */
  dispose(): void {
    this.bindings.clear();
    this.unsubscribe();
    this.attached = false;
  }

  private subscribe(): void {
    const unsubCreate = this.eventManager.onGlobal(
      (event) => {
        if (event.type === "CHECKPOINT_CREATED") {
          this.handleCheckpointCreated(event as CheckpointCreatedEvent).catch((err) =>
            logger.error("VFS snapshot on checkpoint create failed", {
              checkpointId: (event as CheckpointCreatedEvent).checkpointId,
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        }
      },
    );
    this.unsubscribers.push(unsubCreate);

    const unsubRestore = this.eventManager.onGlobal(
      (event) => {
        if (event.type === "CHECKPOINT_RESTORED") {
          this.handleCheckpointRestored(event as CheckpointRestoredEvent).catch((err) =>
            logger.error("VFS restore on checkpoint restore failed", {
              checkpointId: (event as CheckpointRestoredEvent).checkpointId,
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        }
      },
    );
    this.unsubscribers.push(unsubRestore);

    const unsubDelete = this.eventManager.onGlobal(
      (event) => {
        if (event.type === "CHECKPOINT_DELETED") {
          this.handleCheckpointDeleted(event as CheckpointDeletedEvent).catch((err) =>
            logger.error("VFS cleanup on checkpoint delete failed", {
              checkpointId: (event as CheckpointDeletedEvent).checkpointId,
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        }
      },
    );
    this.unsubscribers.push(unsubDelete);
  }

  private unsubscribe(): void {
    for (const unsub of this.unsubscribers) {
      unsub();
    }
    this.unsubscribers = [];
  }

  private async handleCheckpointCreated(event: CheckpointCreatedEvent): Promise<void> {
    for (const binding of this.bindings.values()) {
      await binding.vfs.onCheckpointCreate(event.checkpointId);
      logger.debug("VFS snapshot for checkpoint", {
        checkpointId: event.checkpointId,
        workspaceRoot: binding.workspaceRoot,
      });
    }
  }

  private async handleCheckpointRestored(event: CheckpointRestoredEvent): Promise<void> {
    for (const binding of this.bindings.values()) {
      await binding.vfs.onCheckpointRestore(event.checkpointId);
      logger.debug("VFS restored for checkpoint", {
        checkpointId: event.checkpointId,
        workspaceRoot: binding.workspaceRoot,
      });
    }
  }

  private async handleCheckpointDeleted(event: CheckpointDeletedEvent): Promise<void> {
    for (const binding of this.bindings.values()) {
      await binding.vfs.onCheckpointDelete(event.checkpointId);
    }
  }
}