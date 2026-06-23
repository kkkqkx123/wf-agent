/**
 * Compiler Interface and Types
 * Defines the contract for all condition compilers
 */

export interface CompiledUnit {
  /** Abstract syntax tree representation */
  ast: unknown;
  /** Variable/field dependencies */
  dependencies?: string[];
  /** Complexity score for performance warnings */
  complexity?: number;
  /** Compiler-specific metadata */
  metadata?: Record<string, unknown>;
}

export interface ICompiler {
  /**
   * Compile input into executable form
   * @param input Source code or configuration
   * @returns Compiled unit with execution metadata
   */
  compile(input: string | Record<string, unknown>): CompiledUnit;

  /**
   * Clear compilation cache
   */
  clearCache(): void;

  /**
   * Get cache size
   */
  getCacheSize(): number;
}
