/**
 * NodeTemplateBuilder - Node Template Builder
 * Provides a fluent chain-of-command API for creating and registering node templates.
 */

import type { NodeTemplate } from "@wf-agent/types";
import type { NodeType, NodeConfig } from "@wf-agent/types";
import type { NodeTemplateRegistry } from "../../../core/registry/node-template-registry.js";
import { TemplateBuilder } from "./template-builder.js";
import type { GlobalContext } from "../../../core/global-context.js";

/**
 * NodeTemplateBuilder - Node Template Builder
 */
export class NodeTemplateBuilder extends TemplateBuilder<NodeTemplate> {
  private _name: string;
  private _type: NodeType;
  private _config: NodeConfig = {} as NodeConfig;
  private globalContext: GlobalContext;

  private constructor(globalContext: GlobalContext, name: string, type: NodeType) {
    super();
    this.globalContext = globalContext;
    this._name = name;
    this._type = type;
  }

  /**
   * Create a new NodeTemplateBuilder instance
   * @param globalContext The GlobalContext to access services
   * @param name: Template name
   * @param type: Node type
   * @returns: NodeTemplateBuilder instance
   */
  static create(globalContext: GlobalContext, name: string, type: NodeType): NodeTemplateBuilder {
    return new NodeTemplateBuilder(globalContext, name, type);
  }

  /**
   * Set node configuration
   * @param config Node configuration
   * @returns this
   */
  config(config: NodeConfig): this {
    this._config = config;
    this.updateTimestamp();
    return this;
  }

  /**
   * Merge configuration (partial update)
   * @param partialConfig: A partial configuration object that will be merged shallowly into the existing configuration
   * @returns: The updated configuration object
   */
  mergeConfig(partialConfig: Partial<NodeConfig>): this {
    if (!this._config) {
      this._config = {} as NodeConfig;
    }
    this._config = { ...this._config, ...partialConfig };
    this.updateTimestamp();
    return this;
  }

  /**
   * Register the template in the node template registry.
   * @param template: Node template
   */
  protected registerTemplate(template: NodeTemplate): void {
    const nodeTemplateRegistry = this.globalContext.nodeTemplateRegistry;
    nodeTemplateRegistry.register(template);
  }

  /**
   * Build a node template
   * @returns Node template
   */
  build(): NodeTemplate {
    // Verify required fields
    if (!this._name) {
      throw new Error("The template name cannot be empty.");
    }
    if (!this._type) {
      throw new Error("The node type cannot be empty.");
    }
    if (!this._config) {
      throw new Error("Node configuration cannot be empty.");
    }

    return {
      name: this._name,
      type: this._type,
      config: this._config,
      description: this._description,
      metadata: this._metadata,
      createdAt: this.getCreatedAt(),
      updatedAt: this.getUpdatedAt(),
    };
  }
}
