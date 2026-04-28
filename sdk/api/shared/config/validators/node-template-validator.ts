/**
 * Node Template Configuration Validation Function
 * Responsible for verifying the validity of node template configurations.
 * Note: The actual validation logic is delegated to NodeValidator; this function serves only as an adapter.
 */

import type { NodeTemplate, Node } from "@wf-agent/types";
import type { ConfigFile } from "../types.js";
// ConfigType is used for type guards, not directly needed here
import { ok, err } from "@wf-agent/common-utils";
import type { Result } from "@wf-agent/types";
import { ValidationError } from "@wf-agent/types";
import { NodeValidator } from "../../../../workflow/validation/node-validator.js";
// NodeType is used for validation, imported via NodeValidator
import {
  validateRequiredFields,
  validateStringField,
  validateEnumField,
  validateNumberField,
  validateObjectField,
} from "./base-validator.js";

/**
 * Verify node template configuration
 * @param config Configuration object
 * @returns Verification result
 */
export function validateNodeTemplateConfig(
  config: ConfigFile,
): Result<NodeTemplate, ValidationError[]> {
  const template = config as NodeTemplate;
  const errors: ValidationError[] = [];
  const nodeValidator = new NodeValidator();

  // Verify required fields
  errors.push(
    ...validateRequiredFields(
      template as unknown as Record<string, unknown>,
      ["name", "type", "config", "createdAt", "updatedAt"],
      "NodeTemplate",
    ),
  );

  // Verify the name
  if (template.name) {
    errors.push(
      ...validateStringField(template.name, "NodeTemplate.name", {
        minLength: 1,
        maxLength: 100,
      }),
    );
  }

  // Verify type
  if (template.type) {
    errors.push(
      ...validateEnumField(template.type, "NodeTemplate.type", [
        "START",
        "END",
        "VARIABLE",
        "FORK",
        "JOIN",
        "SUBGRAPH",
        "SCRIPT",
        "LLM",
        "ADD_TOOL",
        "USER_INTERACTION",
        "ROUTE",
        "CONTEXT_PROCESSOR",
        "LOOP_START",
        "LOOP_END",
        "START_FROM_TRIGGER",
        "CONTINUE_FROM_TRIGGER",
      ]),
    );
  }

  // Verify the description.
  if (template.description !== undefined) {
    errors.push(
      ...validateStringField(template.description, "NodeTemplate.description", {
        maxLength: 500,
      }),
    );
  }

  // Verify the timestamp
  if (template.createdAt !== undefined) {
    errors.push(
      ...validateNumberField(template.createdAt, "NodeTemplate.createdAt", {
        integer: true,
        min: 0,
      }),
    );
  }

  if (template.updatedAt !== undefined) {
    errors.push(
      ...validateNumberField(template.updatedAt, "NodeTemplate.updatedAt", {
        integer: true,
        min: 0,
      }),
    );
  }

  // Verify metadata
  if (template.metadata !== undefined) {
    errors.push(...validateObjectField(template.metadata, "NodeTemplate.metadata"));
  }

  // Verify node configuration - Entrusted to NodeValidator
  if (template.config) {
    // Create a temporary node object to verify configuration
    // Use type assertions because this is a temporary verification object
    const tempNode = {
      id: "temp-node-id",
      type: template.type,
      name: template.name,
      description: template.description,
      config: template.config,
      metadata: template.metadata,
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    } as Node;

    const configResult = nodeValidator.validateNode(tempNode);
    if (configResult.isErr()) {
      errors.push(...configResult.error);
    }
  }

  if (errors.length > 0) {
    return err(errors);
  }

  return ok(template);
}
