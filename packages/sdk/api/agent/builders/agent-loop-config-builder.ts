/**
 * AgentLoopConfigBuilder - Declarative Agent Loop Configuration Builder
 * Provides a fluent API to build agent loop runtime configurations
 */

import type {
  AgentLoopRuntimeConfig,
  AgentToolConfig,
  AgentHook,
  AgentTrigger,
  Message,
  ID,
} from "@wf-agent/types";

/**
 * AgentLoopConfigBuilder - Build agent loop runtime configuration with fluent API
 */
export class AgentLoopConfigBuilder {
  private _agentConfigId?: ID;
  private _profileId?: ID;
  private _systemPrompt?: string;
  private _systemPromptTemplateId?: string;
  private _systemPromptTemplateVariables: Record<string, unknown> = {};
  private _initialUserMessage?: string;
  private _maxIterations?: number;
  private _initialMessages: Message[] = [];
  private _availableTools?: AgentToolConfig;
  private _stream: boolean = false;
  private _createCheckpointOnEnd: boolean = false;
  private _createCheckpointOnError: boolean = false;
  private _hooks: AgentHook[] = [];
  private _triggers: AgentTrigger[] = [];
  private _metadata: Record<string, unknown> = {};

  /**
   * Private constructor
   */
  private constructor() {}

  /**
   * Create a new AgentLoopConfigBuilder instance
   * @returns AgentLoopConfigBuilder instance
   */
  static create(): AgentLoopConfigBuilder {
    return new AgentLoopConfigBuilder();
  }

  /**
   * Set the agent config ID
   * @param id Config ID
   * @returns this for chaining
   */
  agentConfigId(id: ID): this {
    this._agentConfigId = id;
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
   * Set the initial user message
   * @param message Initial message
   * @returns this for chaining
   */
  initialUserMessage(message: string): this {
    this._initialUserMessage = message;
    return this;
  }

  /**
   * Set the maximum number of iterations
   * @param iterations Max iterations (positive, or -1 for unlimited)
   * @returns this for chaining
   */
  maxIterations(iterations: number): this {
    this._maxIterations = iterations;
    return this;
  }

  /**
   * Add initial messages
   * @param messages Initial messages
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
   * Create checkpoint at end of execution
   * @returns this for chaining
   */
  createCheckpointOnEnd(): this {
    this._createCheckpointOnEnd = true;
    return this;
  }

  /**
   * Create checkpoint on error
   * @returns this for chaining
   */
  createCheckpointOnError(): this {
    this._createCheckpointOnError = true;
    return this;
  }

  /**
   * Add a hook
   * @param hook Hook configuration
   * @returns this for chaining
   */
  addHook(hook: AgentHook): this {
    this._hooks.push(hook);
    return this;
  }

  /**
   * Add multiple hooks
   * @param hooks Hooks array
   * @returns this for chaining
   */
  hooks(hooks: AgentHook[]): this {
    this._hooks = hooks;
    return this;
  }

  /**
   * Add a trigger
   * @param trigger Trigger configuration
   * @returns this for chaining
   */
  addTrigger(trigger: AgentTrigger): this {
    this._triggers.push(trigger);
    return this;
  }

  /**
   * Add multiple triggers
   * @param triggers Triggers array
   * @returns this for chaining
   */
  triggers(triggers: AgentTrigger[]): this {
    this._triggers = triggers;
    return this;
  }

  /**
   * Add metadata
   * @param key Metadata key
   * @param value Metadata value
   * @returns this for chaining
   */
  metadata(key: string, value: unknown): this;
  /**
   * Add metadata object
   * @param metadata Metadata object
   * @returns this for chaining
   */
  metadata(metadata: Record<string, unknown>): this;
  metadata(keyOrObj: string | Record<string, unknown>, value?: unknown): this {
    if (typeof keyOrObj === "string") {
      this._metadata[keyOrObj] = value;
    } else {
      this._metadata = { ...this._metadata, ...keyOrObj };
    }
    return this;
  }

  /**
   * Build the agent loop configuration
   * @returns AgentLoopRuntimeConfig
   */
  build(): AgentLoopRuntimeConfig {
    const config: AgentLoopRuntimeConfig = {};

    if (this._agentConfigId) {
      config.agentConfigId = this._agentConfigId;
    }

    if (this._profileId) {
      config.profileId = this._profileId;
    }

    if (this._systemPrompt) {
      config.systemPrompt = this._systemPrompt;
    }

    if (this._systemPromptTemplateId) {
      config.systemPromptTemplateId = this._systemPromptTemplateId;
    }

    if (Object.keys(this._systemPromptTemplateVariables).length > 0) {
      config.systemPromptTemplateVariables = this._systemPromptTemplateVariables;
    }

    if (this._initialUserMessage) {
      config.initialUserMessage = this._initialUserMessage;
    }

    if (this._maxIterations !== undefined) {
      config.maxIterations = this._maxIterations;
    }

    if (this._initialMessages.length > 0) {
      config.initialMessages = this._initialMessages;
    }

    if (this._availableTools) {
      config.availableTools = this._availableTools;
    }

    if (this._stream) {
      config.stream = this._stream;
    }

    if (this._createCheckpointOnEnd) {
      config.createCheckpointOnEnd = this._createCheckpointOnEnd;
    }

    if (this._createCheckpointOnError) {
      config.createCheckpointOnError = this._createCheckpointOnError;
    }

    if (this._hooks.length > 0) {
      config.hooks = this._hooks;
    }

    if (this._triggers.length > 0) {
      config.triggers = this._triggers;
    }

    // Attach metadata if present (note: standard AgentLoopRuntimeConfig might not have this field,
    // but we include it for future extensibility)
    if (Object.keys(this._metadata).length > 0) {
      (config as any).metadata = this._metadata;
    }

    return config;
  }

  /**
   * Clone this builder (for creating variations)
   * @returns New builder with same configuration
   */
  clone(): AgentLoopConfigBuilder {
    const cloned = new AgentLoopConfigBuilder();
    cloned._agentConfigId = this._agentConfigId;
    cloned._profileId = this._profileId;
    cloned._systemPrompt = this._systemPrompt;
    cloned._systemPromptTemplateId = this._systemPromptTemplateId;
    cloned._systemPromptTemplateVariables = { ...this._systemPromptTemplateVariables };
    cloned._initialUserMessage = this._initialUserMessage;
    cloned._maxIterations = this._maxIterations;
    cloned._initialMessages = [...this._initialMessages];
    cloned._availableTools = this._availableTools ? { ...this._availableTools } : undefined;
    cloned._stream = this._stream;
    cloned._createCheckpointOnEnd = this._createCheckpointOnEnd;
    cloned._createCheckpointOnError = this._createCheckpointOnError;
    cloned._hooks = [...this._hooks];
    cloned._triggers = [...this._triggers];
    cloned._metadata = { ...this._metadata };
    return cloned;
  }
}
