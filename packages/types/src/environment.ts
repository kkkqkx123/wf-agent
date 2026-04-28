/**
 * Definition of environmental information types
 *
 * Define type interfaces related to the runtime environment
 */

import type { Metadata } from "./common.js";

/**
 * Workspace Information
 */
export interface WorkspaceInfo {
  /**
   * Name of workspace
   */
  name: string;

  /**
   * Workspace path (absolute path)
   */
  path: string;

  /**
   * Workspace URI (optional)
   */
  uri?: string;

  /**
   * Is it a currently active workspace
   */
  isActive?: boolean;

  /**
   * Metadata (optional)
   */
  metadata?: Metadata;
}

/**
 * Environment Information Configuration
 */
export interface EnvironmentInfo {
  /**
   * Operating System Information
   */
  os: string;

  /**
   * Operating System Architecture
   */
  arch?: string;

  /**
   * Operating system version
   */
  osVersion?: string;

  /**
   * time zones
   */
  timezone: string;

  /**
   * user language
   */
  userLanguage: string;

  /**
   * User Area Settings
   */
  locale?: string;

  /**
   * Workspace paths (multiple workspaces supported)
   */
  workspaces: WorkspaceInfo[];

  /**
   * current timestamp
   */
  timestamp?: number;

  /**
   * Node.js version
   */
  nodeVersion?: string;

  /**
   * Additional environmental information
   */
  metadata?: Metadata;
}

/**
 * Environment Information Configuration Items
 */
export interface EnvironmentConfig {
  /**
   * Whether to include operating system details
   */
  includeOSDetails?: boolean;

  /**
   * Whether to include workspace information
   */
  includeWorkspaces?: boolean;

  /**
   * Whether to include the current time
   */
  includeTimestamp?: boolean;

  /**
   * Whether or not to include a Node.js version
   */
  includeNodeVersion?: boolean;

  /**
   * Customized workspace filters
   */
  workspaceFilter?: (workspace: WorkspaceInfo) => boolean;
}
