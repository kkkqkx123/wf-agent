import type { PromptTemplate } from "@wf-agent/types";

export const TOOL_VISIBILITY_DECLARATION_TEMPLATE: PromptTemplate = {
  id: "tool-visibility-declaration",
  name: "Tool Visibility Declaration",
  description: "Template for declaring updated available tools",
  category: "tools",
  content: `Available tools updated:
{{addedTools}}
{{removedTools}}`,
};

export interface ToolUpdateNotification {
  added?: Array<{ id: string; description: string }>;
  removed?: string[];
}
