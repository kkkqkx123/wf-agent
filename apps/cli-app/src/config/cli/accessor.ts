/**
 * CLI Configuration Accessor
 * Provides convenient configuration access API.
 */

import type { CLIConfig } from "./types.js";
import { ConfigValidator } from "../config-validator.js";

/**
 * CLI Configuration Accessor
 * Wraps CLIConfig with convenient getter methods.
 */
export class CLIConfigAccessor {
  private config: CLIConfig;

  constructor(config: CLIConfig) {
    this.config = config;
  }

  /**
   * Get storage configuration.
   */
  getStorageConfig() {
    return this.config.storage;
  }

  /**
   * Get JSON storage configuration.
   */
  getJsonStorageConfig() {
    return this.config.storage?.json;
  }

  /**
   * Get SQLite storage configuration.
   */
  getSqliteStorageConfig() {
    return this.config.storage?.sqlite;
  }

  /**
   * Get storage base directory.
   */
  getStorageBaseDir(): string {
    if (this.config.storage?.type === "json" && this.config.storage.json) {
      return this.config.storage.json.baseDir;
    }
    if (this.config.storage?.type === "sqlite" && this.config.storage.sqlite) {
      const dbPath = this.config.storage.sqlite.dbPath;
      return dbPath.substring(0, dbPath.lastIndexOf("/") + 1);
    }
    return "./storage";
  }

  /**
   * Get output configuration.
   */
  getOutputConfig() {
    return this.config.output;
  }

  /**
   * Get output directory.
   */
  getOutputDir(): string {
    return this.config.output?.dir || "./outputs";
  }

  /**
   * Get log file pattern.
   */
  getLogFilePattern(): string {
    return this.config.output?.logFilePattern || "cli-app-{date}.log";
  }

  /**
   * Check if log terminal is enabled.
   */
  isLogTerminalEnabled(): boolean {
    return this.config.output?.enableLogTerminal ?? true;
  }

  /**
   * Check if SDK logs are enabled.
   */
  isSDKLogsEnabled(): boolean {
    return this.config.output?.enableSDKLogs ?? true;
  }

  /**
   * Get SDK log level.
   */
  getSDKLogLevel(): string {
    return this.config.output?.sdkLogLevel || "silent";
  }

  /**
   * Get presets configuration.
   */
  getPresetsConfig() {
    return this.config.presets;
  }

  /**
   * Get context compression preset configuration.
   */
  getContextCompressionConfig() {
    return this.config.presets?.contextCompression;
  }

  /**
   * Check if context compression is enabled.
   */
  isContextCompressionEnabled(): boolean {
    return this.config.presets?.contextCompression?.enabled ?? true;
  }

  /**
   * Get predefined tools preset configuration.
   */
  getPredefinedToolsConfig() {
    return this.config.presets?.predefinedTools;
  }

  /**
   * Check if predefined tools are enabled.
   */
  isPredefinedToolsEnabled(): boolean {
    return this.config.presets?.predefinedTools?.enabled ?? true;
  }

  /**
   * Get predefined prompts preset configuration.
   */
  getPredefinedPromptsConfig() {
    return this.config.presets?.predefinedPrompts;
  }

  /**
   * Check if predefined prompts are enabled.
   */
  isPredefinedPromptsEnabled(): boolean {
    return this.config.presets?.predefinedPrompts?.enabled ?? true;
  }

  /**
   * Get the full configuration object.
   */
  getFullConfig(): CLIConfig {
    return this.config;
  }

  /**
   * Get a specific configuration value by key.
   */
  get<K extends keyof CLIConfig>(key: K): CLIConfig[K] {
    return this.config[key];
  }

  /**
   * Validate the current configuration.
   */
  validate(): { valid: boolean; errors: string[] } {
    return ConfigValidator.validate(this.config);
  }

  /**
   * Validate the current configuration and throw if invalid.
   */
  validateOrThrow(): void {
    ConfigValidator.validateOrThrow(this.config);
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
