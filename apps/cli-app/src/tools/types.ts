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
  /** Database file path for session notes storage (relative to workspaceDir or absolute) */
  dbPath?: string;
}
