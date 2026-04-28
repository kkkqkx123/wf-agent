# Tool Call Format Integration Analysis

## Current State Analysis

### 1. Multi-Format Support in `tool-call-parser.ts`

The `ToolCallParser` class supports 4 parsing formats:

| Format | Detection | Parsing Method | Use Case |
|--------|-----------|----------------|----------|
| **XML** | `<tool_use>` | `parseXMLToolCalls()` | Small local LLMs without function calling |
| **Wrapped JSON** | `<<<TOOL_CALL>>>` | `parseJSONToolCalls()` | Web-based LLM debugging |
| **Raw JSON** | `{...}` or `[...]` | `parseRawJsonToolCalls()` | Native API compatibility |
| **Native** | OpenAI format | `convertToStandardToolCall()` | Standard function calling |

### 2. LLMProfile.toolMode Definition

```typescript
// packages/types/src/llm/profile.ts
toolMode?: "function_call" | "xml" | "json" | "raw";
```

**Issues Identified:**
1. **Inconsistent naming**: `json` vs `raw` - unclear distinction
2. **Missing format specification**: No clear mapping between `toolMode` and parser formats
3. **No format metadata**: Missing information about tool description format vs tool call format

### 3. FormatterConfig.toolMode Definition

```typescript
// sdk/core/llm/formatters/types.ts
toolMode?: "function_call" | "xml" | "json";
```

**Issues Identified:**
1. **Missing `raw` option**: Inconsistent with LLMProfile
2. **No format configuration**: Cannot customize markers (e.g., `<<<TOOL_CALL>>>`)

### 4. Prompt Templates for Tool Formats

Existing templates in `packages/prompt-templates/src/templates/tools/formatters/`:

- `xml-format.ts`: XML tool description + XML call format
- `json-format.ts`: JSON tool description + Wrapped JSON call format

**Issues Identified:**
1. **Hardcoded formats**: Cannot customize call format markers
2. **No raw format template**: Missing for `raw` toolMode
3. **No validation**: No type checking between toolMode and prompt format

## Integration Gap Analysis

### Critical Gap: ToolMode vs Prompt Format Mismatch

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Current Architecture                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  LLMProfile.toolMode          FormatterConfig.toolMode      Prompt Template  │
│  ─────────────────            ────────────────────────      ───────────────  │
│  "function_call"              "function_call"                Native format   │
│  "xml"                        "xml"                          XML format      │
│  "json"                       "json"                         JSON format     │
│  "raw"                        ❌ MISSING                     ❌ MISSING      │
│                                                                              │
│  Problems:                                                                   │
│  1. FormatterConfig missing "raw" option                                     │
│  2. No prompt template for "raw" format                                      │
│  3. No type safety between profile, config, and templates                    │
│  4. No validation that toolMode matches the prompt format                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Tool Call Parser vs Tool Description Format

| Profile ToolMode | Tool Description Format | Tool Call Format (Expected) | Parser Support |
|------------------|------------------------|----------------------------|----------------|
| `function_call` | Native/OpenAI schema | Native API format | ✅ Yes |
| `xml` | XML description | `<tool_use>...</tool_use>` | ✅ Yes |
| `json` | JSON description | `<<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>` | ✅ Yes |
| `raw` | JSON description | Raw JSON `{...}` | ✅ Yes |

**Note**: Parser supports all formats, but the integration between profile configuration and prompt generation is incomplete.

## Required Improvements

### 1. Type Definition Consolidation

Create a unified `ToolCallFormat` type in `packages/types`:

```typescript
// packages/types/src/llm/tool-call-format.ts

/**
 * Tool call format types
 * Defines how tools are described in prompts and how LLM should format tool calls
 */
export type ToolCallFormat = 
  | "function_call"  // Native API function calling (OpenAI, Anthropic, etc.)
  | "xml"            // XML format for non-function-calling models
  | "json_wrapped"   // JSON wrapped with custom markers
  | "json_raw";      // Raw JSON without markers

/**
 * Tool call format configuration
 */
export interface ToolCallFormatConfig {
  /** Format type */
  format: ToolCallFormat;
  
  /** Custom markers for wrapped JSON format */
  markers?: {
    start: string;
    end: string;
  };
  
  /** XML tag names for XML format */
  xmlTags?: {
    toolUse: string;
    toolName: string;
    parameters: string;
  };
  
  /** Whether to include tool description in prompt */
  includeDescription: boolean;
  
  /** Tool description format style */
  descriptionStyle: "detailed" | "compact" | "minimal";
}
```

### 2. Update LLMProfile

```typescript
// packages/types/src/llm/profile.ts

import type { ToolCallFormat, ToolCallFormatConfig } from "./tool-call-format.js";

export interface LLMProfile {
  // ... existing fields ...
  
  /**
   * Tool call format mode
   * @deprecated Use `toolCallFormat` instead for more control
   */
  toolMode?: ToolCallFormat;
  
  /**
   * Tool call format configuration
   * Provides detailed control over tool calling behavior
   */
  toolCallFormat?: ToolCallFormatConfig;
}
```

### 3. Update FormatterConfig

