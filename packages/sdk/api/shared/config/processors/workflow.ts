/**
 * Workflow Configuration Processing Functions
 * Provide functions for validating, transforming, and exporting Workflow configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig, WorkflowConfigFile } from "../types.js";
import type { Result, WorkflowTemplate, StaticNode, Edge } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { validateWorkflowConfig } from "../../../../workflow/validation/workflow-config-validation.js";
import { substituteParameters } from "../config-utils.js";
import { generateId } from "../../../../utils/id-utils.js";
import { ok } from "@wf-agent/common-utils";
import { ConfigFormat } from "../types.js";
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------

/**
 * Parse workflow configuration from raw content.
 * Combines parse + validate + transform into a single step.
 *
 * @param content - Raw file content (TOML or JSON string).
 * @param format - Expected format.
 * @param parameters - Optional runtime parameters.
 * @returns The fully resolved WorkflowTemplate.
 */
export async function parseWorkflow(
  content: string,
  format: ConfigFormat,
  parameters?: Record<string, unknown>,
): Promise<WorkflowTemplate> {
  const raw: unknown = format === "toml" ? parseToml(content) : parseJson(content);
  // Flatten TOML format before validation (TOML has nested workflow section)
  const flatConfig = flattenWorkflowConfig(raw as WorkflowConfigFile);
  // Apply default timestamps before validation
  const now = Date.now();
  const pendingWorkflow = {
    ...flatConfig,
    createdAt: flatConfig.createdAt ?? now,
    updatedAt: flatConfig.updatedAt ?? now,
  } as WorkflowTemplate;
  // Transform nodes/edges before deep validation
  // (WorkflowValidator expects transformed data with name, id, sourceNodeId, type)
  const transformedNodes = transformNodes(pendingWorkflow.nodes || []);
  const transformedEdges = transformEdges(pendingWorkflow.edges || []);
  const config: ParsedConfig<"workflow"> = {
    configType: "workflow",
    format,
    config: {
      ...pendingWorkflow,
      nodes: transformedNodes,
      edges: transformedEdges,
    } as WorkflowTemplate,
    rawContent: content,
  };
  const validated = validateWorkflow(config);
  if (validated.isErr()) {
    const msgs = validated.error.map(e => e.message).join("\n");
    throw new Error(`Workflow config validation failed:\n${msgs}`);
  }
  return transformWorkflow(validated.value, parameters);
}

// ---------------------------------------------------------------------------
// Validate
// ---------------------------------------------------------------------------

/**
 * Verify Workflow configuration
 * Delegates to the unified config validator in core/validation
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateWorkflow(
  config: ParsedConfig<"workflow">,
): Result<ParsedConfig<"workflow">, ValidationError[]> {
  const workflow = config.config as WorkflowTemplate;

  // Delegate to unified config validator
  const result = validateWorkflowConfig(workflow);

  // Use `andThen` for type conversion
  // ConfigurationValidationError[] is assignable to ValidationError[] (subtype relationship)
  return result.andThen(() => ok(config)) as Result<ParsedConfig<"workflow">, ValidationError[]>;
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

/**
 * Transform workflow configuration to WorkflowTemplate.
 *
 * Processing steps:
 * 1. Flatten TOML format (extract nested `workflow` section)
 * 2. Apply parameter substitution
 * 3. Transform nodes and edges to internal format
 *
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed WorkflowTemplate
 */
export function transformWorkflow(
  config: ParsedConfig<"workflow">,
  parameters?: Record<string, unknown>,
): WorkflowTemplate {
  // 1. Flatten TOML format (extract nested `workflow` section)
  const flattened = flattenWorkflowConfig(config.config);

  // 2. Apply parameter substitution
  const processed = applyParameterSubstitution(flattened, parameters);

  // 3. Transform nodes and edges
  const nodes = transformNodes(processed.nodes || []);
  const edges = transformEdges(processed.edges || []);

  // 4. Build WorkflowTemplate
  const now = Date.now();
  return {
    ...processed,
    nodes,
    edges,
    createdAt: processed.createdAt ?? now,
    updatedAt: processed.updatedAt ?? now,
  } as WorkflowTemplate;
}

