/**
 * ToolVisibilityMessageBuilder - Constructs lightweight tool update notifications
 *
 * Design principles:
 * - Stateless design: Does not maintain any state.
 * - Single responsibility: Only builds notification messages for tool changes.
 * - Minimal noise: No timestamps, scope IDs, or other internal details.
 * - Incremental updates: Only notifies about added/removed tools.
 */

import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import { renderTemplate } from "../../../core/utils/template-renderer/index.js";
import { TOOL_VISIBILITY_DECLARATION_TEMPLATE } from "../../../resources/predefined/prompt-templates/tool-visibility-template.js";

/**
 * Tool Visibility Message Builder
 */
export class ToolVisibilityMessageBuilder {
  constructor(private toolService: ToolRegistry) {}

  /**
   * Build visibility declaration message
   * Generates a complete tool visibility declaration for scope changes
   *
   * @param availableTools List of available tool IDs
   * @returns Declaration message content
   */
  buildVisibilityDeclarationMessage(availableTools: string[]): string {
    return this.buildUpdateNotification(
      availableTools.map(id => {
        const tool = this.toolService.getTool(id);
        return {
          id,
          description: tool?.description || "No description",
        };
      }),
      undefined,
    );
  }

  /**
   * Build metadata for visibility declaration
   *
   * @param scope Current scope
   * @param scopeId Scope ID
   * @param availableTools List of available tool IDs
   * @param changeType Type of change
   * @returns Metadata object
   */
  buildVisibilityDeclarationMetadata(
    scope: string,
    scopeId: string,
    availableTools: string[],
    changeType: string,
  ): Record<string, unknown> {
    return {
      type: "tool_visibility_declaration",
      scope,
      scopeId,
      toolIds: availableTools,
      changeType,
      timestamp: Date.now(),
    };
  }

  /**
   * Build lightweight tool update notification
   * Only includes added/removed tools without exposing internal details
   *
   * @param addedTools List of newly added tools with descriptions
   * @param removedTools List of removed tool IDs
   * @returns Notification message content
   */
  buildUpdateNotification(
    addedTools?: Array<{ id: string; description: string }>,
    removedTools?: string[],
  ): string {
    // Format added tools
    const addedSection =
      addedTools && addedTools.length > 0
        ? addedTools.map(t => `- ${t.id}: ${t.description}`).join("\n")
        : "";

    // Format removed tools
    const removedSection =
      removedTools && removedTools.length > 0 ? removedTools.map(id => `- ${id}`).join("\n") : "";

    // Render template (empty sections will be omitted)
    return renderTemplate(TOOL_VISIBILITY_DECLARATION_TEMPLATE.content, {
      addedTools: addedSection || "",
      removedTools: removedSection || "",
    });
  }

  /**
   * Build metadata for tool update event (for logging/debugging)
   * Note: This is NOT sent to LLM, only used for internal tracking
   *
   * @param scope Scope (internal use only)
   * @param scopeId Scope ID (internal use only)
   * @param addedToolIds Added tool IDs
   * @param removedToolIds Removed tool IDs
   * @returns Event metadata
   */
  buildEventMetadata(
    scope: string,
    scopeId: string,
    addedToolIds?: string[],
    removedToolIds?: string[],
  ): Record<string, unknown> {
    return {
      type: "tool_update",
      scope,
      scopeId,
      addedToolIds,
      removedToolIds,
    };
  }
}
