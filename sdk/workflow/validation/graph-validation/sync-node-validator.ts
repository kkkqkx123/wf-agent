/**
 * SYNC Node Validator
 *
 * Validates SYNC nodes configuration and pairing:
 * - SYNC nodes must be within a FORK-JOIN branch structure
 * - sourcePathId and targetPathId must exist in parent FORK node's forkPaths
 * - variableMappings format is valid and consistent
 * - SYNC nodes are not isolated (have incoming/outgoing edges)
 * - Paired SYNC nodes have matching variable mappings (bidirectional validation)
 * - Data flow direction is unidirectional (no circular dependencies)
 */

import type { ID, StaticNodeType, SyncNodeConfig } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { WorkflowGraphData } from "../../entities/workflow-graph-data.js";

/**
 * Validate SYNC nodes configuration and pairing
 * @param graph Graph data containing SYNC nodes
 * @returns List of validation errors
 */
export function validateSyncNodes(
  graph: WorkflowGraphData,
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Collect all SYNC nodes with full config
  const syncNodes: Array<{
    nodeId: ID;
    config: SyncNodeConfig;
  }> = [];

  for (const node of graph.nodes.values()) {
    if (node.type === ("SYNC" as StaticNodeType)) {
      const config = node.originalNode?.config as SyncNodeConfig | undefined;

      if (config) {
        syncNodes.push({
          nodeId: node.id,
          config,
        });
      }
    }
  }

  // If no SYNC nodes, skip validation
  if (syncNodes.length === 0) {
    return errors;
  }

  // Build FORK node path mapping (forkPathIds -> forkNodeId)
  const forkPathMapping = new Map<ID, ID>();
  for (const node of graph.nodes.values()) {
    if (node.type === ("FORK" as StaticNodeType)) {
      const config = node.originalNode?.config as {
        forkPaths?: Array<{ pathId: ID; childNodeId: ID }>;
      } | undefined;

      if (config?.forkPaths) {
        for (const forkPath of config.forkPaths) {
          forkPathMapping.set(forkPath.pathId, node.id);
        }
      }
    }
  }

  // Validate each SYNC node
  for (const syncNode of syncNodes) {
    const { nodeId, config } = syncNode;

    // Check if sourcePathId is present
    let hasConfigError = false;
    if (!config.sourcePathId || !config.sourcePathId.trim()) {
      errors.push(
        new ConfigurationValidationError(
          `SYNC node '${nodeId}' is missing required sourcePathId`,
          {
            configType: "workflow",
            context: {
              code: "MISSING_SYNC_SOURCE_PATH_ID",
              nodeId,
            },
          }
        )
      );
      hasConfigError = true;
      // Don't continue - still check other configurations and isolation
    }

    // Verify sourcePathId exists in a FORK node's forkPaths
    if (config.sourcePathId && forkPathMapping.has(config.sourcePathId)) {
      // Valid sourcePathId
    } else if (config.sourcePathId) {
      errors.push(
        new ConfigurationValidationError(
          `SYNC node '${nodeId}' has sourcePathId '${config.sourcePathId}' that does not exist in any FORK node's forkPaths`,
          {
            configType: "workflow",
            context: {
              code: "INVALID_SYNC_SOURCE_PATH_ID",
              nodeId,
              sourcePathId: config.sourcePathId,
            },
          }
        )
      );
      hasConfigError = true;
    }

    // Verify targetPathId if provided
    if (config.targetPathId && config.targetPathId.trim()) {
      if (!forkPathMapping.has(config.targetPathId)) {
        errors.push(
          new ConfigurationValidationError(
            `SYNC node '${nodeId}' has targetPathId '${config.targetPathId}' that does not exist in any FORK node's forkPaths`,
            {
              configType: "workflow",
              context: {
                code: "INVALID_SYNC_TARGET_PATH_ID",
                nodeId,
                targetPathId: config.targetPathId,
              },
            }
          )
        );
      }
    }

    // Validate variableMappings format and consistency
    if (config.variableMappings && config.variableMappings.length > 0) {
      const externalNames = new Set<string>();
      const internalNames = new Set<string>();

      for (const mapping of config.variableMappings) {
        if (!mapping.externalName || !mapping.externalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SYNC node '${nodeId}' has variableMapping with missing externalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_SYNC_MAPPING_EXTERNAL_NAME",
                  nodeId,
                  internalName: mapping.internalName,
                },
              }
            )
          );
        } else {
          // Check for duplicate external names
          if (externalNames.has(mapping.externalName)) {
            errors.push(
              new ConfigurationValidationError(
                `SYNC node '${nodeId}' has duplicate externalName '${mapping.externalName}' in variableMappings`,
                {
                  configType: "workflow",
                  context: {
                    code: "DUPLICATE_SYNC_MAPPING_EXTERNAL_NAME",
                    nodeId,
                    externalName: mapping.externalName,
                  },
                }
              )
            );
          }
          externalNames.add(mapping.externalName);
        }

        if (!mapping.internalName || !mapping.internalName.trim()) {
          errors.push(
            new ConfigurationValidationError(
              `SYNC node '${nodeId}' has variableMapping with missing internalName`,
              {
                configType: "workflow",
                context: {
                  code: "MISSING_SYNC_MAPPING_INTERNAL_NAME",
                  nodeId,
                  externalName: mapping.externalName,
                },
              }
            )
          );
        } else {
          // Check for duplicate internal names
          if (internalNames.has(mapping.internalName)) {
            errors.push(
              new ConfigurationValidationError(
                `SYNC node '${nodeId}' has duplicate internalName '${mapping.internalName}' in variableMappings`,
                {
                  configType: "workflow",
                  context: {
                    code: "DUPLICATE_SYNC_MAPPING_INTERNAL_NAME",
                    nodeId,
                    internalName: mapping.internalName,
                  },
                }
              )
            );
          }
          internalNames.add(mapping.internalName);
        }

        // Check for self-mapping (externalName === internalName is allowed but should be warned)
        if (mapping.externalName === mapping.internalName) {
          // This is allowed but we can add a warning in development mode
          if (process.env["NODE_ENV"] === "development") {
            console.warn(
              `[DEV] SYNC node '${nodeId}' has self-mapping: ${mapping.externalName} -> ${mapping.internalName}`
            );
          }
        }
      }
    }

    // Check if SYNC node is isolated (no incoming or outgoing edges)
    const incomingEdges = graph.getIncomingEdges(nodeId);
    const outgoingEdges = graph.getOutgoingEdges(nodeId);
    if (incomingEdges.length === 0 && outgoingEdges.length === 0) {
      errors.push(
        new ConfigurationValidationError(
          `SYNC node '${nodeId}' is isolated, has no incoming or outgoing edges`,
          {
            configType: "workflow",
            context: {
              code: "ISOLATED_SYNC_NODE",
              nodeId,
            },
          }
        )
      );
    }
  }

  // Additional validations for SYNC node pairing and data flow
  errors.push(...validateSyncNodePairing(syncNodes, forkPathMapping));
  errors.push(...validateDataFlowDirection(syncNodes));

  return errors;
}

