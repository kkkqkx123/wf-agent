/**
 * Script Template Engine
 * Renders command templates by injecting runtime arguments
 */

import { renderTemplate, getUnresolvedPlaceholders } from "../../../shared/utils/template-renderer/index.js";
import type { ScriptArgument } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "ScriptTemplateEngine" });

/**
 * Result of template rendering
 */
export interface TemplateRenderResult {
  /** The rendered command string */
  command: string;
  /** Whether all placeholders were resolved */
  resolved: boolean;
  /** Any unresolved placeholders that were left as-is */
  unresolvedPlaceholders: string[];
}

/**
 * Script Template Engine
 * Handles template rendering and argument validation
 */
export class ScriptTemplateEngine {
  /**
   * Resolve arguments by merging provided values with defaults
   * @param args Argument declarations
   * @param providedArgs Runtime-provided argument values
   * @returns Merged argument map
   */
  resolveArguments(
    args: ScriptArgument[],
    providedArgs: Record<string, unknown> = {},
  ): Record<string, unknown> {
    const resolved: Record<string, unknown> = {};

    for (const arg of args) {
      if (providedArgs[arg.key] !== undefined) {
        resolved[arg.key] = providedArgs[arg.key];
      } else if (arg.default !== undefined) {
        resolved[arg.key] = arg.default;
      } else if (arg.required) {
        throw new Error(`Required argument '${arg.key}' is missing`);
      }
    }

    return resolved;
  }

  /**
   * Validate argument values against their type declarations
   * @param args Argument declarations
   * @param values Argument values to validate
   */
  validateArguments(args: ScriptArgument[], values: Record<string, unknown>): void {
    for (const arg of args) {
      const value = values[arg.key];
      if (value === undefined) {
        if (arg.required) {
          throw new Error(`Required argument '${arg.key}' is missing`);
        }
        continue;
      }

      if (arg.type === "number" && typeof value !== "number") {
        throw new Error(`Argument '${arg.key}' must be a number`);
      }
      if (arg.type === "boolean" && typeof value !== "boolean") {
        throw new Error(`Argument '${arg.key}' must be a boolean`);
      }
      if (arg.type === "file" && typeof value !== "string") {
        throw new Error(`Argument '${arg.key}' must be a file path string`);
      }
      if (arg.options && Array.isArray(arg.options)) {
        if (!arg.options.includes(value)) {
          throw new Error(`Argument '${arg.key}' must be one of: ${arg.options.join(", ")}`);
        }
      }
      if (arg.pattern && typeof value === "string") {
        const regex = new RegExp(arg.pattern);
        if (!regex.test(value)) {
          throw new Error(`Argument '${arg.key}' does not match pattern: ${arg.pattern}`);
        }
      }
    }
  }

  /**
   * Render a template with provided variable bindings
   * @param template The template string with {{var}} placeholders
   * @param variables Variable values to inject
   * @returns Rendered result
   */
  render(template: string, variables: Record<string, unknown>): TemplateRenderResult {
    if (!template) {
      return { command: "", resolved: true, unresolvedPlaceholders: [] };
    }

    try {
      const command = renderTemplate(template, variables);
      const unresolvedPlaceholders = getUnresolvedPlaceholders(command);

      return {
        command,
        resolved: unresolvedPlaceholders.length === 0,
        unresolvedPlaceholders,
      };
    } catch (error) {
      logger.error("Template rendering failed", {
        template,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  }
}
