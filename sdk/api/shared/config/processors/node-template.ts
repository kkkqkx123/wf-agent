/**
 * NodeTemplate Configuration Processing Functions
 * Provide functions for validating, converting, and exporting NodeTemplate configurations.
 * All functions are stateless pure functions.
 */

import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { Result } from "@wf-agent/types";
import { ValidationError, ConfigurationError } from "@wf-agent/types";
import { NodeValidator } from "../../../../workflow/validation/node-validator.js";
import {
  validateRequiredFields,
  validateStringField,
  validateEnumField,
  validateNumberField,
  validateObjectField,
} from "../validators/validation-helpers.js";
import { ok, err } from "@wf-agent/common-utils";
import type { NodeTemplate } from "@wf-agent/types";
import { stringifyJson } from "../json-parser.js";
import { substituteParameters } from "../config-utils.js";

/**
 * Verify NodeTemplate configuration
 * @param config The parsed configuration object
 * @returns The verification result
 */
export function validateNodeTemplate(
  config: ParsedConfig<"node_template">,
): Result<ParsedConfig<"node_template">, ValidationError[]> {
  const template = config.config as NodeTemplate;
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
    const tempNode = {
      id: "temp-node-id",
      type: template.type,
      name: template.name,
      description: template.description,
      config: template.config,
      metadata: template.metadata,
      outgoingEdgeIds: [],
      incomingEdgeIds: [],
    } as unknown as Node;

    const configResult = nodeValidator.validateNode(tempNode as any);
    if (configResult.isErr()) {
      errors.push(...configResult.error);
    }
  }

  if (errors.length > 0) {
    return err(errors) as Result<ParsedConfig<"node_template">, ValidationError[]>;
  }

  return ok(config) as Result<ParsedConfig<"node_template">, ValidationError[]>;
}

/**
 * Translate NodeTemplate configuration
 * Handle parameter substitution (if applicable)
 * @param config The parsed configuration object
 * @param parameters Runtime parameters (optional)
 * @returns The transformed NodeTemplate
 */
export function transformNodeTemplate(
  config: ParsedConfig<"node_template">,
  parameters?: Record<string, unknown>,
): NodeTemplate {
  let template = config.config;

  // If parameters are provided, perform parameter substitution
  if (parameters && Object.keys(parameters).length > 0) {
    template = substituteParameters(template, parameters);
  }

  return template;
}

/**
 * Export NodeTemplate configuration
 * @param template NodeTemplate object
 * @param format configuration format
 * @returns string containing the configuration file content
 */
export function exportNodeTemplate(template: NodeTemplate, format: ConfigFormat): string {
  switch (format) {
    case "json":
      return stringifyJson(template, true);
    case "toml":
      throw new ConfigurationError(
        "TOML format does not support export, please use JSON format",
        format,
        {
          suggestion: "Use json instead of",
        },
      );
    default:
      throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
  }
}
