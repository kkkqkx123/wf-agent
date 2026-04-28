/**
 * Tool visibility declaration template
 * Used to generate notification messages for changes to tool visibility
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Main template for tool visibility declarations
 */
export const TOOL_VISIBILITY_DECLARATION_TEMPLATE: PromptTemplate = {
  id: "tools.visibility.declaration",
  name: "Tool Visibility Declaration",
  description: "Main template for tool visibility declarations",
  category: "tools",
  content: `## Tool Visibility Declaration

**Effective Time**: {{timestamp}}
**Current Scope**: {{scope}}({{scopeId}})
**Change Type**: {{changeTypeText}}

### List of Currently Available Tools

| Tool Name | Tool ID | Description |
|----------|--------|------|
{{toolDescriptions}}

### Important Notes

1. **Only tools listed above may be used**; calls to other tools will be rejected
2. Tool parameters must conform to the schema definition
3. If you require additional tools, please complete the current task and exit the current scope`,
  variables: [
    { name: "timestamp", type: "string", required: true, description: "Effective time" },
    { name: "scope", type: "string", required: true, description: "Scope type" },
    { name: "scopeId", type: "string", required: true, description: "Scope ID" },
    { name: "changeTypeText", type: "string", required: true, description: "Change type text" },
    {
      name: "toolDescriptions",
      type: "string",
      required: true,
      description: "Tool description table row",
    },
  ],
};

/**
 * Tool table row template (string constant)
 */
export const TOOL_TABLE_ROW_TEMPLATE = "| {{toolName}} | {{toolId}} | {{toolDescription}} |";

/**
 * Visibility change type text mapping
 */
export const VISIBILITY_CHANGE_TYPE_TEXTS = {
  init: "Initialisation",
  enter_scope: "Enter scope",
  add_tools: "Add tools",
  exit_scope: "Exit scope",
  refresh: "Refresh declaration",
} as const;

/**
 * Visibility change type
 */
export type VisibilityChangeType = keyof typeof VISIBILITY_CHANGE_TYPE_TEXTS;
