/**
 * Node Template Configuration Validation Functions
 * 
 * Provides convenient functions for validating node template configurations.
 * These functions wrap the NodeValidator class to provide a simple functional interface.
 */

import type { NodeTemplate, StaticNode } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError, NodeTemplateSchema } from "@wf-agent/types";
import type { ZodIssue } from "zod";
import { NodeValidator } from "./node-validator.js";
import { ok, err } from "@wf-agent/common-utils";

/**
 * Validate NodeTemplate configuration
 * Two-phase validation:
 * 1. Schema validation (fast, catches format errors)
 * 2. Deep business logic validation (NodeValidator)
 * 
 * @param config The node template configuration object
 * @returns The validation result
 */
export function validateNodeTemplateConfig(
  config: NodeTemplate,
): Result<NodeTemplate, ValidationError[]> {
  const errors: ValidationError[] = [];

  // Phase 1: Schema validation
  const schemaResult = NodeTemplateSchema.safeParse(config);
  if (!schemaResult.success) {
    const schemaErrors = schemaResult.error.issues.map((e: ZodIssue) => 
      new ConfigurationValidationError(e.message, {
        configType: "schema",
        field: e.path.join("."),
      })
    );
    return err(schemaErrors) as Result<NodeTemplate, ValidationError[]>;
  }

  // Phase 2: Deep business logic validation - Entrusted to NodeValidator
  const nodeValidator = new NodeValidator();
  
  // Create a temporary node object to verify configuration
  const tempNode = {
    id: "temp-node-id",
    type: config.type,
    name: config.name,
    description: config.description,
    config: config.config,
    metadata: config.metadata,
    outgoingEdgeIds: [],
    incomingEdgeIds: [],
  } as unknown as StaticNode;

  const configResult = nodeValidator.validateNode(tempNode);
  if (configResult.isErr()) {
    errors.push(...configResult.error);
  }

  if (errors.length > 0) {
    return err(errors) as Result<NodeTemplate, ValidationError[]>;
  }

  return ok(config) as Result<NodeTemplate, ValidationError[]>;
}

/**
 * Get validation warnings for NodeTemplate configuration
 * @param config The node template configuration object
 * @returns Array of warning messages
 */
export function getNodeTemplateValidationWarnings(config: NodeTemplate): string[] {
  const warnings: string[] = [];

  // Add node template-specific warnings here
  if (config.config && typeof config.config === 'object' && 'timeout' in config.config) {
    const timeout = (config.config as { timeout?: number }).timeout;
    if (timeout && timeout > 60000) {
      warnings.push(`Node has a long timeout (${timeout}ms). Consider if this is intentional.`);
    }
  }

  return warnings;
}
