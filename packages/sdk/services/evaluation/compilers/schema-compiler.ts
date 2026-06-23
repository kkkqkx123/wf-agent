/**
 * Schema Compiler
 * Compiles JSON Schema for validation
 * Note: Caching is handled by CacheManager, not by this class
 */

import type { ICompiler, CompiledUnit } from "../types/index.js";

export class SchemaCompiler implements ICompiler {
  compile(input: string | Record<string, unknown>): CompiledUnit {
    if (typeof input === "string") {
      throw new Error("Schema compiler expects object input");
    }

    const schema = input as Record<string, unknown>;

    const unit: CompiledUnit = {
      ast: schema,
      dependencies: [],
      complexity: this.calculateSchemaComplexity(schema),
      metadata: {
        type: "schema",
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

  private calculateSchemaComplexity(schema: Record<string, unknown>, depth: number = 0): number {
    if (depth > 10) return 100;

    let complexity = 1;

    const properties = schema["properties"];
    if (properties && typeof properties === "object") {
      const props = Object.keys(properties as Record<string, unknown>);
      complexity += props.length * 2;

      for (const prop of props) {
        const propSchema = (properties as Record<string, unknown>)[prop];
        if (propSchema && typeof propSchema === "object") {
          complexity += this.calculateSchemaComplexity(propSchema as Record<string, unknown>, depth + 1);
        }
      }
    }

    const items = schema["items"];
    if (items && typeof items === "object") {
      complexity += 2 + this.calculateSchemaComplexity(items as Record<string, unknown>, depth + 1);
    }

    const oneOf = schema["oneOf"];
    if (oneOf && Array.isArray(oneOf)) {
      complexity += oneOf.length * 2;
    }

    const anyOf = schema["anyOf"];
    if (anyOf && Array.isArray(anyOf)) {
      complexity += anyOf.length * 2;
    }

    const allOf = schema["allOf"];
    if (allOf && Array.isArray(allOf)) {
      complexity += allOf.length * 2;
    }

    return complexity;
  }
}

export const schemaCompiler = new SchemaCompiler();
