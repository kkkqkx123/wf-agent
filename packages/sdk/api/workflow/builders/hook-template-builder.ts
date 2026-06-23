/**
 * HookTemplateBuilder - Hook Template Builder
 * Provides a fluent chain-of-command API for creating and registering hook templates.
 */

import type { HookTemplate, NodeHook, HookType } from "@wf-agent/types";
import { TemplateBuilder } from "./template-builder.js";
import type { GlobalContext } from "../../../shared/global-context.js";

/**
 * HookTemplateBuilder - Hook Template Builder
 */
export class HookTemplateBuilder extends TemplateBuilder<HookTemplate> {
  private _name: string;
  private _hook: NodeHook | null = null;
  private globalContext: GlobalContext;

  private constructor(globalContext: GlobalContext, name: string) {
    super();
    this.globalContext = globalContext;
    this._name = name;
  }

  /**
   * Create a new HookTemplateBuilder instance
   * @param globalContext - The GlobalContext to access services
   * @param name - Template name
   * @returns HookTemplateBuilder instance
   */
  static create(globalContext: GlobalContext, name: string): HookTemplateBuilder {
    return new HookTemplateBuilder(globalContext, name);
  }

  /**
   * Set the hook type.
   * @param hookType - The hook type (e.g., "BEFORE_EXECUTE", "AFTER_EXECUTE")
   * @returns this
   */
  hookType(hookType: HookType): this {
    if (!this._hook) {
      this._hook = {
        hookType,
        eventName: "",
      } as NodeHook;
    } else {
      this._hook = { ...this._hook, hookType };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the event name.
   * @param eventName - Event name for identification and routing
   * @returns this
   */
  eventName(eventName: string): this {
    if (!this._hook) {
      this._hook = {
        hookType: "BEFORE_EXECUTE",
        eventName,
      } as NodeHook;
    } else {
      this._hook = { ...this._hook, eventName };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the hook configuration.
   * @param hook - The full NodeHook configuration
   * @returns this
   */
  hook(hook: NodeHook): this {
    this._hook = hook;
    this.updateTimestamp();
    return this;
  }

  /**
   * Merge hook configuration (partial update).
   * @param partialHook - A partial hook configuration to merge into the existing one
   * @returns this
   */
  mergeHook(partialHook: Partial<NodeHook>): this {
    if (!this._hook) {
      this._hook = {
        hookType: "BEFORE_EXECUTE",
        eventName: "",
        ...partialHook,
      } as NodeHook;
    } else {
      this._hook = { ...this._hook, ...partialHook };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Set whether the hook is enabled.
   * @param enabled - Whether the hook is enabled
   * @returns this
   */
  enabled(enabled: boolean): this {
    if (!this._hook) {
      this._hook = { hookType: "BEFORE_EXECUTE", eventName: "", enabled } as NodeHook;
    } else {
      this._hook = { ...this._hook, enabled };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Set the priority weight.
   * @param weight - Priority weight (higher number = higher priority)
   * @returns this
   */
  weight(weight: number): this {
    if (!this._hook) {
      this._hook = { hookType: "BEFORE_EXECUTE", eventName: "", weight } as NodeHook;
    } else {
      this._hook = { ...this._hook, weight };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Set event payload.
   * @param payload - Event payload template, supports variable substitution
   * @returns this
   */
  eventPayload(payload: Record<string, unknown>): this {
    if (!this._hook) {
      this._hook = { hookType: "BEFORE_EXECUTE", eventName: "", eventPayload: payload } as NodeHook;
    } else {
      this._hook = { ...this._hook, eventPayload: payload };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Set checkpoint creation flag.
   * @param createCheckpoint - Whether to create a checkpoint when this hook is triggered
   * @param description - Optional checkpoint description
   * @returns this
   */
  checkpoint(createCheckpoint: boolean, description?: string): this {
    if (!this._hook) {
      this._hook = {
        hookType: "BEFORE_EXECUTE",
        eventName: "",
        createCheckpoint,
        checkpointDescription: description,
      } as NodeHook;
    } else {
      this._hook = {
        ...this._hook,
        createCheckpoint,
        checkpointDescription: description || this._hook.checkpointDescription,
      };
    }
    this.updateTimestamp();
    return this;
  }

  /**
   * Register the template in the hook template registry.
   * @param template - HookTemplate template
   */
  protected registerTemplate(template: HookTemplate): void {
    const hookTemplateRegistry = this.globalContext.hookTemplateRegistry;
    hookTemplateRegistry.register(template);
  }

  /**
   * Build a hook template.
   * @returns HookTemplate
   * @throws Error if required fields are missing
   */
  build(): HookTemplate {
    if (!this._name) {
      throw new Error("The template name cannot be empty.");
    }
    if (!this._hook) {
      throw new Error(
        "Hook configuration cannot be empty. Use .hook(), .hookType() or .eventName() to set it.",
      );
    }
    if (!this._hook.hookType) {
      throw new Error("Hook type cannot be empty. Use .hookType() to set it.");
    }
    if (!this._hook.eventName) {
      throw new Error("Event name cannot be empty. Use .eventName() to set it.");
    }

    return {
      name: this._name,
      hook: this._hook,
      description: this._description,
      metadata: this._metadata,
      createdAt: this.getCreatedAt(),
      updatedAt: this.getUpdatedAt(),
    };
  }
}
