/**
 * PromptTemplateLoader - A prompt template configuration loader responsible for loading and merging prompt template configurations
 *
 * Features:
 * - Supports parsing of configuration files in TOML and JSON formats
 * - Implements configuration merging logic, with application-layer configurations taking precedence
 * - Falls back to default templates (from @wf-agent/prompt-templates) when necessary
 *
 * Design Principles:
 * - Does not involve any file I/O operations; it only processes the configuration content
 * - Configuration merging follows the principle of "application-layer configurations overriding default templates"
 * - Ensures immutability by not modifying the original template objects
 *
 */

import type { PromptTemplate, VariableDefinition } from "@wf-agent/prompt-templates";
import { ConfigFormat } from "./types.js";
import type { PromptTemplateConfigFile } from "./types.js";
import { parseToml } from "./toml-parser.js";
import { parseJson } from "./json-parser.js";
import { ConfigurationError } from "@wf-agent/types";

/**
 * Load prompt word template configuration
 *
 * @param content: The content of the configuration file
 * @param format: The configuration format (toml or json)
 * @returns: The parsed configuration object
 * @throws {ConfigurationError}: Throws when the configuration parsing fails
 */
export function loadPromptTemplateConfig(
  content: string,
  format: ConfigFormat,
): PromptTemplateConfigFile {
  let config: unknown;

  try {
    // Select a parser based on the format.
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

    // Verify required fields
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
        {
          originalError: error.message,
        },
      );
    }
    throw new ConfigurationError(
      "Prompt word template configuration parsing failed: Unknown error",
    );
  }
}

/**
 * Merge prompt template configuration
 * Merges application-layer configuration with default template, with application-layer configuration taking precedence
 *
 * @param defaultTemplate Default template (from @wf-agent/prompt-templates)
 * @param appConfig Application-layer configuration
 * @returns Merged template
 *
 * @example
 * ```ts
 * const defaultTemplate = CODER_SYSTEM_TEMPLATE;
 * const appConfig = loadPromptTemplateConfig(content, 'toml');
 * const mergedTemplate = mergePromptTemplateConfig(defaultTemplate, appConfig);
 * ```
 */
export function mergePromptTemplateConfig(
  defaultTemplate: PromptTemplate,
  appConfig: PromptTemplateConfigFile,
): PromptTemplate {
  // Verify whether the configuration ID matches.
  if (appConfig.id !== defaultTemplate.id) {
    throw new ConfigurationError(
      `Configuration ID mismatch: The ID in the config file is '${appConfig.id}', but the default template ID is '${defaultTemplate.id}'`,
      "prompt_template",
    );
  }

  // Merge configurations, with application layer configuration taking precedence.
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

/**
 * Merge variable definitions
 * Application-layer variables will override variables with the same name in the default template.
 *
 * @param defaultVariables Default variable definitions
 * @param appVariables Application-layer variable definitions
 * @returns Merged variable definitions
 */
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

  // Create a variable mapping with application layer variables taking precedence.
  const variableMap = new Map<string, VariableDefinition>();

  // First, add default variables.
  for (const variable of defaultVariables) {
    variableMap.set(variable.name, variable);
  }

  // Override or add application-layer variables
  for (const variable of appVariables) {
    variableMap.set(variable.name, variable);
  }

  return Array.from(variableMap.values());
}

/**
 * Merge the fragment lists
 * The application layer fragments will be appended to the default fragment list.
 *
 * @param defaultFragments: The default fragment list
 * @param appFragments: The application layer fragment list
 * @returns: The merged fragment list
 */
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

  // Merge the fragments and remove duplicates.
  const fragmentSet = new Set([...defaultFragments, ...appFragments]);
  return Array.from(fragmentSet);
}

/**
 * Load and merge prompt templates
 * A convenient method to complete loading and merging in one go
 *
 * @param content: Content of the configuration file
 * @param format: Configuration format
 * @param defaultTemplate: Default template
 * @returns: The merged template
 */
export function loadAndMergePromptTemplate(
  content: string,
  format: ConfigFormat,
  defaultTemplate: PromptTemplate,
): PromptTemplate {
  const appConfig = loadPromptTemplateConfig(content, format);
  return mergePromptTemplateConfig(defaultTemplate, appConfig);
}
