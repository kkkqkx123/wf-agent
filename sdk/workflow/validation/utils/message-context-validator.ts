/**
 * Subgraph Message Context Validation
 * 
 * Validates message context passing configuration between parent and subgraph workflows.
 * Ensures that inputs/outputs are properly mapped and required contexts are provided.
 */

import type { Node, StartNodeConfig, SubgraphNodeConfig } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Message context mapping result
 */
export interface MessageContextMapping {
  /** Parent workflow context ID → Subgraph internal name */
  inputMapping: Map<string, string>;
  
  /** Subgraph internal name → Parent workflow context ID */
  outputMapping: Map<string, string>;
}

/**
 * Validate and map message contexts for subgraph execution
 * 
 * This function validates that:
 * 1. messagePassing configuration is present (required)
 * 2. All referenced inputs exist in the subgraph's START node configuration
 * 3. All required inputs are provided by the parent workflow
 * 4. All referenced outputs exist in the subgraph's START node configuration
 * 
 * @param subgraphNode The SUBGRAPH node from parent workflow
 * @param subgraphStartNode The START node from subgraph workflow
 * @returns Validation result with mapping if successful
 */
export function validateAndMapMessageContexts(
  subgraphNode: Node,
  subgraphStartNode: Node,
): Result<MessageContextMapping, ConfigurationValidationError[]> {
  const errors: ConfigurationValidationError[] = [];
  
  const subgraphConfig = subgraphNode.config as SubgraphNodeConfig;
  const startConfig = subgraphStartNode.config as unknown as StartNodeConfig;
  
  // messagePassing is now required
  if (!subgraphConfig.messagePassing) {
    errors.push(
      new ConfigurationValidationError(
        `SUBGRAPH node '${subgraphNode.id}' must configure messagePassing`,
        {
          configType: "node",
          configPath: `nodes[${subgraphNode.id}].config.messagePassing`,
          context: {
            code: "MISSING_MESSAGE_PASSING_CONFIG",
            nodeId: subgraphNode.id,
            subgraphId: subgraphConfig.subgraphId,
          },
        }
      )
    );
    return err(errors);
  }
  
  const mapping: MessageContextMapping = {
    inputMapping: new Map(),
    outputMapping: new Map(),
  };
  
  // Validate inputs
  if (subgraphConfig.messagePassing?.inputs) {
    for (const [externalName, parentContextId] of Object.entries(subgraphConfig.messagePassing.inputs)) {
      const inputDef = startConfig.messageInputs?.find((i: { externalName: string }) => i.externalName === externalName);
      
      if (!inputDef) {
        errors.push(
          new ConfigurationValidationError(
            `Subgraph '${subgraphConfig.subgraphId}' does not accept input '${externalName}'`,
            {
              configType: "node",
              configPath: `nodes[${subgraphNode.id}].config.messagePassing.inputs`,
              value: externalName,
              context: {
                code: "INVALID_SUBGRAPH_INPUT",
                nodeId: subgraphNode.id,
                subgraphId: subgraphConfig.subgraphId,
                externalName,
              },
            }
          )
        );
        continue;
      }
      
      // Map parent context to subgraph internal name
      mapping.inputMapping.set(parentContextId as string, inputDef.internalName);
    }
  }
  
  // Validate outputs
  if (subgraphConfig.messagePassing?.outputs) {
    for (const [externalName, parentContextId] of Object.entries(subgraphConfig.messagePassing.outputs)) {
      const outputDef = startConfig.messageOutputs?.find((o: { externalName: string }) => o.externalName === externalName);
      
      if (!outputDef) {
        errors.push(
          new ConfigurationValidationError(
            `Subgraph '${subgraphConfig.subgraphId}' does not produce output '${externalName}'`,
            {
              configType: "node",
              configPath: `nodes[${subgraphNode.id}].config.messagePassing.outputs`,
              value: externalName,
              context: {
                code: "INVALID_SUBGRAPH_OUTPUT",
                nodeId: subgraphNode.id,
                subgraphId: subgraphConfig.subgraphId,
                externalName,
              },
            }
          )
        );
        continue;
      }
      
      // Map subgraph internal name to parent context
      mapping.outputMapping.set(outputDef.internalName, parentContextId as string);
    }
  }
  
  if (errors.length > 0) {
    return err(errors);
  }
  
  return ok(mapping);
}

/**
 * Check if a subgraph has message context configuration
 * 
 * @param subgraphNode The SUBGRAPH node
 * @returns true if messagePassing is configured
 */
export function hasMessageContextConfig(subgraphNode: Node): boolean {
  const config = subgraphNode.config as SubgraphNodeConfig;
  return !!(config.messagePassing && (config.messagePassing.inputs || config.messagePassing.outputs));
}
