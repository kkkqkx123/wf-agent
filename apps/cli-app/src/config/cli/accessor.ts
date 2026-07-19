/**
 * CLI Configuration Accessor
 * Provides convenient configuration access API.
 *
 * Extends the base ConfigAccessor from @wf-agent/runtime with
 * CLI-specific getter methods.
 */

import type { CLIConfig } from "./types.js";
import { ConfigValidator } from "../config-validator.js";
import { ConfigAccessor as BaseConfigAccessor } from "@wf-agent/runtime";

/**
 * CLI Configuration Accessor
 * Wraps CLIConfig with convenient getter methods.
 * Extends the base ConfigAccessor from runtime with CLI-specific functionality.
 */
export class CLIConfigAccessor extends BaseConfigAccessor<CLIConfig> {
  constructor(config?: CLIConfig) {
    super(config);
  }

  /**
   * Get the underlying accessor for generic operations.
   */
  getAccessor(): BaseConfigAccessor<CLIConfig> {
    return this;
  }

  /**
   * Get storage configuration.
   */
  getStorageConfig() {
    return this.get().storage;
  }

  /**
   * Get SQLite storage configuration.
   */
  getSqliteStorageConfig() {
    return this.get().storage?.sqlite;
  }

  /**
   * Get storage base directory.
   */
  getStorageBaseDir(): string {
    const config = this.get();
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
    return this.get().output;
  }

  /**
   * Get output directory.
   */
  getOutputDir(): string {
    return this.get().output?.dir || "./outputs";
  }

  /**
   * Get log file pattern.
   */
  getLogFilePattern(): string {
    return this.get().output?.logFilePattern || "cli-app-{date}.log";
  }

  /**
   * Check if log terminal is enabled.
   */
  isLogTerminalEnabled(): boolean {
    return this.get().output?.enableLogTerminal ?? true;
  }

  /**
   * Check if SDK logs are enabled.
   */
  isSDKLogsEnabled(): boolean {
    return this.get().output?.enableSDKLogs ?? true;
  }

  /**
   * Get SDK log level.
   */
  getSDKLogLevel(): string {
    return this.get().output?.sdkLogLevel || "silent";
  }

  /**
   * Get presets configuration.
   */
  getPresetsConfig() {
    return this.get().presets;
  }

  /**
   * Get context compression preset configuration.
   */
  getContextCompressionConfig() {
    return this.get().presets?.contextCompression;
  }

  /**
   * Check if context compression is enabled.
   */
  isContextCompressionEnabled(): boolean {
    return this.get().presets?.contextCompression?.enabled ?? true;
  }

  /**
   * Get predefined tools preset configuration.
   */
  getPredefinedToolsConfig() {
    return this.get().presets?.predefinedTools;
  }

  /**
   * Check if predefined tools are enabled.
   */
  isPredefinedToolsEnabled(): boolean {
    return this.get().presets?.predefinedTools?.enabled ?? true;
  }

  /**
   * Get predefined prompts preset configuration.
   */
  getPredefinedPromptsConfig() {
    return this.get().presets?.predefinedPrompts;
  }

  /**
   * Check if predefined prompts are enabled.
   */
  isPredefinedPromptsEnabled(): boolean {
    return this.get().presets?.predefinedPrompts?.enabled ?? true;
  }

  /**
   * Get the full configuration object.
   */
  getFullConfig(): CLIConfig {
    return this.get();
  }

  /**
   * Get a specific configuration value by key.
   */
  getValue<K extends keyof CLIConfig>(key: K): CLIConfig[K] {
    return this.get()[key];
  }

  /**
   * Validate the current configuration.
   */
  validate(): { valid: boolean; errors: string[] } {
    return ConfigValidator.validate(this.get());
  }

  /**
   * Validate the current configuration and throw if invalid.
   */
  validateOrThrow(): void {
    ConfigValidator.validateOrThrow(this.get());
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
