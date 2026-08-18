# Shared Tool System

## 1. Overview

The shared tool system provides tool registration, schema formatting, description generation, and execution support used by both workflow and agent modules.

## 2. ToolRegistry

Manages tool registration and lookup:

```
ToolRegistry
├── Registration
│   ├── registerTool(tool) → void
│   │   ├── Validate tool definition
│   │   ├── Check for duplicate registration
│   │   └── Store in registry
│   │
│   ├── registerTools(tools) → void
│   │   └── Batch register multiple tools
│   │
│   ├── unregisterTool(name) → boolean
│   │   └── Remove tool from registry
│   │
│   └── clear() → void
│       └── Clear all tools
│
├── Query
│   ├── getTool(name) → Tool | undefined
│   │   └── Lookup by name
│   │
│   ├── getAllTools() → Tool[]
│   │   └── Return all registered tools
│   │
│   ├── getAvailableTools(filter?) → Tool[]
│   │   ├── Apply include/exclude filter
│   │   └── Return filtered tools
│   │
│   ├── hasTool(name) → boolean
│   │   └── Check tool existence
│   │
│   └── getToolCount() → number
│       └── Return total tool count
│
└── Tool Categories
    ├── getToolsByCategory(category) → Tool[]
    └── getCategories() → string[]
```

## 3. Tool Schema Helpers

### prepareToolSchemas

Prepares tool schemas for LLM consumption:

```
prepareToolSchemas(tools, formatConfig?):
  1. Convert tool definitions to LLM-compatible schemas
  2. Apply tool format configuration:
     - OpenAI format (functions)
     - Anthropic format (tools)
     - Gemini format (function_declarations)
  3. Filter tools based on availability config
  4. Return ToolSchema[]
```

### ToolSchema

```typescript
interface ToolSchema {
  name: string;
  description: string;
  input_schema: {
    type: "object";
    properties: Record<string, SchemaProperty>;
    required?: string[];
  };
}
```

## 4. Tool Declaration Formatter

Formats tool declarations for different LLM providers:

```
ToolDeclarationFormatter
├── formatForOpenAI(tool) → OpenAITool
├── formatForAnthropic(tool) → AnthropicTool
├── formatForGemini(tool) → GeminiTool
└── formatForProvider(tool, provider) → ProviderTool
```

## 5. Tool Description Generator

Generates human-readable tool descriptions:

```
ToolDescriptionGenerator
├── generateDescription(tool) → string
│   ├── Generate from tool name, description, parameters
│   └── Return formatted description string
│
├── generateParameterDescriptions(tool) → ParameterDescription[]
│   └── Generate parameter descriptions
│
└── generateUsageGuide(tool) → string
    └── Generate usage guide with examples
```

## 6. Tool Description Registry

Registry for tool descriptions:

```
ToolDescriptionRegistry
├── registerDescription(toolName, description) → void
├── getDescription(toolName) → string | undefined
├── hasDescription(toolName) → boolean
└── generateMissingDescriptions() → void
```

## 7. Tool Execution Signal

Handles tool execution signals (approval, abort):

```
ToolExecutionSignal
├── APPROVAL_REQUIRED
├── APPROVED
├── DENIED
├── TIMEOUT
└── ABORTED
```

## 8. Tool Types

```typescript
interface Tool {
  name: string;
  description: string;
  category?: ToolCategory;
  parameters: ToolParameter[];
  execute: (params: Record<string, unknown>, context: ToolContext) => Promise<ToolResult>;
  metadata?: Record<string, unknown>;
}

interface ToolParameter {
  name: string;
  type: "string" | "number" | "boolean" | "object" | "array";
  description: string;
  required?: boolean;
  enum?: string[];
  default?: unknown;
}

interface ToolResult {
  success: boolean;
  data?: unknown;
  error?: string;
  metadata?: Record<string, unknown>;
}

interface ToolContext {
  executionId: string;
  signal?: AbortSignal;
  metadata?: Record<string, unknown>;
}
```

## 9. Tool Categories

| Category | Description |
|----------|-------------|
| `builtin` | Built-in tools (file operations, calculations, etc.) |
| `native` | Custom tools registered via plugin/tool system |
| `rest` | External API tools |
| `mcp` | Model Context Protocol tools |
| `system` | System-level tools |

## 10. Tool Format Compatibility

The `validateToolFormatCompatibility()` function checks if the tool format is compatible with the selected LLM profile:

```
validateToolFormatCompatibility(toolSchemas, llmProfile):
  1. Check LLM provider supports the tool format
  2. Check tool schema constraints (max parameters, nesting depth)
  3. Return compatibility result with warnings
```