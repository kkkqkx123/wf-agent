/**
 * TriggerTemplateBuilder - Trigger Template Builder
 * Provides a seamless chain of APIs for creating and registering trigger templates
 */

import type { TriggerTemplate, TriggerCondition, TriggerAction } from "@wf-agent/types";
import { EventType, TriggerActionType } from "@wf-agent/types";
import { TemplateBuilder } from "./template-builder.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";

/**
 * TriggerTemplateBuilder - Trigger template builder
 */
export class TriggerTemplateBuilder extends TemplateBuilder<TriggerTemplate> {
  private _name: string;
  private _condition?: TriggerCondition;
  private _action?: TriggerAction;
  private _enabled?: boolean;
  private _maxTriggers?: number;

  private constructor(name: string) {
    super();
    this._name = name;
  }

  /**
   * Create a new TriggerTemplateBuilder instance
   * @param name: The name of the template
   * @returns: A TriggerTemplateBuilder instance
   */
  static create(name: string): TriggerTemplateBuilder {
    return new TriggerTemplateBuilder(name);
  }

  /**
   * Set the trigger condition
   * @param condition The trigger condition
   * @returns this
   */
  condition(condition: TriggerCondition): this {
    this._condition = condition;
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the trigger action
   * @param action The trigger action
   * @returns this
   */
  action(action: TriggerAction): this {
    this._action = action;
    this.updateTimestamp();
    return this;
  }

  /**
   * Set whether to enable or not
   * @param enabled: Whether to enable or not
   * @returns: This value
   */
  enabled(enabled: boolean): this {
    this._enabled = enabled;
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the maximum number of triggers
   * @param max The maximum number of triggers (0 indicates no limit)
   * @returns this
   */
  maxTriggers(max: number): this {
    this._maxTriggers = max;
    this.updateTimestamp();
    return this;
  }

  /**
   * Set trigger conditions based on event type (type safety)
   * @param eventType  Event type
   * @param eventName  Custom event name (only used for NODE_CUSTOM_EVENT events)
   * @param metadata  Condition metadata
   * @returns this
   */
  withEventCondition(
    eventType: EventType,
    eventName?: string,
    metadata?: Record<string, unknown>,
  ): this {
    this._condition = {
      eventType,
      ...(eventName ? { eventName } : {}),
      ...(metadata ? { metadata: metadata as import("@wf-agent/types").Metadata } : {}),
    };
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the trigger action (type-safe)
   * @param type: Type of the action
   * @param parameters: Action parameters
   * @param metadata: Action metadata
   * @returns: This
   */
  withAction(
    type: TriggerActionType,
    parameters: Record<string, unknown> = {},
    metadata?: Record<string, unknown>,
  ): this {
    this._action = {
      type,
      parameters: parameters as unknown,
      ...(metadata ? { metadata: metadata as import("@wf-agent/types").Metadata } : {}),
    } as import("@wf-agent/types").TriggerAction;
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the action to stop the thread
   * @param reason: The reason for stopping
   * @param parameters: Additional parameters
   * @returns: This
   */
  stopThread(reason?: string, parameters: Record<string, unknown> = {}): this {
    return this.withAction("stop_thread", {
      ...(reason ? { reason } : {}),
      ...parameters,
    } as Record<string, unknown>);
  }

  /**
   * Set the action to trigger the execution of a sub-workflow
   * @param triggeredWorkflowId: ID of the sub-workflow to be triggered
   * @param waitForCompletion: Whether to wait for the completion of the sub-workflow
   * @param parameters: Additional parameters to pass to the sub-workflow
   * @returns: This object representing the current state of the workflow
   */
  executeTriggeredSubgraph(
    triggeredWorkflowId: string,
    waitForCompletion: boolean = true,
    parameters: Record<string, unknown> = {},
  ): this {
    return this.withAction("execute_triggered_subgraph", {
      triggeredWorkflowId,
      waitForCompletion,
      ...parameters,
    });
  }

  /**
   * Register the template in the trigger template registry.
   * @param template: Trigger template
   */
  protected registerTemplate(template: TriggerTemplate): void {
    const container = getContainer();
    const triggerTemplateRegistry = container.get(Identifiers.TriggerTemplateRegistry) as {
      register: (template: TriggerTemplate) => void;
    };
    triggerTemplateRegistry.register(template);
  }

  /**
   * Build a trigger template
   * @returns Trigger template
   */
  build(): TriggerTemplate {
    // Verify required fields.
    if (!this._name) {
      throw new Error("Template name cannot be empty");
    }
    if (!this._condition) {
      throw new Error("Trigger condition cannot be null");
    }
    if (!this._action) {
      throw new Error("Trigger action cannot be null");
    }

    return {
      name: this._name,
      description: this._description,
      condition: this._condition,
      action: this._action,
      enabled: this._enabled !== undefined ? this._enabled : true,
      maxTriggers: this._maxTriggers,
      metadata: this._metadata,
      createdAt: this.getCreatedAt(),
      updatedAt: this.getUpdatedAt(),
    };
  }
}
