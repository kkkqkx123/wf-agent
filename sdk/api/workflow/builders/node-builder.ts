/**
 * NodeBuilder - node builder
 * Provides a fluent chaining API to build node definitions, supports creation from templates and configuration overrides
 */

import type { Node, NodeConfig, NodeType } from "@wf-agent/types";
import type { NodeTemplateRegistry } from "../../../core/registry/node-template-registry.js";
import { NodeTemplateNotFoundError, ConfigurationValidationError } from "@wf-agent/types";
import { generateId } from "../../../utils/id-utils.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { BaseBuilder } from "../../shared/base-builder.js";

/**
 * NodeBuilder - node builder
 */
export class NodeBuilder extends BaseBuilder<Node> {
  private _id: string;
  private _type?: NodeType;
  private _name?: string;
  private _config: NodeConfig = {};
  private _outgoingEdgeIds: string[] = [];
  private _incomingEdgeIds: string[] = [];

  private constructor(id?: string) {
    super();
    this._id = id || generateId();
  }

  /**
   * Create a new instance of NodeBuilder
   * @param id Node ID (optional, auto-generated)
   * @returns NodeBuilder instance
   */
  static create(id?: string): NodeBuilder {
    return new NodeBuilder(id);
  }

  /**
   * Setting the node type
   * @param type Node type
   * @returns this
   */
  type(type: NodeType): this {
    this._type = type;
    this.updateTimestamp();
    return this;
  }

  /**
   * Setting the node name
   * @param name Node name
   * @returns this
   */
  name(name: string): this {
    this._name = name;
    this.updateTimestamp();
    return this;
  }

  /**
   * Setting the Node Configuration
   * @param config node configuration
   * @returns this
   */
  config(config: NodeConfig): this {
    this._config = config;
    this.updateTimestamp();
    return this;
  }

  /**
   * Merge Configuration (partial update)
   * @param partialConfig partialConfig object that merges shallow into existing configuration
   * @returns this
   */
  mergeConfig(partialConfig: Partial<NodeConfig>): this {
    this._config = { ...this._config, ...partialConfig };
    this.updateTimestamp();
    return this;
  }

  /**
   * Loading configuration from a node template
   * @param templateName node template name
   * @param configOverride Configuration override (optional)
   * @returns this
   */
  fromTemplate(templateName: string, configOverride?: Partial<NodeConfig>): this {
    const container = getContainer();
    const nodeTemplateRegistry = container.get(
      Identifiers.NodeTemplateRegistry,
    ) as NodeTemplateRegistry;
    const template = nodeTemplateRegistry.get(templateName);
    if (!template) {
      throw new NodeTemplateNotFoundError(
        `Node template '${templateName}' does not exist`,
        templateName,
      );
    }

    this._type = template.type;
    this._name = template.name;
    this._config = configOverride ? { ...template.config, ...configOverride } : template.config;
    this.updateTimestamp();
    return this;
  }

  /**
   * Building Nodes
   * @returns node definition
   */
  build(): Node {
    // Validating Required Fields
    if (!this._id) {
      throw new ConfigurationValidationError("Node ID cannot be null", { configType: "node" });
    }
    if (!this._type) {
      throw new ConfigurationValidationError("The node type cannot be null", {
        configType: "node",
      });
    }
    if (!this._name) {
      this._name = this._id;
    }

    // Use type assertions, since runtime validation already ensures that the type is correct
    return {
      id: this._id,
      type: this._type,
      name: this._name,
      config: this._config,
      outgoingEdgeIds: this._outgoingEdgeIds,
      incomingEdgeIds: this._incomingEdgeIds,
    } as Node;
  }

  // The following shortcut methods are used to quickly create specific types of nodes

  /**
   * Create the START node
   * @param id node id (optional, defaults to 'start')
   * @returns this
   */
  start(id: string = "start"): this {
    this._id = id;
    return this.type("start" as NodeType).name("Start");
  }

  /**
   * Create the END node
   * @param id node id (optional, defaults to 'end')
   * @returns this
   */
  end(id: string = "end"): this {
    this._id = id;
    return this.type("end" as NodeType).name("End");
  }

  /**
   * Create an LLM node
   * @param profileId LLM Profile ID
   * @param prompt Prompt (optional)
   * @returns this
   */
  llm(profileId: string, prompt?: string): this {
    return this.type("llm" as NodeType).mergeConfig({
      profileId,
      ...(prompt && { prompt }),
    });
  }

  /**
   * Creating a CODE node
   * @param scriptName Script name
   * @param risk Risk level
   * @returns this
   */
  code(scriptName: string, risk: "none" | "low" | "medium" | "high"): this {
    return this.type("code" as NodeType).mergeConfig({
      scriptName,
      risk,
    });
  }

  /**
   * Create a VARIABLE node
   * @param variableName: Variable name
   * @param variableType: Variable type
   * @param expression: Expression
   * @returns: This node itself
   */
  variable(
    variableName: string,
    variableType: "number" | "string" | "boolean" | "array" | "object",
    expression: string,
  ): this {
    return this.type("variable" as NodeType).mergeConfig({
      variableName,
      variableType,
      expression,
    });
  }

  /**
   * Creating a ROUTE node
   * @param routes Routing rules
   * @param defaultTargetNodeId Default target node ID (optional)
   * @returns this
   */
  route(
    routes: Array<{ condition: string; targetNodeId: string; priority?: number }>,
    defaultTargetNodeId?: string,
  ): this {
    return this.type("route" as NodeType).mergeConfig({
      routes: routes.map(route => ({
        ...route,
        condition: { expression: route.condition },
      })),
      defaultTargetNodeId,
    });
  }
}
