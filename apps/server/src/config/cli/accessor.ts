/**
 * CLI Configuration Accessor
 * Provides convenient configuration access API.
 *
 * Refactored to use SDK base accessor pattern.
 */

import type { CLIConfig } from "./types.js";
import { ConfigValidator } from "../config-validator.js";
import { createConfigAccessor, type ConfigAccessor } from "@wf-agent/sdk/api";

/**
 * CLI Configuration Accessor
 * Wraps CLIConfig with convenient getter methods.
 * Extends SDK base accessor pattern with CLI-specific functionality.
 */
export class CLIConfigAccessor {
  private accessor: ConfigAccessor<CLIConfig>;

  constructor(config: CLIConfig) {
    this.accessor = createConfigAccessor(config);
  }

  /**
   * Get the underlying SDK accessor for generic operations.
   */
  getAccessor(): ConfigAccessor<CLIConfig> {
    return this.accessor;
  }

  /**
   * Get storage configuration.
   */
  getStorageConfig() {
    return this.accessor.get().storage;
  }

  /**
   * Get JSON storage configuration.
   */
  getJsonStorageConfig() {
    return this.accessor.get().storage?.json;
  }

  /**
   * Get SQLite storage configuration.
   */
  getSqliteStorageConfig() {
    return this.accessor.get().storage?.sqlite;
  }

  /**
   * Get storage base directory.
   */
  getStorageBaseDir(): string {
    const config = this.accessor.get();
    if (config.storage?.type === "json" && config.storage.json) {
      return config.storage.json.baseDir;
    }
    if (config.storage?.type === "sqlite" && config.storage.sqlite) {
      const dbPath = config.storage.sqlite.dbPath;
      return dbPath.substring(0, dbPath.lastIndexOf("/") + 1);
    }
    return "./storage";
  }

  /**
   * Get output configuration.
   */
  getOutputConfig() {
    return this.accessor.get().output;
  }

  /**
   * Get output directory.
   */
  getOutputDir(): string {
    return this.accessor.get().output?.dir || "./outputs";
  }

  /**
   * Get log file pattern.
   */
  getLogFilePattern(): string {
    return this.accessor.get().output?.logFilePattern || "cli-app-{date}.log";
  }

  /**
   * Check if log terminal is enabled.
   */
  isLogTerminalEnabled(): boolean {
    return this.accessor.get().output?.enableLogTerminal ?? true;
  }

  /**
   * Check if SDK logs are enabled.
   */
  isSDKLogsEnabled(): boolean {
    return this.accessor.get().output?.enableSDKLogs ?? true;
  }

  /**
   * Get SDK log level.
   */
  getSDKLogLevel(): string {
    return this.accessor.get().output?.sdkLogLevel || "silent";
  }

  /**
   * Get presets configuration.
   */
  getPresetsConfig() {
    return this.accessor.get().presets;
  }

  /**
   * Get context compression preset configuration.
   */
  getContextCompressionConfig() {
    return this.accessor.get().presets?.contextCompression;
  }

  /**
   * Check if context compression is enabled.
   */
  isContextCompressionEnabled(): boolean {
    return this.accessor.get().presets?.contextCompression?.enabled ?? true;
  }

  /**
   * Get predefined tools preset configuration.
   */
  getPredefinedToolsConfig() {
    return this.accessor.get().presets?.predefinedTools;
  }

  /**
   * Check if predefined tools are enabled.
   */
  isPredefinedToolsEnabled(): boolean {
    return this.accessor.get().presets?.predefinedTools?.enabled ?? true;
  }

  /**
   * Get predefined prompts preset configuration.
   */
  getPredefinedPromptsConfig() {
    return this.accessor.get().presets?.predefinedPrompts;
  }

  /**
   * Check if predefined prompts are enabled.
   */
  isPredefinedPromptsEnabled(): boolean {
    return this.accessor.get().presets?.predefinedPrompts?.enabled ?? true;
  }

  /**
   * Get the full configuration object.
   */
  getFullConfig(): CLIConfig {
    return this.accessor.get();
  }

  /**
   * Get a specific configuration value by key.
   */
  get<K extends keyof CLIConfig>(key: K): CLIConfig[K] {
    return this.accessor.get()[key];
  }

  /**
   * Set the full configuration object.
   */
  setFullConfig(config: CLIConfig): void {
    this.accessor.set(config);
  }

  /**
   * Reset to default configuration.
   */
  reset(): void {
    this.accessor.reset();
  }

  /**
   * Check if accessor has been initialized.
   */
  isInitialized(): boolean {
    return this.accessor.isInitialized();
  }

  /**
   * Validate the current configuration.
   */
  validate(): { valid: boolean; errors: string[] } {
    return ConfigValidator.validate(this.accessor.get());
  }

  /**
   * Validate the current configuration and throw if invalid.
   */
  validateOrThrow(): void {
    ConfigValidator.validateOrThrow(this.accessor.get());
  }
}

/**
 * Global CLI configuration accessor instance.
 */
let globalCLIConfigAccessor: CLIConfigAccessor | null = null;

/**
 * Get the global CLI configuration accessor instance.
 * @param config Optional configuration to initialize with
 * @returns CLI configuration accessor instance
 */
export function getCLIConfigAccessor(config?: CLIConfig): CLIConfigAccessor {
  if (!globalCLIConfigAccessor && config) {
    globalCLIConfigAccessor = new CLIConfigAccessor(config);
  }
  if (!globalCLIConfigAccessor) {
    throw new Error("CLIConfigAccessor not initialized. Call initCLIConfigAccessor first.");
  }
  return globalCLIConfigAccessor;
}

/**
 * Initialize the global CLI configuration accessor.
 * @param config Configuration to initialize with
 */
export function initCLIConfigAccessor(config: CLIConfig): void {
  globalCLIConfigAccessor = new CLIConfigAccessor(config);
}

/**
 * Reset the global CLI configuration accessor.
 */
export function resetCLIConfigAccessor(): void {
  globalCLIConfigAccessor = null;
}
