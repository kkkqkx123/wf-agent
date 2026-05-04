/**
 * Agent Loop Entity Serializer
 *
 * Handles serialization and deserialization of complete Agent Loop entities.
 */

import { Serializer } from "../serializer.js";
import { SerializationRegistry } from "../serialization-registry.js";
import type { SnapshotBase, AgentLoopRuntimeConfig, Message } from "@wf-agent/types";
import type { AgentLoopStateSnapshot } from "@wf-agent/types";
import type { AgentLoopEntity } from "../../../agent/entities/agent-loop-entity.js";

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
      variables: entity.getAllVariables(),
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

    // Create a new entity with the same config
    const entity = await AgentLoopFactory.create(snapshot.config);

    // Restore messages
    entity.setMessages(snapshot.messages);
    
    // Restore variables
    for (const [key, value] of Object.entries(snapshot.variables)) {
      entity.setVariable(key, value);
    }

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
      variables: entity.getAllVariables(),
      config: entity.config,
      iterationHistory: [], // TODO: Add iteration history tracking to AgentLoopEntity
      isStreaming: false, // Not persisted, runtime only
      pendingToolCalls: [], // Not persisted, runtime only
    };
  }
}

/**
 * Register Agent Loop Entity serializer with the global registry
 */
export function registerAgentLoopEntitySerializer(): void {
  const registry = SerializationRegistry.getInstance();

  // Note: Entity snapshots don't use delta calculation, so we pass undefined
  // The registry type should allow null/undefined for deltaCalculator
  registry.register({
    entityType: "agentLoop",
    serializer: new AgentLoopEntitySerializer(),
    deltaCalculator: undefined as any, // Entity snapshots don't use delta calculation
  });
}
