/**
 * Message Publisher API
 *
 * Convenience API for publishing common message types.
 */

import type {
  CreateMessageInput,
  EntityIdentity,
  MessageCategory,
  MessageLevel,
} from "@wf-agent/types";
import {
  MessageCategory as MsgCategory,
  WorkflowExecutionMessageType,
  AgentMessageType,
  SubgraphMessageType,
  ToolMessageType,
  HumanRelayMessageType,
  SystemMessageType,
  CheckpointMessageType,
  EventMessageType,
} from "@wf-agent/types";
import type { MessageBus } from "./message-bus.js";

/**
 * Message Publisher API
 *
 * Provides convenience methods for publishing specific message types.
 */
export class MessagePublisher {
  constructor(private bus: MessageBus) {}

  /**
   * Publish a message
   * @param input Message input
   */
  publish(input: CreateMessageInput): void {
    this.bus.publish(input);
  }

  /**
   * Publish a Workflow Execution message
   */
  publishWorkflowExecution(
    type: WorkflowExecutionMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.WORKFLOW_EXECUTION,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish an Agent message
   */
  publishAgent(
    type: AgentMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.AGENT,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish a Subgraph message
   */
  publishSubgraph(
    type: SubgraphMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.SUBGRAPH,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish a Tool message
   */
  publishTool(
    type: ToolMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.TOOL,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish a Human Relay message
   */
  publishHumanRelay(
    type: HumanRelayMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.HUMAN_RELAY,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish a System message
   */
  publishSystem(type: SystemMessageType, data: unknown, level: MessageLevel = "info"): void {
    // System messages use a synthetic entity
    const entity: EntityIdentity = {
      type: "workflowExecution",
      id: "system",
      rootId: "system",
      depth: 0,
    };

    this.bus.publish({
      category: MsgCategory.SYSTEM,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish a Checkpoint message
   */
  publishCheckpoint(
    type: CheckpointMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.CHECKPOINT,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish an Event message
   */
  publishEvent(
    type: EventMessageType,
    entity: EntityIdentity,
    data: unknown,
    level: MessageLevel = "info",
  ): void {
    this.bus.publish({
      category: MsgCategory.EVENT,
      type,
      level,
      entity,
      data,
    });
  }

  /**
   * Publish an error message
   */
  publishError(
    category: MessageCategory,
    type: string,
    entity: EntityIdentity,
    error: Error | string,
    data?: unknown,
  ): void {
    const errorData =
      typeof error === "string"
        ? { message: error }
        : { message: error.message, stack: error.stack };

    this.bus.publish({
      category,
      type,
      level: "error",
      entity,
      data: data !== undefined ? { ...errorData, ...(data as Record<string, unknown>) } : errorData,
    });
  }
}

/**
 * Create a message publisher
 * @param bus Message bus instance
 * @returns Message publisher instance
 */
export function createMessagePublisher(bus: MessageBus): MessagePublisher {
  return new MessagePublisher(bus);
}
