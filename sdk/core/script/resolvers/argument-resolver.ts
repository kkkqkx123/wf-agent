/**
 * Argument Resolver
 * Resolves script arguments with defaults, type validation, and variable injection
 */

import type { ScriptArgument } from "@wf-agent/types";

/**
 * Argument Resolver
 * Handles argument resolution from multiple sources (static, variable, expression)
 */
export class ArgumentResolver {
  /**
   * Resolve argument values from provided context
   * @param args Argument declarations
   * @param providedArgs Direct argument values
   * @param contextVariables Workflow context variables for variable source resolution
   * @returns Resolved argument map
   */
  resolve(
    args: ScriptArgument[],
    providedArgs: Record<string, unknown> = {},
    contextVariables: Record<string, unknown> = {},
  ): Record<string, unknown> {
    const resolved: Record<string, unknown> = {};

    for (const arg of args) {
      let value: unknown;

      if (providedArgs[arg.key] !== undefined) {
        value = providedArgs[arg.key];
      } else if (arg.source === "variable" && contextVariables[arg.key] !== undefined) {
        value = contextVariables[arg.key];
      } else if (arg.default !== undefined) {
        value = arg.default;
      } else if (arg.required) {
        throw new Error(`Required argument '${arg.key}' is not provided and has no default`);
      }

      if (value !== undefined) {
        this.validateArgument(arg, value);
        resolved[arg.key] = value;
      }
    }

    return resolved;
  }

  /**
   * Validate a single argument value against its declaration
   */
  private validateArgument(arg: ScriptArgument, value: unknown): void {
    if (arg.type === "number" && typeof value !== "number") {
      throw new Error(`Argument '${arg.key}' must be a number, got ${typeof value}`);
    }
    if (arg.type === "boolean" && typeof value !== "boolean") {
      throw new Error(`Argument '${arg.key}' must be a boolean, got ${typeof value}`);
    }
    if (arg.type === "file" && typeof value !== "string") {
      throw new Error(`Argument '${arg.key}' must be a file path string, got ${typeof value}`);
    }
    if (arg.options && Array.isArray(arg.options) && !arg.options.includes(value)) {
      throw new Error(
        `Argument '${arg.key}' must be one of: ${arg.options.join(", ")}`,
      );
    }
    if (arg.pattern && typeof value === "string") {
      const regex = new RegExp(arg.pattern);
      if (!regex.test(value)) {
        throw new Error(`Argument '${arg.key}' does not match required pattern`);
      }
    }
  }
}