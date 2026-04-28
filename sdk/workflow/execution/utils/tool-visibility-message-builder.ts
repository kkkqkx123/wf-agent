/**
 * ToolVisibilityMessageBuilder - A tool for constructing messages related to tool visibility, specifically designed to generate messages regarding tool visibility declarations.
 *
 * Design principles:
 * - Stateless design: Does not maintain any state.
 * - Single responsibility: Is responsible only for message construction.
 * - Testability: Acts as a pure function, making it easy to perform unit testing.
 * - Use of a template renderer: Utilizes templates from @wf-agent/prompt-templates.
 *
 */

import type { ToolScope } from "../../stores/tool-context-store.js";
import type { VisibilityChangeType } from "../types/tool-visibility.types.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import { now, renderTemplate } from "@wf-agent/common-utils";
import { generateToolTable } from "../../../core/utils/tools/tool-description-generator.js";
import {
  TOOL_VISIBILITY_DECLARATION_TEMPLATE,
  VISIBILITY_CHANGE_TYPE_TEXTS,
} from "@wf-agent/prompt-templates";

/**
 * Tool Visibility Message Builder
 */
export class ToolVisibilityMessageBuilder {
  constructor(private toolService: ToolRegistry) {}

  /**
   * Construct the visibility assertion message content
   * @param scope  Scope
   * @param scopeId  Scope ID
   * @param toolIds  List of tool IDs
   * @param changeType  Change type
   * @returns  Assertion message content
   */
  buildVisibilityDeclarationMessage(
    scope: ToolScope,
    scopeId: string,
    toolIds: string[],
    changeType: VisibilityChangeType,
  ): string {
    const timestamp = new Date().toISOString();
    const changeTypeText = VISIBILITY_CHANGE_TYPE_TEXTS[changeType] || changeType;

    // Get the list of tool objects.
    const tools = toolIds.map(id => this.toolService.getTool(id)).filter(Boolean);

    // Use the tool description generator to create a tool table.
    const toolDescriptions = tools.length > 0 ? generateToolTable(tools) : "No tools available";

    // Use a template renderer to generate messages.
    const variables = {
      timestamp,
      scope,
      scopeId,
      changeTypeText,
      toolDescriptions,
    };

    return renderTemplate(TOOL_VISIBILITY_DECLARATION_TEMPLATE.content, variables);
  }

  /**
   * Construct visibility declaration message metadata
   * @param scope  Scope
   * @param scopeId  Scope ID
   * @param toolIds  List of tool IDs
   * @param changeType  Change type
   * @returns  Message metadata
   */
  buildVisibilityDeclarationMetadata(
    scope: ToolScope,
    scopeId: string,
    toolIds: string[],
    changeType: VisibilityChangeType,
  ): Record<string, unknown> {
    return {
      type: "tool_visibility_declaration",
      timestamp: now(),
      scope,
      scopeId,
      toolIds,
      changeType,
    };
  }
}
