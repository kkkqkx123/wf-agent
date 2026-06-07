/**
 * PromptTemplate Configuration Processing Functions
 *
 * Pure data-processing layer:
 *  - parse:       raw string → typed PromptTemplateConfigFile
 *  - validate:    typed config → Result<ValidationError[]>
 *  - merge:       defaultTemplate + appConfig → final PromptTemplate
 *  - export:      final template → serialisation-ready object
 *
 * This module never touches file I/O.  File I/O orchestration belongs
 * in the application layer (apps/config-processor).
 */

import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";
import type { ParsedConfig } from "../types.js";
import { ConfigFormat } from "../types.js";
import type { PromptTemplateConfigFile } from "../types.js";
import type { Result, PromptTemplate, PromptVariableDefinition } from "@wf-agent/types";
import { ValidationError, ConfigurationValidationError, ConfigurationError } from "@wf-agent/types";
import { PromptTemplateSchema } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";

// ---------------------------------------------------------------------------
// Parse — raw string → typed config record
// ---------------------------------------------------------------------------

/**
 * Parse raw prompt-template file content into a typed config record.
 *
 * @param content - Raw file content (TOML or JSON string).
 * @param format - Expected format (toml | json).
 * @returns The parsed and structurally validated configuration object.
 * @throws {ConfigurationError} When parsing fails or required fields are missing.
 */
export function parsePromptTemplateConfig(
  content: string,
  format: ConfigFormat,
): PromptTemplateConfigFile {
  let config: unknown;

  try {
    switch (format) {
      case "toml":
        config = parseToml(content);
        break;
      case "json":
        config = parseJson(content);
        break;
      default:
        throw new ConfigurationError(`Unsupported configuration format: ${format}`, format);
    }

    const configRecord = config as Record<string, unknown>;
    if (!configRecord["id"]) {
      throw new ConfigurationError(
        "The prompt word template configuration must include an id field.",
        "prompt_template",
      );
    }

    return config as PromptTemplateConfigFile;
  } catch (error) {
    if (error instanceof ConfigurationError) {
      throw error;
    }
    if (error instanceof Error) {
      throw new ConfigurationError(
        `Prompt template configuration parsing failed: ${error.message}`,
        "prompt_template",
        { originalError: error.message },
      );
    }
    throw new ConfigurationError(
      "Prompt word template configuration parsing failed: Unknown error",
    );
  }
}

// ---------------------------------------------------------------------------
// Validate — Zod-schema validation
// ---------------------------------------------------------------------------

/**
 * Verify PromptTemplate configuration using Zod schema.
 */
export function validatePromptTemplate(
  config: ParsedConfig<"prompt_template">,
): Result<ParsedConfig<"prompt_template">, ValidationError[]> {
  const cfg = config.config as PromptTemplateConfigFile;

  const result = PromptTemplateSchema.safeParse(cfg);

  if (!result.success) {
    const errors = result.error.issues.map(
      (e) =>
        new ConfigurationValidationError(e.message, {
          configType: "schema",
          field: e.path.join("."),
        }),
    );
    return err(errors);
  }

  return ok(config);
}

// ---------------------------------------------------------------------------
// Merge — default + application config → final template
// ---------------------------------------------------------------------------

/**
 * Merge prompt template configuration.
 *
 * Application-layer configuration overrides default template values.
 *
 * @param defaultTemplate - Default template (e.g. from @wf-agent/prompt-templates).
 * @param appConfig - Application-layer configuration parsed from file.
 * @returns Merged template.
 */
export function mergePromptTemplateConfig(
  defaultTemplate: PromptTemplate,
  appConfig: PromptTemplateConfigFile,
): PromptTemplate {
  if (appConfig.id !== defaultTemplate.id) {
    throw new ConfigurationError(
      `Configuration ID mismatch: The ID in the config file is '${appConfig.id}', but the default template ID is '${defaultTemplate.id}'`,
      "prompt_template",
    );
  }

  const merged: PromptTemplate = {
    id: defaultTemplate.id,
    name: appConfig.name ?? defaultTemplate.name,
    description: appConfig.description ?? defaultTemplate.description,
    category: appConfig.category ?? defaultTemplate.category,
    content: appConfig.content ?? defaultTemplate.content,
    variables: mergeVariables(defaultTemplate.variables, appConfig.variables),
    fragments: mergeFragments(defaultTemplate.fragments, appConfig.fragments),
  };

  return merged;
}

function mergeVariables(
  defaultVariables: PromptTemplate["variables"],
  appVariables: PromptTemplateConfigFile["variables"],
): PromptTemplate["variables"] {
  if (!appVariables || appVariables.length === 0) {
    return defaultVariables;
  }
  if (!defaultVariables || defaultVariables.length === 0) {
    return appVariables;
  }

  const variableMap = new Map<string, PromptVariableDefinition>();
  for (const variable of defaultVariables) {
    variableMap.set(variable.name, variable);
  }
  for (const variable of appVariables) {
    variableMap.set(variable.name, variable);
  }
  return Array.from(variableMap.values());
}

function mergeFragments(
  defaultFragments: PromptTemplate["fragments"],
  appFragments: PromptTemplateConfigFile["fragments"],
): PromptTemplate["fragments"] {
  if (!appFragments || appFragments.length === 0) {
    return defaultFragments;
  }
  if (!defaultFragments || defaultFragments.length === 0) {
    return appFragments;
  }
  const fragmentSet = new Set([...defaultFragments, ...appFragments]);
  return Array.from(fragmentSet);
}

// ---------------------------------------------------------------------------
// Orchestration convenience (no file I/O)
// ---------------------------------------------------------------------------

/**
 * Parse + merge in one call.
 *
 * Accepts raw content (string), not a file path, so it remains pure and
 * testable without mocking the filesystem.
 *
 * @param content - Raw file content.
 * @param format - Configuration format (toml | json).
 * @param defaultTemplate - Default template.
 * @returns The merged template.
 */
export function loadAndMergePromptTemplate(
  content: string,
  format: ConfigFormat,
  defaultTemplate: PromptTemplate,
): PromptTemplate {
  const appConfig = parsePromptTemplateConfig(content, format);
  return mergePromptTemplateConfig(defaultTemplate, appConfig);
}

// ---------------------------------------------------------------------------
// Export — typed → serialisation-ready
// ---------------------------------------------------------------------------

/**
 * Export PromptTemplate configuration.
 * Returns typed data ready for serialization.
 */
export function transformPromptTemplate(
  config: ParsedConfig<"prompt_template">,
  defaultTemplate: PromptTemplate,
): PromptTemplate {
  return mergePromptTemplateConfig(defaultTemplate, config.config);
}

/**
 * Export PromptTemplate configuration.
 */
export function exportPromptTemplate(template: PromptTemplate): PromptTemplate {
  return template;
}