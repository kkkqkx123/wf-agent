/**
 * WorkflowBuilder - Declarative Workflow Builder
 * Provides a smooth chaining API to build workflow definitions, supports adding nodes and triggers from templates
 */

import type {
  WorkflowTemplate,
  WorkflowTemplateType,
  VariableDefinition,
  WorkflowConfig,
  WorkflowMetadata,
  Metadata,
  SubgraphNodeConfig,
  WorkflowStartConfig,
  LoopStartNodeConfig,
  TriggerConfigOverride,
} from "@wf-agent/types";
import type { StaticNode, StaticNodeType } from "@wf-agent/types";
import type { Edge } from "@wf-agent/types";
import type { Condition } from "@wf-agent/types";
import type { WorkflowTrigger } from "@wf-agent/types";
import type { TriggerReference } from "@wf-agent/types";
import { NodeTemplateNotFoundError } from "@wf-agent/types";
import { generateId } from "../../../utils/id-utils.js";
import { parseWorkflow, ConfigFormat, getConfigFormatFromPath } from "../../shared/config/index.js";
import * as fs from "fs/promises";
import { NodeBuilder } from "./node-builder.js";
import { BaseBuilder } from "../../shared/base-builder.js";
import type { GlobalContext } from "../../../shared/global-context.js";

/**
 * WorkflowBuilder - Declarative Workflow Builder
 */
export class WorkflowBuilder extends BaseBuilder<WorkflowTemplate> {
  private _id: string;
  private _name: string;
  private _version: string = "1.0.0";
  private _type?: string;
  private _config?: WorkflowConfig;
  private nodes: Map<string, StaticNode> = new Map();
  private edges: Edge[] = [];
  private variables: VariableDefinition[] = [];
  private triggers: (WorkflowTrigger | TriggerReference)[] = [];
  private globalContext: GlobalContext;

  private constructor(globalContext: GlobalContext, id: string) {
    super();
    this.globalContext = globalContext;
    this._id = id;
    this._name = id;
  }

