/**
 * Tool Registry Configuration
 * Defines tool registration configuration specific to the app layer
 */

/**
 * Tool registry configuration
 */
export interface ToolRegistryConfig {
  /** Working directory */
  workspaceDir?: string;
  /** Memory file path */
  memoryFile?: string;
}
