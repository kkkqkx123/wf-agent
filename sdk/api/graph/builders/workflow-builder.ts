/**
 * WorkflowBuilder - Declarative Workflow Builder
 * Provides a smooth chaining API to build workflow definitions, supports adding nodes and triggers from templates
 */

import type {
  WorkflowDefinition,
  WorkflowVariable,
  WorkflowConfig,
  WorkflowMetadata,
  Metadata,
} from "@wf-agent/types";
import type { Node, NodeConfig } from "@wf-agent/types";
import { NodeType } from "@wf-agent/types";
import type { Edge } from "@wf-agent/types";
import type { Condition } from "@wf-agent/types";
import type { WorkflowTrigger } from "@wf-agent/types";
import type { TriggerReference } from "@wf-agent/types";
import { NodeTemplateNotFoundError } from "@wf-agent/types";
import { generateId } from "../../../utils/id-utils.js";
import { getContainer } from "../../../core/di/index.js";
import * as Identifiers from "../../../core/di/service-identifiers.js";
import { ConfigParser, ConfigFormat } from "../../shared/config/index.js";
import { loadConfigContent } from "../../shared/config/config-utils.js";
import { NodeBuilder } from "./node-builder.js";
import { BaseBuilder } from "../../shared/base-builder.js";

/**
 * WorkflowBuilder - Declarative Workflow Builder
 */
export class WorkflowBuilder extends BaseBuilder<WorkflowDefinition> {
  private _id: string;
  private _name: string;
  private _version: string = "1.0.0";
  private _config?: WorkflowConfig;
  private nodes: Map<string, Node> = new Map();
  private edges: Edge[] = [];
  private variables: WorkflowVariable[] = [];
  private triggers: (WorkflowTrigger | TriggerReference)[] = [];

  private constructor(id: string) {
    super();
    this._id = id;
    this._name = id;
  }

  /**
   * Creating a new instance of WorkflowBuilder
   * @param id Workflow ID
   * @returns WorkflowBuilder instance
   */
  static create(id: string): WorkflowBuilder {
    return new WorkflowBuilder(id);
  }

  /**
   * Setting the workflow name
   * @param name Workflow name
   * @returns this
   */
  name(name: string): this {
    this._name = name;
    this.updateTimestamp();
    return this;
  }

  /**
   * Setting the workflow version
   * @param version version number
   * @returns this
   */
  version(version: string): this {
    this._version = version;
    this.updateTimestamp();
    return this;
  }

  /**
   * Setting up a workflow configuration
   * @param config workflow configuration
   * @returns this
   */
  config(config: WorkflowConfig): this {
    this._config = config;
    this.updateTimestamp();
    return this;
  }

  /**
   * Setting workflow metadata
   * @param metadata Workflow metadata
   * @returns this
   */
  override metadata(metadata: WorkflowMetadata): this {
    super.metadata(metadata as Metadata);
    return this;
  }

  /**
   * Adding a node
   * @param id Node ID
   * @param type Node type
   * @param config node configuration
   * @param name Node name (optional, defaults to ID)
   * @returns this
   */
  addNode(id: string, type: NodeType, config: NodeConfig, name?: string): this {
    const nodeBuilder = NodeBuilder.create(id).type(type).config(config);
    if (name) {
      nodeBuilder.name(name);
    }
    const node = nodeBuilder.build();
    this.nodes.set(id, node);
    this.updateTimestamp();
    return this;
  }

  /**
   * Add nodes using NodeBuilder
   * @param nodeBuilder An instance of NodeBuilder
   * @returns this
   */
  addNodeWithBuilder(nodeBuilder: NodeBuilder): this {
    const node = nodeBuilder.build();
    this.nodes.set(node.id, node);
    this.updateTimestamp();
    return this;
  }

  /**
   * Adding a node from a node template
   * @param nodeId node ID (unique in the workflow)
   * @param templateName Node template name
   * @param configOverride Configuration override (optional)
   * @param nodeName node name (optional)
   * @returns this
   */
  addNodeFromTemplate(
    nodeId: string,
    templateName: string,
    configOverride?: Partial<NodeConfig>,
    nodeName?: string,
  ): this {
    const container = getContainer();
    const nodeTemplateRegistry = container.get(Identifiers.NodeTemplateRegistry) as {
      get: (name: string) => { config: NodeConfig; type: NodeType; name: string } | undefined;
    };
    const template = nodeTemplateRegistry.get(templateName);
    if (!template) {
      throw new NodeTemplateNotFoundError(
        `Node template '${templateName}' does not exist`,
        templateName,
      );
    }

    // Merge Configuration
    const mergedConfig = configOverride
      ? { ...template.config, ...configOverride }
      : template.config;

    return this.addNode(nodeId, template.type, mergedConfig, nodeName || template.name);
  }

  /**
   * Add the START node
   * @param id node id (optional, defaults to 'start')
   * @returns this
   */
  addStartNode(id: string = "start"): this {
    return this.addNode(id, "START", {});
  }

  /**
   * Add the END node
   * @param id node id (optional, defaults to 'end')
   * @returns this
   */
  addEndNode(id: string = "end"): this {
    return this.addNode(id, "END", {});
  }

