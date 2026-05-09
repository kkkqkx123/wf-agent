/**
 * Agent Loop Entity Serializer
 *
 * Handles serialization and deserialization of complete Agent Loop entities.
 */

import { Serializer } from "../serializer.js";
import { SerializationRegistry } from "../serialization-registry.js";
import { DeltaCalculator } from "../delta-calculator.js";
import type { SnapshotBase, AgentLoopRuntimeConfig, Message } from "@wf-agent/types";
import type { AgentLoopStateSnapshot } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "agent-loop-entity-serializer" });

/**
 * Agent Loop Entity Snapshot
 *
 * Represents a complete snapshot of an Agent Loop entity for persistence.
 */
export interface AgentLoopEntitySnapshot extends SnapshotBase {
  _entityType: "agentLoop";
  /** Agent Loop ID */
  id: string;
  /** Agent Loop configuration */
  config: AgentLoopRuntimeConfig;
  /** State snapshot */
  state: AgentLoopStateSnapshot;
  /** Messages */
  messages: Message[];
  /** Variables */
  variables: Record<string, unknown>;
  /** Parent execution context (unified hierarchy) */
  parentContext?: {
    parentType: 'WORKFLOW' | 'AGENT_LOOP';
    parentId: string;
    nodeId?: string;
    delegationPurpose?: string;
  };
}

/**
 * Agent Loop Entity Serializer
 */
export class AgentLoopEntitySerializer extends Serializer<AgentLoopEntitySnapshot> {
  constructor() {
    super({ prettyPrint: true, targetVersion: 1 });
  }

  /**
   * Serialize an Agent Loop entity to Uint8Array
   */
  async serializeEntity(entity: AgentLoopEntity): Promise<Uint8Array> {
    const parentContext = entity.getParentContext();
    const snapshot: AgentLoopEntitySnapshot = {
      _version: 1,
      _timestamp: Date.now(),
      _entityType: "agentLoop",
      id: entity.id,
      config: entity.config,
      state: this.createStateSnapshot(entity),
      messages: entity.getMessages(),
      // Note: AgentLoop doesn't manage variables (that's a Workflow feature)
      variables: {},
      parentContext: parentContext ? {
        parentType: parentContext.parentType,
        parentId: parentContext.parentId,
        ...(parentContext.parentType === 'WORKFLOW' && { nodeId: parentContext.nodeId }),
        ...(parentContext.parentType === 'AGENT_LOOP' && { delegationPurpose: parentContext.delegationPurpose }),
      } : undefined,
    };
    return this.serialize(snapshot);
  }

  /**
   * Deserialize to an Agent Loop entity
   * 
   * Note: Full entity restoration requires implementing a proper factory method.
   * This is a simplified version that creates a new entity and restores basic state.
   */
  async deserializeEntity(data: Uint8Array): Promise<AgentLoopEntity> {
    const snapshot = await this.deserialize(data);
    if (snapshot._entityType !== "agentLoop") {
      throw new Error(`Expected agentLoop, got ${snapshot._entityType}`);
    }

    // Import dynamically to avoid circular dependencies
    const { AgentLoopFactory } = await import("../../../agent/execution/factories/index.js");
    
    // Get GlobalContext from container manager
    let globalContext: import("../../global-context.js").GlobalContext | undefined;
    try {
      const { ContainerManager } = await import("../../di/container-manager.js");
      const Identifiers = await import("../../di/service-identifiers.js");
      const manager = ContainerManager.getInstance();
      const containerIds = manager.getAllContainerIds();
      if (containerIds.length > 0) {
        const container = manager.getContainer(containerIds[0]!);
        globalContext = container.get(Identifiers.GlobalContext);
      }
    } catch (error) {
      logger.warn('Failed to get GlobalContext for AgentLoopFactory', { error });
    }

    if (!globalContext) {
      throw new Error('GlobalContext not available for AgentLoop entity restoration');
    }

    // Create a new entity with the same config
    const entity = await AgentLoopFactory.create(globalContext, snapshot.config);

    // Restore messages
    entity.setMessages(snapshot.messages);
    
    // Note: AgentLoop doesn't manage variables (that's a Workflow feature)
    // Variables field is preserved for compatibility but not restored

    // TODO: Restore additional state properties when AgentLoopState supports it
    // For now, the entity starts with default state values
    // Future enhancement: Add restoreFromSnapshot method to AgentLoopState

    return entity;
  }

  /**
   * Create state snapshot from entity
   */
  private createStateSnapshot(entity: AgentLoopEntity): AgentLoopStateSnapshot {
    return {
      status: entity.state.status,
      currentIteration: entity.state.currentIteration,
      toolCallCount: entity.state.toolCallCount,
      startTime: entity.state.startTime,
      endTime: entity.state.endTime,
      error: entity.state.error,
      messages: entity.getMessages(),
      // Note: AgentLoop doesn't manage variables (that's a Workflow feature)
      variables: {},
      config: entity.config,
      iterationHistory: entity.state.iterationHistory,
      // Note: isStreaming, pendingToolCalls, and streamMessage are runtime-only fields
      // and are not included in the snapshot
    };
  }
}

/**
 * Register Agent Loop Entity serializer with the global registry
 */
export function registerAgentLoopEntitySerializer(): void {
  const registry = SerializationRegistry.getInstance();

  // Note: Entity snapshots don't use delta calculation, so we provide a no-op calculator
  const noOpCalculator = new DeltaCalculator({ deepCompare: false });
  
  registry.register({
    entityType: "agentLoop",
    serializer: new AgentLoopEntitySerializer(),
    deltaCalculator: noOpCalculator,
  });
}
