/**
 * Rejection Message Builder
 * Builds customized rejection messages for blocked tools
 */

/**
 * Tool rejection message configuration
 */
export interface ToolRejectionConfig {
  /**
   * Global default rejection template
   * Used when no tool-specific config exists
   *
   * Variables:
   * - {{toolId}}: The blocked tool ID
   * - {{reason}}: Optional blocking reason
   */
  globalDefaultTemplate?: string;

  /**
   * Per-tool rejection templates
   * Key: tool ID
   * Value: rejection message template
   */
  toolSpecificTemplates?: Record<string, string>;

  /**
   * Whether to inject hint in next user message
   * @default true
   */
  injectUserMessageHint?: boolean;

  /**
   * User message hint template
   * Variables:
   * - {{enabledTools}}: Comma-separated list of newly enabled tools
   * - {{disabledTools}}: Comma-separated list of newly disabled tools
   */
  userMessageHintTemplate?: string;
}

/**
 * Default configuration
 */
export const DEFAULT_REJECTION_CONFIG: ToolRejectionConfig = {
  globalDefaultTemplate: "Tool '{{toolId}}' is currently unavailable. {{reason}}",

  injectUserMessageHint: true,

  userMessageHintTemplate:
    "[Note: {{disabledTools}} are now disabled. {{enabledTools}} are now available.]",
};

/**
 * Rejection Message Builder
 *
 * Builds customized rejection messages for blocked tools.
 * Supports global defaults and per-tool custom templates.
 */
export class RejectionMessageBuilder {
  private config: ToolRejectionConfig;

  constructor(config?: ToolRejectionConfig) {
    this.config = config || {};
  }

  /**
   * Build rejection message for a blocked tool
   * @param toolId - The blocked tool ID
   * @param reason - Optional blocking reason
   * @returns Formatted rejection message
   */
  buildRejectionMessage(toolId: string, reason?: string): string {
    // Try tool-specific template first
    const toolTemplate = this.config.toolSpecificTemplates?.[toolId];

    if (toolTemplate) {
      return this.renderTemplate(toolTemplate, { toolId, reason: reason || "" });
    }

    // Fall back to global default
    const globalTemplate =
      this.config.globalDefaultTemplate || DEFAULT_REJECTION_CONFIG.globalDefaultTemplate!;

    return this.renderTemplate(globalTemplate, { toolId, reason: reason || "" });
  }

  /**
   * Build user message hint
   * @param enabledTools - List of newly enabled tools
   * @param disabledTools - List of newly disabled tools
   * @returns Formatted hint or null if hints are disabled
   */
  buildUserMessageHint(enabledTools: string[], disabledTools: string[]): string | null {
    if (this.config.injectUserMessageHint === false) {
      return null;
    }

    if (enabledTools.length === 0 && disabledTools.length === 0) {
      return null;
    }

    const template =
      this.config.userMessageHintTemplate || DEFAULT_REJECTION_CONFIG.userMessageHintTemplate!;

    return this.renderTemplate(template, {
      enabledTools: enabledTools.join(", "),
      disabledTools: disabledTools.join(", "),
    });
  }

  /**
   * Render a template with variables
   * @param template - Template string with {{variable}} placeholders
   * @param variables - Variable values
   * @returns Rendered string
   */
  private renderTemplate(template: string, variables: Record<string, string>): string {
    return template.replace(/\{\{(\w+)\}\}/g, (_, key) => {
      const value = variables[key];
      return value !== undefined ? value : "";
    });
  }

  /**
   * Update configuration
   * @param config - New or partial configuration
   */
  updateConfig(config: Partial<ToolRejectionConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /**
   * Get current configuration
   */
  getConfig(): ToolRejectionConfig {
    return { ...this.config };
  }
}