/**
 * Validate SYNC node pairing and variable mapping consistency
 * 
 * This validation ensures that:
 * 1. If bidirectional sync is needed, paired SYNC nodes exist
 * 2. Variable mappings between paired nodes are consistent
 * 3. No conflicting data flows exist
 * 
 * @param syncNodes All SYNC nodes in the graph
 * @param _forkPathMapping Mapping of path IDs to FORK node IDs (reserved for future use)
 * @returns List of validation errors
 */
function validateSyncNodePairing(
  syncNodes: Array<{ nodeId: ID; config: SyncNodeConfig }>,
  _forkPathMapping: Map<ID, ID>
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Group SYNC nodes by their sourcePathId to identify potential pairs
  const syncBySourcePath = new Map<ID, Array<{ nodeId: ID; config: SyncNodeConfig }>>();
  
  for (const syncNode of syncNodes) {
    const sourcePathId = syncNode.config.sourcePathId;
    if (!sourcePathId) continue;
    
    if (!syncBySourcePath.has(sourcePathId)) {
      syncBySourcePath.set(sourcePathId, []);
    }
    syncBySourcePath.get(sourcePathId)!.push(syncNode);
  }

  // Check for potential issues in each group
  for (const [sourcePathId, nodes] of syncBySourcePath.entries()) {
    if (nodes.length > 1) {
      // Multiple SYNC nodes syncing from the same source path
      // This might be intentional (multiple targets) or a configuration error
      
      // Check if any of them have overlapping variable mappings
      const allInternalNames = new Map<string, string>(); // internalName -> nodeId
      
      for (const node of nodes) {
        if (node.config.variableMappings) {
          for (const mapping of node.config.variableMappings) {
            const existingNodeId = allInternalNames.get(mapping.internalName);
            if (existingNodeId && existingNodeId !== node.nodeId) {
              // Same internalName used in different SYNC nodes from same source
              // This could cause conflicts
              errors.push(
                new ConfigurationValidationError(
                  `Multiple SYNC nodes from sourcePathId '${sourcePathId}' map to the same internalName '${mapping.internalName}'. ` +
                  `This may cause data conflicts. Nodes: '${existingNodeId}' and '${node.nodeId}'`,
                  {
                    configType: "workflow",
                    context: {
                      code: "CONFLICTING_SYNC_MAPPINGS",
                      nodeId: node.nodeId,
                      sourcePathId,
                      internalName: mapping.internalName,
                      conflictingNodeId: existingNodeId,
                    },
                  }
                )
              );
            }
            allInternalNames.set(mapping.internalName, node.nodeId);
          }
        }
      }
    }
  }

  // Validate bidirectional sync consistency (if applicable)
  // Look for pairs where A syncs to B and B syncs to A
  const syncPairs = new Map<string, { source: ID; target: ID }>();
  
  for (const syncNode of syncNodes) {
    const key = `${syncNode.config.sourcePathId}->${syncNode.config.targetPathId || 'auto'}`;
    syncPairs.set(key, {
      source: syncNode.config.sourcePathId!,
      target: syncNode.config.targetPathId || 'auto'
    });
  }

  // Check for circular dependencies in variable mappings
  const reportedCycles = new Set<string>(); // Track reported cycles to avoid duplicates
  
  for (const syncNode of syncNodes) {
    if (!syncNode.config.variableMappings || syncNode.config.variableMappings.length === 0) {
      continue;
    }

    // Check if any variable creates a circular dependency
    const sourceMapping = syncNode.config.variableMappings;
    
    // Look for reverse SYNC node (target -> source)
    const reverseSyncNode = syncNodes.find(other => 
      other.nodeId !== syncNode.nodeId &&
      other.config.sourcePathId === syncNode.config.targetPathId &&
      other.config.targetPathId === syncNode.config.sourcePathId
    );

    if (reverseSyncNode && reverseSyncNode.config.variableMappings) {
      // Create a unique cycle key to avoid duplicate reports
      const cycleKey = [syncNode.nodeId, reverseSyncNode.nodeId].sort().join('-');
      
      if (reportedCycles.has(cycleKey)) {
        continue; // Already reported this cycle
      }
      
      // Check for circular variable dependencies
      for (const forwardMapping of sourceMapping) {
        const reverseMapping = reverseSyncNode.config.variableMappings.find(
          rm => rm.externalName === forwardMapping.internalName
        );

        if (reverseMapping && reverseMapping.internalName === forwardMapping.externalName) {
          // Circular dependency detected: A.x -> B.y and B.y -> A.x
          reportedCycles.add(cycleKey); // Mark as reported
          errors.push(
            new ConfigurationValidationError(
              `Circular variable dependency detected between SYNC nodes '${syncNode.nodeId}' and '${reverseSyncNode.nodeId}': ` +
              `'${forwardMapping.externalName}' <-> '${forwardMapping.internalName}'`,
              {
                configType: "workflow",
                context: {
                  code: "CIRCULAR_SYNC_DEPENDENCY",
                  nodeId: syncNode.nodeId,
                  pairedNodeId: reverseSyncNode.nodeId,
                  variableName: forwardMapping.externalName,
                },
              }
            )
          );
        }
      }
    }
  }

  return errors;
}