  /**
   * Creating a new instance of WorkflowBuilder
   * @param globalContext The GlobalContext to access services
   * @param id Workflow ID
   * @returns WorkflowBuilder instance
   */
  static create(globalContext: GlobalContext, id: string): WorkflowBuilder {
    return new WorkflowBuilder(globalContext, id);
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
   * Setting the workflow type
   * @param type Workflow type ("STANDALONE" | "DEPENDENT" | "TRIGGERED_SUBWORKFLOW")
   * @returns this
   */
  type(type: string): this {
    this._type = type;
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
  addNode(id: string, type: StaticNodeType, config: StaticNode["config"], name?: string): this {
    if (this.nodes.has(id)) {
      throw new Error(`Node ID must be unique: '${id}' already exists in workflow '${this._id}'`);
    }
    const nodeBuilder = NodeBuilder.create(this.globalContext, id).type(type).config(config);
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
    if (this.nodes.has(node.id)) {
      throw new Error(
        `Node ID must be unique: '${node.id}' already exists in workflow '${this._id}'`,
      );
    }
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
    configOverride?: Partial<StaticNode["config"]>,
    nodeName?: string,
  ): this {
    const nodeTemplateRegistry = this.globalContext.nodeTemplateRegistry;
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

    return this.addNode(
      nodeId,
      template.type,
      mergedConfig as StaticNode["config"],
      nodeName || template.name,
    );
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
   * @param risk Risk level
   * @param name Node name (optional)
   * @returns this
   */
  addCodeNode(
    id: string,
    scriptName: string,
    risk: "none" | "low" | "medium" | "high",
    name?: string,
  ): this {
    const config = {
      scriptName,
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
          typeof route.condition === "string"
            ? ({ type: "expression", expression: route.condition } as Condition)
            : route.condition,
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
        ? ({ type: "expression", expression: condition } as Condition)
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
      freeze?: boolean;
    },
  ): this {
    const variable: VariableDefinition = {
      name,
      type,
      value: options?.defaultValue ?? undefined,
      readonly: options?.readonly || false,
      freeze: options?.freeze,
      metadata: {
        description: options?.description,
        required: options?.required,
      },
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
    configOverride?: TriggerConfigOverride,
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
  build(): WorkflowTemplate {
    // Validating workflows
    this.validate();

    // Building a complete workflow definition
    const workflow: WorkflowTemplate = {
      id: this._id,
      name: this._name,
      version: this._version,
      type: (this._type || "STANDALONE") as WorkflowTemplateType,
      description: this._description,
      config: this._config,
      metadata: this._metadata as Metadata,
      nodes: Array.from(this.nodes.values()),
      edges: this.edges,
      variables: this.variables.length > 0 ? this.variables : undefined,
      triggers: this.triggers.length > 0 ? this.triggers : undefined,
      createdAt: this.getCreatedAt(),
      updatedAt: this.getUpdatedAt(),
    } as unknown as WorkflowTemplate;

    return workflow;
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

    // Check for duplicate node IDs (defensive: Map keys are unique by design,
    // but this check verifies no corruption via internal code paths)
    const nodeIds = Array.from(this.nodes.keys());
    if (nodeIds.length !== new Set(nodeIds).size) {
      errors.push("Node IDs must be unique within the workflow.");
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
    const triggerTemplateRegistry = this.globalContext.triggerTemplateRegistry;
    for (const trigger of this.triggers) {
      if ("templateName" in trigger) {
        const reference = trigger as TriggerReference;
        if (!triggerTemplateRegistry.has(reference.templateName)) {
          errors.push(`Trigger template '${reference.templateName}' does not exist`);
        }
      }
    }

    // Validate subgraph variable mappings
    for (const node of Array.from(this.nodes.values())) {
      if (node.type === "SUBGRAPH") {
        const config = node.config as SubgraphNodeConfig;

        // Check that variableInputs reference valid parent variables
        if (config.variableInputs && config.variableInputs.length > 0) {
          for (const input of config.variableInputs) {
            const parentVar = this.variables.find(v => v.name === input.externalName);
            if (!parentVar && input.required && input.defaultValue === undefined) {
              errors.push(
                `Subgraph '${node.id}' requires variable '${input.externalName}' which is not defined in parent workflow`,
              );
            }
          }
        }
      }

      if (node.type === "START") {
        const config = node.config as WorkflowStartConfig;

        // Validate START node variable declarations
        if (config.variableInputs) {
          // Ensure no duplicate internal names
          const internalNames = config.variableInputs.map(i => i.internalName);
          const uniqueNames = new Set(internalNames);
          if (uniqueNames.size !== internalNames.length) {
            errors.push(`START node has duplicate internal variable names`);
          }
        }
      }

      if (node.type === "LOOP_START") {
        const config = node.config as LoopStartNodeConfig;

        // Validate LOOP_START variable inputs
        if (config.variableInputs && config.variableInputs.length > 0) {
          for (const input of config.variableInputs) {
            const parentVar = this.variables.find(v => v.name === input.externalName);
            if (!parentVar && input.required && input.defaultValue === undefined) {
              errors.push(
                `Loop '${config.loopId}' requires variable '${input.externalName}' which is not defined in parent workflow`,
              );
            }
          }
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
   * @param globalContext The GlobalContext to access services
   * @param configFile Configuration file content
   * @param format Configuration format ('toml' | 'json')
   * @param parameters runtime parameters
   * @returns WorkflowBuilder instance
   */
  static async fromConfig(
    globalContext: GlobalContext,
    configFile: string,
    format: ConfigFormat = "toml",
    parameters?: Record<string, unknown>,
  ): Promise<WorkflowBuilder> {
    const workflowDef = await parseWorkflow(configFile, format, parameters);

    const builder = new WorkflowBuilder(globalContext, workflowDef.id);
    // Populate the internal state of the builder
    builder._name = workflowDef.name;
    builder._version = workflowDef.version;
    builder._description = workflowDef.description;
    builder._type = workflowDef.type;
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
   * @param globalContext The GlobalContext to access services
   * @param filePath Configuration file path
   * @param parameters Runtime parameters
   * @returns WorkflowBuilder instance
   */
  static async fromConfigFile(
    globalContext: GlobalContext,
    filePath: string,
    parameters?: Record<string, unknown>,
  ): Promise<WorkflowBuilder> {
    // The application layer is responsible for file reading
    const content = await fs.readFile(filePath, "utf-8");
    const format = getConfigFormatFromPath(filePath);
    const workflowDef = await parseWorkflow(content, format, parameters);

    const builder = new WorkflowBuilder(globalContext, workflowDef.id);
    // Populate the internal state of the builder
    builder._name = workflowDef.name;
    builder._version = workflowDef.version;
    builder._description = workflowDef.description;
    builder._type = workflowDef.type;
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
