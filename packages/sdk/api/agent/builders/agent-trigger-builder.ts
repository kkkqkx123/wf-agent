/**
 * AgentTriggerBuilder - Fluent builder for Agent trigger configuration
 *
 * Provides a declarative API for building AgentTriggerStatic objects.
 * Triggers control agent execution flow (pause, stop, checkpoint, etc.).
 */

import type { AgentTriggerStatic, AgentTriggerAction } from "@wf-agent/types";

/**
 * AgentTriggerBuilder - Build agent trigger configuration with fluent API
 */
export class AgentTriggerBuilder {
  private _id?: string;
  private _name?: string;
  private _type?: "event" | "condition" | "schedule";
  private _condition?: string;
  private _eventName?: string;
  private _enabled: boolean = true;
  private _action?: AgentTriggerAction;

  /**
   * Create a new AgentTriggerBuilder instance
   * @param name Trigger name
   * @returns AgentTriggerBuilder instance
   */
  static create(name: string): AgentTriggerBuilder {
    const builder = new AgentTriggerBuilder();
    builder._name = name;
    builder._id = name; // Default id to name
    return builder;
  }

  /**
   * Create a builder for condition-based trigger
   * @param name Trigger name
   * @returns AgentTriggerBuilder instance
   */
  static onCondition(name: string): AgentTriggerBuilder {
    const builder = AgentTriggerBuilder.create(name);
    builder._type = "condition";
    return builder;
  }

  /**
   * Create a builder for event-based trigger
   * @param name Trigger name
   * @returns AgentTriggerBuilder instance
   */
  static onEvent(name: string): AgentTriggerBuilder {
    const builder = AgentTriggerBuilder.create(name);
    builder._type = "event";
    return builder;
  }

  /**
   * Create a builder for schedule-based trigger
   * @param name Trigger name
   * @returns AgentTriggerBuilder instance
   */
  static onSchedule(name: string): AgentTriggerBuilder {
    const builder = AgentTriggerBuilder.create(name);
    builder._type = "schedule";
    return builder;
  }

  /**
   * Set the trigger ID
   * @param id Trigger ID
   * @returns this for chaining
   */
  id(id: string): this {
    this._id = id;
    return this;
  }

  /**
   * Set the trigger type
   * @param type Trigger type (event, condition, or schedule)
   * @returns this for chaining
   */
  type(type: "event" | "condition" | "schedule"): this {
    this._type = type;
    return this;
  }

  /**
   * Set the trigger condition expression
   * @param condition Condition expression string
   * @returns this for chaining
   */
  condition(condition: string): this {
    this._condition = condition;
    return this;
  }

  /**
   * Set the event name (for event-based triggers)
   * @param eventName Event name to listen for
   * @returns this for chaining
   */
  eventName(eventName: string): this {
    this._eventName = eventName;
    return this;
  }

  /**
   * Enable the trigger
   * @returns this for chaining
   */
  enable(): this {
    this._enabled = true;
    return this;
  }

  /**
   * Disable the trigger
   * @returns this for chaining
   */
  disable(): this {
    this._enabled = false;
    return this;
  }

  /**
   * Set enabled state
   * @param enabled Whether trigger is enabled
   * @returns this for chaining
   */
  enabled(enabled: boolean): this {
    this._enabled = enabled;
    return this;
  }

  /**
   * Set action to pause execution
   * @param config Optional action configuration
   * @returns this for chaining
   */
  pause(config?: Record<string, unknown>): this {
    this._action = {
      type: "pause",
      config,
    };
    return this;
  }

  /**
   * Set action to stop execution
   * @param config Optional action configuration
   * @returns this for chaining
   */
  stop(config?: Record<string, unknown>): this {
    this._action = {
      type: "stop",
      config,
    };
    return this;
  }

  /**
   * Set action to create checkpoint
   * @param config Optional action configuration
   * @returns this for chaining
   */
  checkpoint(config?: Record<string, unknown>): this {
    this._action = {
      type: "checkpoint",
      config,
    };
    return this;
  }

  /**
   * Set a custom action
   * @param actionType Custom action type
   * @param config Action configuration
   * @returns this for chaining
   */
  customAction(actionType: string, config?: Record<string, unknown>): this {
    this._action = {
      type: "custom",
      config: { ...config, actionType },
    };
    return this;
  }

  /**
   * Set the trigger action
   * @param action Trigger action object
   * @returns this for chaining
   */
  action(action: AgentTriggerAction): this {
    this._action = action;
    return this;
  }

  /**
   * Build the agent trigger configuration
   * @returns AgentTriggerStatic
   */
  build(): AgentTriggerStatic {
    if (!this._name) {
      throw new Error("Trigger name is required");
    }

    if (!this._type) {
      throw new Error("Trigger type is required (event, condition, or schedule)");
    }

    if (!this._action) {
      throw new Error("Trigger action is required (pause, stop, checkpoint, or custom)");
    }

    const trigger: AgentTriggerStatic = {
      id: this._id || this._name,
      type: this._type,
      enabled: this._enabled,
      action: this._action,
    };

    if (this._condition) {
      trigger.condition = this._condition;
    }

    if (this._eventName) {
      trigger.eventName = this._eventName;
    }

    return trigger;
  }

  /**
   * Clone this builder (for creating variations)
   * @returns New builder with same configuration
   */
  clone(): AgentTriggerBuilder {
    const cloned = new AgentTriggerBuilder();
    cloned._id = this._id;
    cloned._name = this._name;
    cloned._type = this._type;
    cloned._condition = this._condition;
    cloned._eventName = this._eventName;
    cloned._enabled = this._enabled;
    cloned._action = this._action ? { ...this._action } : undefined;
    return cloned;
  }
}