/**
 * Flatten workflow configuration from TOML format.
 *
 * TOML format: { workflow: { id, name, ... }, nodes: [...], edges: [...] }
 * WorkflowTemplate: { id, name, nodes: [...], edges: [...], ... }
 *
 * @param configFile The parsed configuration file
 * @returns Flattened configuration
 */
function flattenWorkflowConfig(configFile: WorkflowConfigFile): WorkflowTemplate {
  const configRecord = configFile as unknown as Record<string, unknown>;

  // Check if the config has a nested 'workflow' property (TOML format)
  if (configRecord["workflow"] && typeof configRecord["workflow"] === "object") {
    const { workflow, ...rest } = configRecord;
    return {
      ...(workflow as Record<string, unknown>),
      ...rest,
    } as unknown as WorkflowTemplate;
  }

  // Already in flat format (JSON format)
  return configFile;
}

/**
 * Apply parameter substitution to workflow configuration.
 *
 * @param config Workflow configuration
 * @param parameters Runtime parameters
 * @returns Configuration with parameters substituted
 */
function applyParameterSubstitution(
  config: WorkflowConfigFile,
  parameters?: Record<string, unknown>,
): WorkflowConfigFile {
  // substituteParameters is idempotent for empty parameters
  return substituteParameters(config, parameters);
}

/**
 * Transform nodes from config format to internal format.
 *
 * Config format: { id, type, config, name? }
 * Internal format: StaticNode (without edge IDs - they are added at runtime)
 *
 * @param nodeConfigs Array of node configurations
 * @returns Array of transformed nodes
 */
function transformNodes(nodeConfigs: unknown[]): StaticNode[] {
  return nodeConfigs.map(nodeConfig => {
    const node = nodeConfig as {
      id: string;
      type: string;
      name?: string;
      config?: unknown;
      description?: string;
      metadata?: unknown;
      hooks?: unknown;
      checkpointBeforeExecute?: boolean;
      checkpointAfterExecute?: boolean;
    };

    return {
      id: node.id,
      type: node.type as StaticNode["type"],
      name: node.name || node.id,
      config: node.config as StaticNode["config"],
      description: node.description,
      metadata: node.metadata as Record<string, unknown>,
      hooks: node.hooks,
      checkpointBeforeExecute: node.checkpointBeforeExecute,
      checkpointAfterExecute: node.checkpointAfterExecute,
    } as StaticNode;
  });
}

/**
 * Transform edges from config format to internal format.
 *
 * Config format: { from, to, condition? }
 * Internal format: { id, sourceNodeId, targetNodeId, type, condition?, ... }
 *
 * @param edgeConfigs Array of edge configurations
 * @returns Array of transformed edges
 */
function transformEdges(edgeConfigs: unknown[]): Edge[] {
  return edgeConfigs.map(edgeConfig => {
    const edge = edgeConfig as {
      id?: string;
      sourceNodeId?: string;
      from?: string;
      targetNodeId?: string;
      to?: string;
      type?: string;
      condition?: unknown;
      label?: string;
      description?: string;
      weight?: number;
      metadata?: unknown;
    };

    const sourceNodeId = edge.sourceNodeId || edge.from;
    const targetNodeId = edge.targetNodeId || edge.to;

    // Validate required fields
    if (!sourceNodeId || !targetNodeId) {
      throw new ValidationError(
        `Edge missing required source/target: sourceNodeId=${sourceNodeId}, targetNodeId=${targetNodeId}`,
        "edge",
        { sourceNodeId, targetNodeId },
      );
    }

    const hasCondition = edge.condition !== undefined;

    return {
      id: edge.id || generateId(),
      sourceNodeId,
      targetNodeId,
      type: edge.type || (hasCondition ? "CONDITIONAL" : "DEFAULT"),
      condition: edge.condition,
      label: edge.label,
      description: edge.description,
      weight: edge.weight,
      metadata: edge.metadata,
    } as Edge;
  });
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/**
 * Export Workflow configuration
 * Returns typed data ready for serialization.
 * @param workflowDef WorkflowTemplate object
 * @returns The workflow data ready for export
 */
export function exportWorkflow(workflowDef: WorkflowTemplate): WorkflowTemplate {
  return workflowDef;
}
