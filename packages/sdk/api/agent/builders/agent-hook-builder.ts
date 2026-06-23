/**
 * AgentHookBuilder - Fluent builder for Agent hook configuration
 *
 * Provides a declarative API for building AgentHookStatic objects.
 * Used for file-based agent configuration hooks.
 */

import type { AgentHookStatic } from "@wf-agent/types";
import type { AgentHookType } from "@wf-agent/types";

/**
 * AgentHookBuilder - Build agent hook configuration with fluent API
 */
export class AgentHookBuilder {
  private _hookType?: AgentHookType;
  private _eventName?: string;
  private _condition?: string;
  private _eventPayload: Record<string, unknown> = {};
  private _enabled: boolean = true;
  private _weight?: number;
  private _createCheckpoint: boolean = false;
  private _checkpointDescription?: string;

  /**
   * Create a builder for BEFORE_ITERATION hook
   * @returns AgentHookBuilder instance
   */
  static beforeIteration(): AgentHookBuilder {
    const builder = new AgentHookBuilder();
    builder._hookType = "BEFORE_ITERATION";
    return builder;
  }

  /**
   * Create a builder for AFTER_ITERATION hook
   * @returns AgentHookBuilder instance
   */
  static afterIteration(): AgentHookBuilder {
    const builder = new AgentHookBuilder();
    builder._hookType = "AFTER_ITERATION";
    return builder;
  }

  /**
   * Create a builder for BEFORE_TOOL_CALL hook
   * @returns AgentHookBuilder instance
   */
  static beforeToolCall(): AgentHookBuilder {
    const builder = new AgentHookBuilder();
    builder._hookType = "BEFORE_TOOL_CALL";
    return builder;
  }

  /**
   * Create a builder for AFTER_TOOL_CALL hook
   * @returns AgentHookBuilder instance
   */
  static afterToolCall(): AgentHookBuilder {
    const builder = new AgentHookBuilder();
    builder._hookType = "AFTER_TOOL_CALL";
    return builder;
  }

  /**
   * Create a builder for BEFORE_LLM_CALL hook
   * @returns AgentHookBuilder instance
   */
  static beforeLlmCall(): AgentHookBuilder {
    const builder = new AgentHookBuilder();
    builder._hookType = "BEFORE_LLM_CALL";
    return builder;
  }

  /**
   * Create a builder for AFTER_LLM_CALL hook
   * @returns AgentHookBuilder instance
   */
  static afterLlmCall(): AgentHookBuilder {
    const builder = new AgentHookBuilder();
    builder._hookType = "AFTER_LLM_CALL";
    return builder;
  }

  /**
   * Set the hook type
   * @param hookType Hook type
   * @returns this for chaining
   */
  hookType(hookType: AgentHookType): this {
    this._hookType = hookType;
    return this;
  }

  /**
   * Set the event name
   * @param eventName Event name for identification and logging
   * @returns this for chaining
   */
  eventName(eventName: string): this {
    this._eventName = eventName;
    return this;
  }

  /**
   * Set the trigger condition expression
   * @param condition Condition expression (string format for serialization)
   * @returns this for chaining
   */
  condition(condition: string): this {
    this._condition = condition;
    return this;
  }

  /**
   * Add event payload data
   * @param payload Event payload object
   * @returns this for chaining
   */
  eventPayload(payload: Record<string, unknown>): this {
    this._eventPayload = { ...this._eventPayload, ...payload };
    return this;
  }

  /**
   * Enable the hook
   * @returns this for chaining
   */
  enable(): this {
    this._enabled = true;
    return this;
  }

  /**
   * Disable the hook
   * @returns this for chaining
   */
  disable(): this {
    this._enabled = false;
    return this;
  }

  /**
   * Set enabled state
   * @param enabled Whether hook is enabled
   * @returns this for chaining
   */
  enabled(enabled: boolean): this {
    this._enabled = enabled;
    return this;
  }

  /**
   * Set priority weight (higher = earlier execution)
   * @param weight Priority weight
   * @returns this for chaining
   */
  weight(weight: number): this {
    this._weight = weight;
    return this;
  }

  /**
   * Enable checkpoint creation when hook is triggered
   * @returns this for chaining
   */
  createCheckpoint(): this {
    this._createCheckpoint = true;
    return this;
  }

  /**
   * Set checkpoint description
   * @param description Description for the checkpoint
   * @returns this for chaining
   */
  checkpointDescription(description: string): this {
    this._checkpointDescription = description;
    this._createCheckpoint = true;
    return this;
  }

  /**
   * Build the agent hook configuration
   * @returns AgentHookStatic
   */
  build(): AgentHookStatic {
    if (!this._hookType) {
      throw new Error("Hook type is required");
    }

    if (!this._eventName) {
      throw new Error("Event name is required");
    }

    const hook: AgentHookStatic = {
      hookType: this._hookType,
      eventName: this._eventName,
      enabled: this._enabled,
    };

    if (this._condition) {
      hook.condition = this._condition;
    }

    if (Object.keys(this._eventPayload).length > 0) {
      hook.eventPayload = this._eventPayload;
    }

    if (this._weight !== undefined) {
      hook.weight = this._weight;
    }

    if (this._createCheckpoint) {
      hook.createCheckpoint = this._createCheckpoint;
    }

    if (this._checkpointDescription) {
      hook.checkpointDescription = this._checkpointDescription;
    }

    return hook;
  }

  /**
   * Clone this builder (for creating variations)
   * @returns New builder with same configuration
   */
  clone(): AgentHookBuilder {
    const cloned = new AgentHookBuilder();
    cloned._hookType = this._hookType;
    cloned._eventName = this._eventName;
    cloned._condition = this._condition;
    cloned._eventPayload = { ...this._eventPayload };
    cloned._enabled = this._enabled;
    cloned._weight = this._weight;
    cloned._createCheckpoint = this._createCheckpoint;
    cloned._checkpointDescription = this._checkpointDescription;
    return cloned;
  }
}
