import { conditionLexer } from "./tokens.js";
import type { IToken, ILexingResult } from "chevrotain";

/**
 * Lexer for condition expressions
 */
export class ConditionLexer {
  /**
   * Tokenize an expression string
   */
  tokenize(expression: string): ILexingResult {
    return conditionLexer.tokenize(expression);
  }

  /**
   * Check if tokenization has errors
   */
  hasErrors(result: ILexingResult): boolean {
    return result.errors.length > 0;
  }

  /**
   * Get formatted error messages
   */
  formatErrors(result: ILexingResult): string[] {
    return result.errors.map(err => `Lexer error at offset ${err.offset}: ${err.message}`);
  }

  /**
   * Get tokens with position information
   */
  getTokens(expression: string): IToken[] {
    const result = this.tokenize(expression);
    if (this.hasErrors(result)) {
      throw new Error(`Lexer errors: ${this.formatErrors(result).join(", ")}`);
    }
    return result.tokens;
  }
}

// Export singleton instance
export const conditionLexerInstance = new ConditionLexer();
