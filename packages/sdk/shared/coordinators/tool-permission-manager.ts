/**
 * Tool Permission Manager
 * Manages runtime tool permissions (enabled/disabled state)
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger();

/**
 * Permission change record
 */
export interface PermissionChange {
  /** Timestamp of the change */
  timestamp: number;
  /** Type of change */
  type: "enable" | "disable";
  /** Affected tool IDs */
  toolIds: string[];
  /** Optional reason for the change */
  reason?: string;
  /** Node ID that triggered the change (if applicable) */
  nodeId?: string;
}

/**
 * Tool permission state
 */
export interface ToolPermissionState {
  /** Currently enabled tools (can execute without approval) */
  enabledTools: Set<string>;

  /** Temporarily disabled tools (cannot execute) */
  disabledTools: Set<string>;

  /** History of permission changes */
  history: PermissionChange[];
}

/**
 * Tool Permission Manager
 *
 * Manages the runtime state of tool permissions.
 * Tracks which tools are currently enabled/disabled.
 *
 * Key invariants:
 * - enabledTools ∪ disabledTools = schemaTools (from AvailableTools config)
 * - Initial state: enabledTools = resolveInitialTools(config)
 * - TOOL_VISIBILITY nodes modify this state at runtime
 */
export class ToolPermissionManager {
  /** Current permission state */
  private state: ToolPermissionState;

  /** Complete set of schema tools (from AvailableTools config) */
  private readonly schemaTools: Set<string>;

  /**
   * Constructor
   * @param initialEnabledTools - Initially enabled tool IDs
   * @param allSchemaTools - Complete set of tools in the schema
   */
  constructor(initialEnabledTools: string[], allSchemaTools: string[]) {
    this.schemaTools = new Set(allSchemaTools);

    // Initialize enabled tools
    let enabledSet: Set<string>;

    // If initial list is empty, enable all schema tools
    if (initialEnabledTools.length === 0) {
      enabledSet = new Set(allSchemaTools);
    } else {
      enabledSet = new Set<string>();
      for (const toolId of initialEnabledTools) {
        if (this.schemaTools.has(toolId)) {
          enabledSet.add(toolId);
        } else {
          logger.warn(`Tool '${toolId}' in initial list is not in schema, ignoring`);
        }
      }
    }

    // Disabled tools = schema - enabled
    const disabledSet = new Set<string>();
    for (const toolId of this.schemaTools) {
      if (!enabledSet.has(toolId)) {
        disabledSet.add(toolId);
      }
    }

    this.state = {
      enabledTools: enabledSet,
      disabledTools: disabledSet,
      history: [],
    };

    logger.info("ToolPermissionManager initialized", {
      enabledCount: enabledSet.size,
      disabledCount: disabledSet.size,
      totalSchemaTools: this.schemaTools.size,
    });
  }

  /**
   * Get current permission state
   */
  getState(): ToolPermissionState {
    return { ...this.state };
  }

  /**
   * Check if a tool is currently enabled
   * @param toolId - Tool ID to check
   * @returns true if enabled, false otherwise
   */
  isEnabled(toolId: string): boolean {
    return this.state.enabledTools.has(toolId);
  }

  /**
   * Check if a tool is currently disabled
   * @param toolId - Tool ID to check
   * @returns true if disabled, false otherwise
   */
  isDisabled(toolId: string): boolean {
    return this.state.disabledTools.has(toolId);
  }

  /**
   * Get all currently enabled tools
   */
  getEnabledTools(): string[] {
    return Array.from(this.state.enabledTools);
  }

  /**
   * Get all currently disabled tools
   */
  getDisabledTools(): string[] {
    return Array.from(this.state.disabledTools);
  }

  /**
   * Get the block reason for a tool (from most recent disable action)
   * @param toolId - Tool ID
   * @returns Reason string or undefined
   */
  getBlockReason(toolId: string): string | undefined {
    // Find the most recent disable action for this tool
    const disableEntry = [...this.state.history]
      .reverse()
      .find(entry => entry.type === "disable" && entry.toolIds.includes(toolId));

    return disableEntry?.reason;
  }

  /**
   * Enable one or more tools
   * @param toolIds - Tool IDs to enable
   * @param reason - Optional reason for enabling
   * @param nodeId - Optional node ID that triggered this change
   */
  enableTools(toolIds: string[], reason?: string, nodeId?: string): void {
    const actuallyEnabled: string[] = [];

    for (const toolId of toolIds) {
      if (!this.schemaTools.has(toolId)) {
        logger.warn(`Cannot enable tool '${toolId}': not in schema`);
        continue;
      }

      if (this.state.disabledTools.has(toolId)) {
        this.state.disabledTools.delete(toolId);
        this.state.enabledTools.add(toolId);
        actuallyEnabled.push(toolId);
      }
    }

    if (actuallyEnabled.length > 0) {
      this.recordChange("enable", actuallyEnabled, reason, nodeId);
      logger.info("Tools enabled", {
        toolIds: actuallyEnabled,
        reason,
        nodeId,
      });
    }
  }

  /**
   * Disable one or more tools
   * @param toolIds - Tool IDs to disable
   * @param reason - Optional reason for disabling
   * @param nodeId - Optional node ID that triggered this change
   */
  disableTools(toolIds: string[], reason?: string, nodeId?: string): void {
    const actuallyDisabled: string[] = [];

    for (const toolId of toolIds) {
      if (!this.schemaTools.has(toolId)) {
        logger.warn(`Cannot disable tool '${toolId}': not in schema`);
        continue;
      }

      if (this.state.enabledTools.has(toolId)) {
        this.state.enabledTools.delete(toolId);
        this.state.disabledTools.add(toolId);
        actuallyDisabled.push(toolId);
      }
    }

    if (actuallyDisabled.length > 0) {
      this.recordChange("disable", actuallyDisabled, reason, nodeId);
      logger.info("Tools disabled", {
        toolIds: actuallyDisabled,
        reason,
        nodeId,
      });
    }
  }

  /**
   * Record a permission change in history
   */
  private recordChange(
    type: "enable" | "disable",
    toolIds: string[],
    reason?: string,
    nodeId?: string,
  ): void {
    this.state.history.push({
      timestamp: Date.now(),
      type,
      toolIds,
      reason,
      nodeId,
    });
  }

  /**
   * Serialize state for checkpointing
   */
  serialize(): unknown {
    return {
      enabledTools: Array.from(this.state.enabledTools),
      disabledTools: Array.from(this.state.disabledTools),
      history: this.state.history.slice(-100), // Keep last 100 entries
    };
  }

  /**
   * Deserialize state from checkpoint
   * @param data - Serialized state
   */
  static deserialize(data: unknown, allSchemaTools: string[]): ToolPermissionManager {
    if (!data || typeof data !== "object") {
      throw new Error("Invalid permission state data");
    }

    const serialized = data as {
      enabledTools?: string[];
      disabledTools?: string[];
      history?: PermissionChange[];
    };

    const enabledTools = serialized.enabledTools || [];
    const manager = new ToolPermissionManager(enabledTools, allSchemaTools);

    // Restore history if available
    if (serialized.history) {
      manager.state.history = serialized.history;
    }

    return manager;
  }
}
