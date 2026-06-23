/**
 * Script Compiler
 * Compiles JavaScript script strings into executable functions
 * with restricted access to prevent malicious execution
 * Note: Caching is handled by CacheManager, not by this class
 */

import type { ICompiler, CompiledUnit } from "../types/index.js";
import { ExpressionSecurityError } from "@wf-agent/types";

export class ScriptCompiler implements ICompiler {
  private readonly DANGEROUS_PATTERNS = [
    /require\s*\(/i,
    /import\s+/i,
    /eval\s*\(/i,
    /Function\s*\(/i,
    /setTimeout\s*\(/i,
    /setInterval\s*\(/i,
    /process\s*\./i,
    /global\s*\./i,
    /window\s*\./i,
  ];

  compile(input: string | Record<string, unknown>): CompiledUnit {
    if (typeof input !== "string") {
      throw new Error("Script compiler expects string input");
    }

    const script = input as string;

    this.validateScript(script);

    let scriptFunction: (variables: Record<string, unknown>, input: unknown, output: unknown) => unknown;
    try {
      scriptFunction = new Function(
        "variables",
        "input",
        "output",
        `return (${script})`,
      ) as (variables: Record<string, unknown>, input: unknown, output: unknown) => unknown;
    } catch (error) {
      throw new ExpressionSecurityError(
        `Failed to compile script: ${error instanceof Error ? error.message : String(error)}`,
        { operation: "script_compilation", field: "script", value: script },
      );
    }

    const unit: CompiledUnit = {
      ast: { scriptFunction },
      dependencies: this.extractDependencies(script),
      complexity: 10,
      metadata: {
        type: "script",
        script: script.length > 100 ? script.substring(0, 100) : script,
      },
    };

    return unit;
  }

  clearCache(): void {
    // Caching is handled by CacheManager, nothing to clear here
  }

  getCacheSize(): number {
    // Caching is handled by CacheManager
    return 0;
  }

  private validateScript(script: string): void {
    if (script.length > 10000) {
      throw new ExpressionSecurityError("Script exceeds maximum length of 10000 characters", {
        operation: "script_validation",
        field: "script",
        value: "too long",
      });
    }

    for (const pattern of this.DANGEROUS_PATTERNS) {
      if (pattern.test(script)) {
        throw new ExpressionSecurityError(
          "Script contains forbidden patterns (require/import/eval/process/window)",
          { operation: "script_validation", field: "script", value: script.substring(0, 50) },
        );
      }
    }
  }

  private extractDependencies(script: string): string[] {
    const deps = new Set<string>();

    const varPattern = /\b(variables|input|output)\s*\.\s*(\w+)/g;
    let match;

    while ((match = varPattern.exec(script)) !== null) {
      const scope = match[1];
      const name = match[2];
      if (scope === "variables" && name) {
        deps.add(name);
      }
    }

    return Array.from(deps);
  }
}

export const scriptCompiler = new ScriptCompiler();
