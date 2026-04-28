/**
 * TomlParserManager - Manages the lifecycle of the TOML parser.
 *
 * Provides functionality:
 * - Preload mode (preloaded during SDK initialization)
 * - Singleton pattern ensuring thread safety
 * - Test-friendly reset capability
 * - Clear error handling
 *
 * Usage example:
 *   const parser = TomlParserManager.getInstance();
 *   const config = parser.parse(tomlContent);
 *   // ... After usage ...
 *   TomlParserManager.dispose();
 *
 * Notes:
 *   - getInstance() is a synchronous method and must be called after SDK initialization.
 *   - The initialize() method is automatically called during SDK initialization for preloading.
 */

import { ConfigurationError } from "@wf-agent/types";

/**
 * TomlParserManager - Manages the lifecycle of the TOML parser
 */
export class TomlParserManager {
  private static instance: unknown = null;
  private static initializationPromise: Promise<unknown> | null = null;

  /**
   * Get a singleton instance of the TOML parser (synchronous)
   * Must be called after SDK initialization (pre-loaded)
   * @returns An instance of the TOML parser
   * @throws {ConfigurationError} Throws when the TOML parser is not initialized
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  static getInstance(): any {
    if (!TomlParserManager.instance) {
      throw new ConfigurationError(
        "The TOML parser is not initialized. Please make sure that the SDK is properly initialized.",
        undefined,
        { suggestion: "The TOML parser is automatically preloaded when the SDK is initialized." },
      );
    }
    return TomlParserManager.instance;
  }

  /**
   * Asynchronously initialize the parser (called during SDK bootstrap)
   * @returns Promise that resolves when the parser is initialized
   * @throws {ConfigurationError} Throws when the TOML parsing library is not found
   */
  static async initialize(): Promise<void> {
    if (!TomlParserManager.instance && !TomlParserManager.initializationPromise) {
      TomlParserManager.initializationPromise = (async () => {
        try {
          TomlParserManager.instance = await import("@iarna/toml");
        } catch {
          throw new ConfigurationError(
            "TOML parsing library not found. Make sure you have @iarna/toml installed: pnpm install",
            undefined,
            { suggestion: "pnpm install @iarna/toml" },
          );
        }
      })();
    }
    await TomlParserManager.initializationPromise;
  }

  /**
   * Check if the parser instance exists.
   * @returns Returns true if the parser has been initialized.
   */
  static hasInstance(): boolean {
    return TomlParserManager.instance !== null;
  }

  /**
   * Release parser instance
   * After calling, getInstance() will create a new instance
   */
  static dispose(): void {
    TomlParserManager.instance = null;
    TomlParserManager.initializationPromise = null;
  }

  /**
   * Parse TOML content (synchronous)
   * A convenient method to obtain a parser and perform parsing in one go
   * @param content - The TOML content as a string
   * @returns The parsed object
   * @throws {ConfigurationError} Throws when the TOML parser is not initialized
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  static parse(content: string): any {
    const parser = TomlParserManager.getInstance();
    return parser.parse(content);
  }
}
