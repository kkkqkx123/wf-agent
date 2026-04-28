/**
 * Tool Usage Specification Segment Template
 *
 * Structured templates that provide instructions for using the tool
 */

import type { PromptTemplate } from "../../../types/template.js";

/**
 * Template for tool usage summary fragment structure
 * Generic specification for describing tool use (without specific tool descriptions)
 */
export const TOOL_USAGE_SUMMARY_STRUCTURE: PromptTemplate = {
  id: "fragments.tool-usage.summary.structure",
  name: "Tool Usage Summary Structure",
  description: "Tool uses a structural template for summary fragments (general specification)",
  category: "fragments",
  content: `## {{sectionTitle}}

### Tool Call Format
When you need to use a tool, you must format your response as follows:

{{callFormat}}

### Important Notes
{{notes}}`,
  variables: [
    {
      name: "sectionTitle",
      type: "string",
      required: true,
      description: 'Chapter titles, such as "Tool Usage" or "Tool Usage"',
    },
    {
      name: "callFormat",
      type: "string",
      required: true,
      description: "Tool call format description",
    },
    { name: "notes", type: "string", required: true, description: "important note" },
  ],
};

/**
 * Tools use summary fragment constants
 */
export const TOOL_USAGE_SUMMARY_TEMPLATE = `## {{sectionTitle}}

### Tool Call Format
When you need to use a tool, you must format your response as follows:

{{callFormat}}

### Important Notes
{{notes}}`;

/**
 * XML formatting tool call example template
 */
export const TOOL_XML_CALL_FORMAT_TEMPLATE = `<tool_call>
<tool_name>{{exampleToolName}}</tool_name>
<parameters>
{{exampleParameters}}
</parameters>
</tool_call>`;

/**
 * JSON format tool call example template
 */
export const TOOL_JSON_CALL_FORMAT_TEMPLATE = `<tool_call>
{
  "tool_name": "{{exampleToolName}}",
  "parameters": {
{{exampleParameters}}
  }
}
</tool_call>`;

/**
 * Tool List Description Template
 * For dynamic injection of specific tool descriptions
 */
export const TOOL_LIST_DESCRIPTION_TEMPLATE: PromptTemplate = {
  id: "fragments.tool-usage.list.description",
  name: "Tool List Description",
  description: "Dynamic tool list description template",
  category: "fragments",
  content: `### Available Tools
The following tools are available for you to use:

{{toolDescriptions}}

### Tool Usage Rules
1. Only use the tools listed above
2. Follow the exact parameter schema for each tool
3. Wait for tool execution results before making the next call`,
  variables: [
    {
      name: "toolDescriptions",
      type: "string",
      required: true,
      description: "Tool description list (dynamically generated)",
    },
  ],
};

/**
 * Tool Description Item Template
 */
export const TOOL_DESCRIPTION_ITEM_TEMPLATE = `#### {{toolName}}
- **ID**: {{toolId}}
- **Description**: {{toolDescription}}
- **Parameters**: {{parameterSummary}}`;
