/**
 * TriggerTemplateBuilder - Trigger Template Builder
 * Provides a seamless chain of APIs for creating and registering trigger templates
 */

import type { TriggerTemplate, TriggerCondition, TriggerAction, Metadata } from "@wf-agent/types";
import { EventType, TriggerActionType } from "@wf-agent/types";
import { TemplateBuilder } from "./template-builder.js";
import type { GlobalContext } from "../../../shared/global-context.js";

/**
 * TriggerTemplateBuilder - Trigger template builder
 */
export class TriggerTemplateBuilder extends TemplateBuilder<TriggerTemplate> {
  private _name: string;
  private _condition?: TriggerCondition;
  private _action?: TriggerAction;
  private _enabled?: boolean;
  private _maxTriggers?: number;
  private globalContext: GlobalContext;

  private constructor(globalContext: GlobalContext, name: string) {
    super();
    this.globalContext = globalContext;
    this._name = name;
  }

  /**
   * Create a new TriggerTemplateBuilder instance
   * @param globalContext The GlobalContext to access services
   * @param name: The name of the template
   * @returns: A TriggerTemplateBuilder instance
   */
  static create(globalContext: GlobalContext, name: string): TriggerTemplateBuilder {
    return new TriggerTemplateBuilder(globalContext, name);
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
      ...(metadata ? { metadata: metadata as Metadata } : {}),
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
      ...(metadata ? { metadata: metadata as Metadata } : {}),
    } as TriggerAction;
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the action to stop the workflow execution
   * @param reason: The reason for stopping
   * @param parameters: Additional parameters
   * @returns: This
   */
  stopWorkflowExecution(reason?: string, parameters: Record<string, unknown> = {}): this {
    return this.withAction("stop_workflow_execution", {
      ...(reason ? { reason } : {}),
      ...parameters,
    } as Record<string, unknown>);
  }

  /**
   * Set the action to trigger the execution of a sub-workflow
   * @param triggeredWorkflowId: ID of the sub-workflow to be triggered
   * @param options: Configuration options including input/output mapping
   * @returns: This object representing the current state of the workflow
   */
  executeTriggeredSubgraph(
    triggeredWorkflowId: string,
    options: {
      waitForCompletion?: boolean;
      timeout?: number;
      recordHistory?: boolean;
      inputMapping?: {
        variables?: Record<string, string>;
        messageContexts?: Record<string, string>;
        additionalParams?: Record<string, unknown>;
      };
      outputMapping?: {
        variables?: {
          include?: string[];
          includeAll?: boolean;
          rename?: Record<string, string>;
        };
        messageContexts?: {
          include?: string[];
          includeAll?: boolean;
        };
      };
    } = {},
  ): this {
    return this.withAction("execute_triggered_subworkflow", {
      triggeredWorkflowId,
      ...options,
    });
  }

  /**
   * Register the template in the trigger template registry.
   * @param template: Trigger template
   */
  protected registerTemplate(template: TriggerTemplate): void {
    const triggerTemplateRegistry = this.globalContext.triggerTemplateRegistry;
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
