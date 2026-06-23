/**
 * AgentToolConfigBuilder - Fluent builder for Agent tool configuration
 *
 * Provides a declarative API for building AgentToolConfig objects.
 * Validates that requireApproval tools are a subset of available tools.
 */

import type { AgentToolConfig } from "@wf-agent/types";

/**
 * AgentToolConfigBuilder - Build agent tool configuration with fluent API
 */
export class AgentToolConfigBuilder {
  private _tools: string[] = [];
  private _requireApproval: string[] = [];
  private _allowedWorkflows: string[] = [];

  /**
   * Create a new AgentToolConfigBuilder instance
   * @returns AgentToolConfigBuilder instance
   */
  static create(): AgentToolConfigBuilder {
    return new AgentToolConfigBuilder();
  }

  /**
   * Add a tool to the available tools list
   * @param tool Tool name/id
   * @returns this for chaining
   */
  addTool(tool: string): this {
    if (!this._tools.includes(tool)) {
      this._tools.push(tool);
    }
    return this;
  }

  /**
   * Add multiple tools to the available tools list
   * @param tools Array of tool names/ids
   * @returns this for chaining
   */
  addTools(tools: string[]): this {
    for (const tool of tools) {
      this.addTool(tool);
    }
    return this;
  }

  /**
   * Set the complete tools list (replaces existing)
   * @param tools Array of tool names/ids
   * @returns this for chaining
   */
  tools(tools: string[]): this {
    this._tools = [...tools];
    return this;
  }

  /**
   * Mark a tool as requiring approval before execution
   * @param tool Tool name/id that requires approval
   * @returns this for chaining
   */
  requireApproval(tool: string): this {
    if (!this._requireApproval.includes(tool)) {
      this._requireApproval.push(tool);
    }
    return this;
  }

  /**
   * Mark multiple tools as requiring approval
   * @param tools Array of tool names/ids
   * @returns this for chaining
   */
  requireApprovals(tools: string[]): this {
    for (const tool of tools) {
      this.requireApproval(tool);
    }
    return this;
  }

  /**
   * Allow execution of specific workflows
   * @param workflows Array of workflow IDs, or ['*'] to allow all
   * @returns this for chaining
   */
  allowedWorkflows(workflows: string[]): this {
    this._allowedWorkflows = [...workflows];
    return this;
  }

  /**
   * Add a single workflow to allowed list
   * @param workflow Workflow ID
   * @returns this for chaining
   */
  addAllowedWorkflow(workflow: string): this {
    if (!this._allowedWorkflows.includes(workflow)) {
      this._allowedWorkflows.push(workflow);
    }
    return this;
  }

  /**
   * Build the agent tool configuration
   * @returns AgentToolConfig
   * @throws Error if requireApproval contains tools not in the tools list
   */
  build(): AgentToolConfig {
    if (this._tools.length === 0) {
      throw new Error("At least one tool must be added");
    }

    // Validate that all requireApproval tools are in the tools list
    const toolSet = new Set(this._tools);
    for (const tool of this._requireApproval) {
      if (!toolSet.has(tool)) {
        throw new Error(
          `Tool '${tool}' in requireApproval is not in the tools list`
        );
      }
    }

    const config: AgentToolConfig = {
      tools: this._tools,
    };

    if (this._requireApproval.length > 0) {
      config.requireApproval = this._requireApproval;
    }

    if (this._allowedWorkflows.length > 0) {
      config.allowedWorkflows = this._allowedWorkflows;
    }

    return config;
  }

  /**
   * Clone this builder (for creating variations)
   * @returns New builder with same configuration
   */
  clone(): AgentToolConfigBuilder {
    const cloned = new AgentToolConfigBuilder();
    cloned._tools = [...this._tools];
    cloned._requireApproval = [...this._requireApproval];
    cloned._allowedWorkflows = [...this._allowedWorkflows];
    return cloned;
  }
}