```typescript
// sdk/core/llm/formatters/types.ts

import type { ToolCallFormat, ToolCallFormatConfig } from "@wf-agent/types";

export interface FormatterConfig {
  // ... existing fields ...
  
  /**
   * Tool call format mode (legacy)
   * @deprecated Use `toolCallFormat` instead
   */
  toolMode?: ToolCallFormat;
  
  /**
   * Tool call format configuration
   */
  toolCallFormat?: ToolCallFormatConfig;
}
```

### 4. Create Format-Aware Prompt Templates

```typescript
// packages/prompt-templates/src/templates/tools/formatters/raw-format.ts

/**
 * Raw JSON format template
 * For models that output raw JSON without markers
 */
export const TOOL_RAW_FORMAT_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw",
  name: "Tool Raw JSON Format",
  description: "Raw JSON format for tool definitions",
  category: "tools",
  content: `### {{toolName}}

{{toolDescription}}

Parameters:
{{parametersDescription}}`,
  variables: [
    { name: "toolName", type: "string", required: true },
    { name: "toolDescription", type: "string", required: true },
    { name: "parametersDescription", type: "string", required: false },
  ],
};

export const TOOLS_RAW_LIST_TEMPLATE: PromptTemplate = {
  id: "tools.formatters.raw_list",
  name: "Tools Raw JSON List",
  description: "Raw JSON tool list with call format instructions",
  category: "tools",
  content: `## Available Tools

{{toolsRaw}}

### Tool Call Format

Use the following format to call tools:

\`\`\`json
{"tool": "tool_name", "parameters": {"parameter_name": "parameter_value"}}
\`\`\`

Or for multiple tools:

\`\`\`json
[
  {"tool": "tool1", "parameters": {...}},
  {"tool": "tool2", "parameters": {...}}
]
\`\`\``,`,
  variables: [
    { name: "toolsRaw", type: "string", required: true },
  ],
};
```

### 5. Create Format Selection Utility

```typescript
// sdk/core/llm/formatters/tool-format-selector.ts

import type { ToolCallFormat, ToolCallFormatConfig } from "@wf-agent/types";
import type { PromptTemplate } from "@wf-agent/prompt-templates";
import {
  TOOLS_XML_LIST_TEMPLATE,
  TOOLS_JSON_LIST_TEMPLATE,
  TOOL_RAW_FORMAT_TEMPLATE,
} from "@wf-agent/prompt-templates";

/**
 * Get the appropriate prompt template for tool format
 */
export function getToolFormatTemplate(
  format: ToolCallFormat
): PromptTemplate {
  switch (format) {
    case "function_call":
      // Native format doesn't need special template
      return null;
    case "xml":
      return TOOLS_XML_LIST_TEMPLATE;
    case "json_wrapped":
      return TOOLS_JSON_LIST_TEMPLATE;
    case "json_raw":
      return TOOLS_RAW_LIST_TEMPLATE;
    default:
      throw new Error(`Unknown tool call format: ${format}`);
  }
}

/**
 * Get tool call parser options for format
 */
export function getToolCallParserOptions(format: ToolCallFormat) {
  switch (format) {
    case "xml":
      return { preferredFormats: ["xml"] };
    case "json_wrapped":
      return { preferredFormats: ["json"] };
    case "json_raw":
      return { preferredFormats: ["raw"] };
    case "function_call":
      return null; // Native parsing
    default:
      return { preferredFormats: ["xml", "json", "raw"] };
  }
}

/**
 * Validate that profile format matches prompt format
 */
export function validateToolFormatCompatibility(
  profileFormat: ToolCallFormat,
  promptFormat: ToolCallFormat
): boolean {
  return profileFormat === promptFormat;
}
```

## Implementation Plan

### Phase 1: Type Definitions
1. Create `packages/types/src/llm/tool-call-format.ts`
2. Update `packages/types/src/llm/profile.ts`
3. Update `packages/types/src/index.ts` exports

### Phase 2: Formatter Updates
1. Update `sdk/core/llm/formatters/types.ts`
2. Create `sdk/core/llm/formatters/tool-format-selector.ts`
3. Update formatters to use `toolCallFormat` instead of `toolMode`

### Phase 3: Prompt Template Updates
1. Create `packages/prompt-templates/src/templates/tools/formatters/raw-format.ts`
2. Update `packages/prompt-templates/src/templates/tools/formatters/index.ts`
3. Create format-aware template selection utilities

### Phase 4: Integration & Validation
1. Update system prompt builder to respect `toolCallFormat`
2. Add validation that profile format matches prompt format
3. Add tests for format compatibility

## Backward Compatibility

- `toolMode` remains as deprecated field for backward compatibility
- Default behavior: If `toolCallFormat` is not specified, derive from `toolMode`
- Migration path: `toolMode` → `toolCallFormat.format`

```typescript
// Migration helper
function migrateToolMode(toolMode?: string): ToolCallFormatConfig | undefined {
  if (!toolMode) return undefined;
  
  const formatMap: Record<string, ToolCallFormat> = {
    "function_call": "function_call",
    "xml": "xml",
    "json": "json_wrapped",
    "raw": "json_raw",
  };
  
  return {
    format: formatMap[toolMode] || "function_call",
    includeDescription: true,
    descriptionStyle: "detailed",
  };
}
```