  /**
   * Adding an LLM node
   * @param id Node ID
   * @param profileId LLM Profile ID
   * @param prompt Prompt word (optional)
   * @param name Node name (optional)
   * @returns this
   */
  addLLMNode(id: string, profileId: string, prompt?: string, name?: string): this {
    const config = {
      profileId,
      prompt,
    };
    return this.addNode(id, "LLM", config, name);
  }

  /**
   * Adding a CODE node
   * @param id Node ID
   * @param scriptName Script name
   * @param scriptType script type
   * @param risk Risk level
   * @param name Node name (optional)
   * @returns this
   */
  addCodeNode(
    id: string,
    scriptName: string,
    scriptType: "shell" | "cmd" | "powershell" | "python" | "javascript",
    risk: "none" | "low" | "medium" | "high",
    name?: string,
  ): this {
    const config = {
      scriptName,
      scriptType,
      risk,
    };
    return this.addNode(id, "SCRIPT", config, name);
  }

  /**
   * Adding a VARIABLE node
   * @param id Node ID
   * @param variableName Variable name
   * @param variableType Variable type
   * @param expression expression
   * @param name Node name (optional)
   * @returns this
   */
  addVariableNode(
    id: string,
    variableName: string,
    variableType: "number" | "string" | "boolean" | "array" | "object",
    expression: string,
    name?: string,
  ): this {
    const config = {
      variableName,
      variableType,
      expression,
    };
    return this.addNode(id, "VARIABLE", config, name);
  }

  /**
   * Adding a ROUTE node
   * @param id node id
   * @param routes Routing rules
   * @param defaultTargetNodeId Default target node id (optional)
   * @param name Node name (optional)
   * @returns this
   */
  addRouteNode(
    id: string,
    routes: Array<{ condition: string | Condition; targetNodeId: string; priority?: number }>,
    defaultTargetNodeId?: string,
    name?: string,
  ): this {
    const config = {
      routes: routes.map(route => ({
        ...route,
        condition:
          typeof route.condition === "string" ? { expression: route.condition } : route.condition,
      })),
      defaultTargetNodeId,
    };
    return this.addNode(id, "ROUTE", config, name);
  }

  /**
   * Add Edge
   * @param from source node ID
   * @param to target node ID
   * @param condition (optional)
   * @returns this
   */
  addEdge(from: string, to: string, condition?: string | Condition): this {
    const edgeCondition: Condition | undefined = condition
      ? typeof condition === "string"
        ? { expression: condition }
        : condition
      : undefined;

    const edge: Edge = {
      id: generateId(),
      sourceNodeId: from,
      targetNodeId: to,
      type: edgeCondition ? "CONDITIONAL" : "DEFAULT",
      condition: edgeCondition,
    };
    this.edges.push(edge);
    this.updateTimestamp();
    return this;
  }

  /**
   * Adding Variables
   * @param name Variable name
   * @param type Variable type
   * @param options Variable options
   * @returns this
   */
  addVariable(
    name: string,
    type: "number" | "string" | "boolean" | "array" | "object",
    options?: {
      defaultValue?: unknown;
      description?: string;
      required?: boolean;
      readonly?: boolean;
      scope?: "global" | "thread" | "local" | "loop";
    },
  ): this {
    const variable: WorkflowVariable = {
      name,
      type,
      ...options,
    };
    this.variables.push(variable);
    this.updateTimestamp();
    return this;
  }

  /**
   * Adding a Trigger
   * @param trigger Trigger definition
   * @returns this
   */
  addTrigger(trigger: WorkflowTrigger): this {
    this.triggers.push(trigger);
    this.updateTimestamp();
    return this;
  }

  /**
   * Adding a trigger from a trigger template
   * @param triggerId Trigger ID (unique in the workflow)
   * @param templateName Trigger template name
   * @param configOverride Configuration override (optional)
   * @param triggerName Trigger name (optional)
   * @returns this
   */
  addTriggerFromTemplate(
    triggerId: string,
    templateName: string,
    configOverride?: import("@wf-agent/types").TriggerConfigOverride,
    triggerName?: string,
  ): this {
    const reference: TriggerReference = {
      templateName,
      triggerId,
      triggerName,
      configOverride,
    };
    this.triggers.push(reference);
    this.updateTimestamp();
    return this;
  }

  /**
   * Building Workflow Definitions
   * @returns Workflow Definitions
   */
  build(): WorkflowDefinition {
    // Update edge references of a node
    this.updateNodeEdgeReferences();

    // Validating workflows
    this.validate();

    // Building a complete workflow definition
    const workflow: WorkflowDefinition = {
      id: this._id,
      name: this._name,
      version: this._version,
      description: this._description,
      config: this._config,
      metadata: this._metadata as Metadata,
      nodes: Array.from(this.nodes.values()),
      edges: this.edges,
      variables: this.variables.length > 0 ? this.variables : undefined,
      triggers: this.triggers.length > 0 ? this.triggers : undefined,
      createdAt: this.getCreatedAt(),
      updatedAt: this.getUpdatedAt(),
    } as unknown as WorkflowDefinition;

    return workflow;
  }

