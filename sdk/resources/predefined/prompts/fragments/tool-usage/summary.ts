/**
 * Summary of Tool Usage Guidelines
 */

import type { SystemPromptFragment } from "@wf-agent/prompt-templates";

/**
 * XML Format Tool Call Summary Fragment
 */
export const TOOL_USAGE_XML_SUMMARY_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.tool-usage.xml-summary",
  category: "tool-usage",
  description: "Summary of the XML Format Tool Invocation Specification",
  content: `## Tool Usage

### Tool Call Format
When you need to use a tool, you must format your response as follows:

<tool_call>
<tool_name>tool_name</tool_name>
<parameters>
<param1>value1</param1>
<param2>value2</param2>
</parameters>
</tool_call>

### Important Notes
- Only use the tools explicitly provided to you
- Tool parameters must conform to the schema definition
- Wait for tool execution results before making the next call
- If no tool is needed, respond directly without tool_call tags`,
};

/**
 * JSON Format Tool Call Summary Fragment
 */
export const TOOL_USAGE_JSON_SUMMARY_FRAGMENT: SystemPromptFragment = {
  id: "sdk.fragments.tool-usage.json-summary",
  category: "tool-usage",
  description: "Summary of JSON Format Tool Invocation Specification",
  content: `## Tool Usage

### Tool Call Format
When you need to use a tool, you must format your response as follows:

<tool_call>
{
  "tool_name": "tool_name",
  "parameters": {
    "param1": "value1",
    "param2": "value2"
  }
}
</tool_call>

### Important Notes
- Only use the tools explicitly provided to you
- Tool parameters must conform to the schema definition
- Wait for tool execution results before making the next call
- If no tool is needed, respond directly without tool_call tags`,
};
