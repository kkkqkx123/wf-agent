/**
 * Script Condition Executor
 * Executes compiled JavaScript scripts with sandboxed access
 */

import type { EvaluationContext } from "@wf-agent/types";
import { ExpressionSecurityError, RuntimeValidationError } from "@wf-agent/types";
import { BaseExecutor } from "../base-executor.js";
import type { CompiledUnit, IExecutor } from "../types/index.js";

export class ScriptExecutor extends BaseExecutor implements IExecutor {
  execute(compiled: CompiledUnit, context: EvaluationContext): unknown {
    this.validateContext(context);

    const ast = compiled.ast as Record<string, unknown>;
    const { scriptFunction } = ast;

    if (typeof scriptFunction !== "function") {
      throw new Error("Compiled script missing function");
    }

    try {
      const result = scriptFunction(context.variables, context.input, context.output);

      if (typeof result !== "boolean" && typeof result !== "number" && typeof result !== "string") {
        throw new RuntimeValidationError("Script must return a boolean or truthy/falsy value", {
          operation: "script_execution",
          field: "result",
          value: result,
        });
      }

      return result;
    } catch (error) {
      if (error instanceof ExpressionSecurityError || error instanceof RuntimeValidationError) {
        throw error;
      }

      throw new RuntimeValidationError(
        `Script execution failed: ${error instanceof Error ? error.message : String(error)}`,
        {
          operation: "script_execution",
          field: "script",
          value: "error during execution",
        },
      );
    }
  }
}

export const scriptExecutor = new ScriptExecutor();