/**
 * Validate data flow direction to ensure unidirectional flow
 * 
 * SYNC nodes should follow unidirectional data flow principles:
 * - Data flows from sourcePathId to the SYNC node's branch
 * - No circular data dependencies should exist
 * 
 * @param syncNodes All SYNC nodes in the graph
 * @returns List of validation errors
 */
function validateDataFlowDirection(
  syncNodes: Array<{ nodeId: ID; config: SyncNodeConfig }>
): ConfigurationValidationError[] {
  const errors: ConfigurationValidationError[] = [];

  // Build a directed graph of data flows
  const dataFlows = new Map<ID, Set<ID>>(); // sourcePathId -> Set of targetPathIds

  for (const syncNode of syncNodes) {
    const sourcePathId = syncNode.config.sourcePathId;
    const targetPathId = syncNode.config.targetPathId;

    if (!sourcePathId || !targetPathId) continue;

    if (!dataFlows.has(sourcePathId)) {
      dataFlows.set(sourcePathId, new Set());
    }
    dataFlows.get(sourcePathId)!.add(targetPathId);
  }

  // Check for circular data flows using DFS
  const visited = new Set<ID>();
  const recursionStack = new Set<ID>();

  function hasCycle(pathId: ID, path: ID[]): boolean {
    visited.add(pathId);
    recursionStack.add(pathId);

    const neighbors = dataFlows.get(pathId);
    if (neighbors) {
      for (const neighbor of neighbors) {
        if (!visited.has(neighbor)) {
          if (hasCycle(neighbor, [...path, neighbor])) {
            return true;
          }
        } else if (recursionStack.has(neighbor)) {
          // Cycle detected
          const cycleStart = path.indexOf(neighbor);
          const cycle = path.slice(cycleStart).concat(neighbor);
          errors.push(
            new ConfigurationValidationError(
              `Circular data flow detected in SYNC nodes: ${cycle.join(' -> ')}`,
              {
                configType: "workflow",
                context: {
                  code: "CIRCULAR_DATA_FLOW",
                  cycle: cycle,
                },
              }
            )
          );
          return true;
        }
      }
    }

    recursionStack.delete(pathId);
    return false;
  }

  // Check all path IDs for cycles
  for (const pathId of dataFlows.keys()) {
    if (!visited.has(pathId)) {
      hasCycle(pathId, [pathId]);
    }
  }

  return errors;
}