  /**
   * Update edge references of a node
   */
  private updateNodeEdgeReferences(): void {
    // Clear edge references for all nodes
    for (const node of Array.from(this.nodes.values())) {
      node.outgoingEdgeIds = [];
      node.incomingEdgeIds = [];
    }

    // Refill side references
    for (const edge of this.edges) {
      const fromNode = this.nodes.get(edge.sourceNodeId);
      const toNode = this.nodes.get(edge.targetNodeId);

      if (fromNode) {
        fromNode.outgoingEdgeIds.push(edge.id);
      }
      if (toNode) {
        toNode.incomingEdgeIds.push(edge.id);
      }
    }
  }

  /**
   * Validating workflows
   */
  private validate(): void {
    const errors: string[] = [];

    // Check for nodes
    if (this.nodes.size === 0) {
      errors.push("The workflow must have at least one node.");
    }

    // Check for START nodes
    const startNodes = Array.from(this.nodes.values()).filter(n => n.type === "START");
    if (startNodes.length === 0) {
      errors.push("The workflow must have a START node.");
    } else if (startNodes.length > 1) {
      errors.push("The workflow can only have one START node.");
    }

    // Check for END nodes
    const endNodes = Array.from(this.nodes.values()).filter(n => n.type === "END");
    if (endNodes.length === 0) {
      errors.push("The workflow must have an END node.");
    } else if (endNodes.length > 1) {
      errors.push("The workflow can only have one END node.");
    }

    // Checking the validity of edges
    for (const edge of this.edges) {
      if (!this.nodes.has(edge.sourceNodeId)) {
        errors.push(`Source node of edge does not exist: ${edge.sourceNodeId}`);
      }
      if (!this.nodes.has(edge.targetNodeId)) {
        errors.push(`Target node of edge does not exist: ${edge.targetNodeId}`);
      }
    }

    // Validating Trigger References
    const container = getContainer();
    const triggerTemplateRegistry = container.get(Identifiers.TriggerTemplateRegistry) as {
      has: (name: string) => boolean;
    };
    for (const trigger of this.triggers) {
      if ("templateName" in trigger) {
        const reference = trigger as TriggerReference;
        if (!triggerTemplateRegistry.has(reference.templateName)) {
          errors.push(`Trigger template '${reference.templateName}' does not exist`);
        }
      }
    }

    // Throw an exception if there is an error
    if (errors.length > 0) {
      throw new Error(`Workflow validation failed:\n${errors.join("\n")}`);
    }
  }

  /**
   * Creating a workflow from the contents of a configuration file
   * @param configFile Configuration file content
   * @param format Configuration format ('toml' | 'json')
   * @param parameters runtime parameters
   * @returns WorkflowBuilder instance
   */
  static async fromConfig(
    configFile: string,
    format: ConfigFormat = "toml",
    parameters?: Record<string, unknown>,
  ): Promise<WorkflowBuilder> {
    const parser = new ConfigParser();
    const workflowDef = await parser.parseAndTransform(configFile, format, parameters);

    const builder = new WorkflowBuilder(workflowDef.id);
    // Populate the internal state of the builder
    builder._name = workflowDef.name;
    builder._version = workflowDef.version;
    builder._description = workflowDef.description;
    builder._config = workflowDef.config;
    builder._metadata = (workflowDef.metadata || {}) as Metadata;
    builder._createdAt = workflowDef.createdAt;
    builder._updatedAt = workflowDef.updatedAt;

    // Reconstructing the mapping of nodes and edges
    for (const node of workflowDef.nodes) {
      builder.nodes.set(node.id, node);
    }
    builder.edges = workflowDef.edges;
    builder.variables = workflowDef.variables || [];
    builder.triggers = workflowDef.triggers || [];

    return builder;
  }

  /**
   * Creating a workflow from a configuration file path
   * @param filePath Configuration file path
   * @param parameters Runtime parameters
   * @returns WorkflowBuilder instance
   */
  static async fromConfigFile(
    filePath: string,
    parameters?: Record<string, unknown>,
  ): Promise<WorkflowBuilder> {
    const parser = new ConfigParser();

    // The application layer is responsible for file reading
    const { content, format } = await loadConfigContent(filePath);
    const workflowDef = await parser.parseAndTransform(content, format, parameters);

    const builder = new WorkflowBuilder(workflowDef.id);
    // Populate the internal state of the builder
    builder._name = workflowDef.name;
    builder._version = workflowDef.version;
    builder._description = workflowDef.description;
    builder._config = workflowDef.config;
    builder._metadata = (workflowDef.metadata || {}) as Metadata;
    builder._createdAt = workflowDef.createdAt;
    builder._updatedAt = workflowDef.updatedAt;

    // Reconstructing the mapping of nodes and edges
    for (const node of workflowDef.nodes) {
      builder.nodes.set(node.id, node);
    }
    builder.edges = workflowDef.edges;
    builder.variables = workflowDef.variables || [];
    builder.triggers = workflowDef.triggers || [];

    return builder;
  }
}
