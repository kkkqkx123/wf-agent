/**
 * AgentDefinitionBuilder - Fluent builder for Agent Loop definition
 *
 * Provides a declarative API for building AgentLoopDefinition objects.
 * Used for creating static agent configurations.
 */

import type {
  AgentLoopDefinition,
  AgentHookStatic,
  AgentTriggerStatic,
  AgentToolConfig,
  AgentCheckpointConfig,
  Message,
  ID,
} from "@wf-agent/types";
import type { DynamicContextConfig } from "@wf-agent/types";
import { BaseBuilder } from "../../shared/base-builder.js";

/**
 * Utility to generate unique IDs
 */
function generateId(): string {
  return `agent-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/**
 * AgentDefinitionBuilder - Build agent loop definition with fluent API
 * Extends BaseBuilder to share common builder functionality (description, metadata, tags, timestamps)
 * with other builders across Agent and Workflow domains.
 */
export class AgentDefinitionBuilder extends BaseBuilder<AgentLoopDefinition> {
  private _id?: ID;
  private _name?: string;
  private _version: string = "1.0.0";
  private _profileId?: ID;
  private _systemPrompt?: string;
  private _systemPromptTemplateId?: string;
  private _systemPromptTemplateVariables: Record<string, unknown> = {};
  private _maxIterations?: number;
  private _initialMessages: Message[] = [];
  private _availableTools?: AgentToolConfig;
  private _stream: boolean = false;
  private _hooks: AgentHookStatic[] = [];
  private _triggers: AgentTriggerStatic[] = [];
  private _dynamicContext?: DynamicContextConfig;
  private _checkpoint?: AgentCheckpointConfig;

  /**
   * Create a new AgentDefinitionBuilder instance
   * @returns AgentDefinitionBuilder instance
   */
  static create(): AgentDefinitionBuilder {
    return new AgentDefinitionBuilder();
  }

  /**
   * Set the agent ID
   * @param id Agent ID
   * @returns this for chaining
   */
  id(id: ID): this {
    this._id = id;
    return this;
  }

  /**
   * Set the agent name
   * @param name Agent name
   * @returns this for chaining
   */
  name(name: string): this {
    this._name = name;
    return this;
  }

  /**
   * Set the agent version
   * @param version Version string
   * @returns this for chaining
   */
  version(version: string): this {
    this._version = version;
    return this;
  }

  /**
   * Set the agent description
   * @param description Agent description
   * @returns this for chaining
   */
  override description(description: string): this {
    this._description = description;
    this._updatedAt = Date.now();
    return this;
  }

  /**
   * Set the LLM profile ID
   * @param profileId Profile ID
   * @returns this for chaining
   */
  profileId(profileId: ID): this {
    this._profileId = profileId;
    return this;
  }

  /**
   * Set the system prompt
   * @param prompt System prompt message
   * @returns this for chaining
   */
  systemPrompt(prompt: string): this {
    this._systemPrompt = prompt;
    return this;
  }

  /**
   * Set the system prompt template ID
   * @param templateId Template ID
   * @returns this for chaining
   */
  systemPromptTemplate(templateId: string): this {
    this._systemPromptTemplateId = templateId;
    return this;
  }

  /**
   * Set system prompt template variables
   * @param variables Template variables
   * @returns this for chaining
   */
  systemPromptVariables(variables: Record<string, unknown>): this {
    this._systemPromptTemplateVariables = variables;
    return this;
  }

  /**
   * Set the maximum number of iterations
   * @param iterations Max iterations (-1 for unlimited)
   * @returns this for chaining
   */
  maxIterations(iterations: number): this {
    this._maxIterations = iterations;
    return this;
  }

  /**
   * Set initial messages
   * @param messages Initial message list
   * @returns this for chaining
   */
  initialMessages(messages: Message[]): this {
    this._initialMessages = messages;
    return this;
  }

  /**
   * Set available tools configuration
   * @param toolConfig Tool configuration
   * @returns this for chaining
   */
  availableTools(toolConfig: AgentToolConfig): this {
    this._availableTools = toolConfig;
    return this;
  }

  /**
   * Enable streaming output
   * @returns this for chaining
   */
  enableStreaming(): this {
    this._stream = true;
    return this;
  }

  /**
   * Disable streaming output
   * @returns this for chaining
   */
  disableStreaming(): this {
    this._stream = false;
    return this;
  }

  /**
   * Set streaming state
   * @param stream Whether streaming is enabled
   * @returns this for chaining
   */
  stream(stream: boolean): this {
    this._stream = stream;
    return this;
  }

  /**
   * Add a hook
   * @param hook Hook configuration
   * @returns this for chaining
   */
  addHook(hook: AgentHookStatic): this {
    this._hooks.push(hook);
    return this;
  }

  /**
   * Add multiple hooks
   * @param hooks Hooks array
   * @returns this for chaining
   */
  addHooks(hooks: AgentHookStatic[]): this {
    this._hooks.push(...hooks);
    return this;
  }

  /**
   * Set hooks (replaces existing)
   * @param hooks Hooks array
   * @returns this for chaining
   */
  hooks(hooks: AgentHookStatic[]): this {
    this._hooks = hooks;
    return this;
  }

  /**
   * Add a trigger
   * @param trigger Trigger configuration
   * @returns this for chaining
   */
  addTrigger(trigger: AgentTriggerStatic): this {
    this._triggers.push(trigger);
    return this;
  }

  /**
   * Add multiple triggers
   * @param triggers Triggers array
   * @returns this for chaining
   */
  addTriggers(triggers: AgentTriggerStatic[]): this {
    this._triggers.push(...triggers);
    return this;
  }

  /**
   * Set triggers (replaces existing)
   * @param triggers Triggers array
   * @returns this for chaining
   */
  triggers(triggers: AgentTriggerStatic[]): this {
    this._triggers = triggers;
    return this;
  }

  /**
   * Set dynamic context configuration
   * @param config Dynamic context config
   * @returns this for chaining
   */
  dynamicContext(config: DynamicContextConfig): this {
    this._dynamicContext = config;
    return this;
  }

  /**
   * Set checkpoint configuration
   * @param config Checkpoint config
   * @returns this for chaining
   */
  checkpoint(config: AgentCheckpointConfig): this {
    this._checkpoint = config;
    return this;
  }

  /**
   * Enable checkpoint creation at end
   * @returns this for chaining
   */
  createCheckpointOnEnd(): this {
    if (!this._checkpoint) {
      this._checkpoint = {};
    }
    this._checkpoint.createOnEnd = true;
    return this;
  }

  /**
   * Enable checkpoint creation on error
   * @returns this for chaining
   */
  createCheckpointOnError(): this {
    if (!this._checkpoint) {
      this._checkpoint = {};
    }
    this._checkpoint.createOnError = true;
    return this;
  }

  /**
   * Add metadata field
   * @param keyOrObj Metadata key or object
   * @param value Metadata value (if key is a string)
   * @returns this for chaining
   */
  override metadata(keyOrObj: string | Record<string, unknown>, value?: unknown): this {
    if (typeof keyOrObj === "string") {
      this._metadata[keyOrObj] = value;
    } else {
      this._metadata = { ...this._metadata, ...keyOrObj };
    }
    this._updatedAt = Date.now();
    return this;
  }

  /**
   * Build the agent loop definition
   * @returns AgentLoopDefinition
   */
  build(): AgentLoopDefinition {
    if (!this._name) {
      throw new Error("Agent name is required");
    }

    const definition: AgentLoopDefinition = {
      id: this._id || generateId(),
      name: this._name,
      version: this._version,
    };

    if (this._description) {
      definition.description = this._description;
    }

    if (this._profileId) {
      definition.profileId = this._profileId;
    }

    if (this._systemPrompt) {
      definition.systemPrompt = this._systemPrompt;
    }

    if (this._systemPromptTemplateId) {
      definition.systemPromptTemplateId = this._systemPromptTemplateId;
    }

    if (Object.keys(this._systemPromptTemplateVariables).length > 0) {
      definition.systemPromptTemplateVariables = this._systemPromptTemplateVariables;
    }

    if (this._maxIterations !== undefined) {
      definition.maxIterations = this._maxIterations;
    }

    if (this._initialMessages.length > 0) {
      definition.initialMessages = this._initialMessages;
    }

    if (this._availableTools) {
      definition.availableTools = this._availableTools;
    }

    if (this._stream) {
      definition.stream = this._stream;
    }

    if (this._hooks.length > 0) {
      definition.hooks = this._hooks;
    }

    if (this._triggers.length > 0) {
      definition.triggers = this._triggers;
    }

    if (this._dynamicContext) {
      definition.dynamicContext = this._dynamicContext;
    }

    if (this._checkpoint) {
      definition.checkpoint = this._checkpoint;
    }

    if (Object.keys(this._metadata).length > 0) {
      definition.metadata = this._metadata;
    }

    definition.createdAt = Date.now();

    return definition;
  }

  /**
   * Clone this builder (for creating variations)
   * @returns New builder with same configuration
   */
  clone(): AgentDefinitionBuilder {
    const cloned = new AgentDefinitionBuilder();
    cloned._id = this._id;
    cloned._name = this._name;
    cloned._version = this._version;
    cloned._description = this._description;
    cloned._profileId = this._profileId;
    cloned._systemPrompt = this._systemPrompt;
    cloned._systemPromptTemplateId = this._systemPromptTemplateId;
    cloned._systemPromptTemplateVariables = { ...this._systemPromptTemplateVariables };
    cloned._maxIterations = this._maxIterations;
    cloned._initialMessages = [...this._initialMessages];
    cloned._availableTools = this._availableTools ? { ...this._availableTools } : undefined;
    cloned._stream = this._stream;
    cloned._hooks = [...this._hooks];
    cloned._triggers = [...this._triggers];
    cloned._dynamicContext = this._dynamicContext ? { ...this._dynamicContext } : undefined;
    cloned._checkpoint = this._checkpoint ? { ...this._checkpoint } : undefined;
    cloned._metadata = { ...this._metadata };
    return cloned;
  }
}
