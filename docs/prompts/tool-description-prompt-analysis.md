# Tool Description Prompt Building Analysis

This document analyzes the tool description prompt building functionality.

## Status: ✅ FULLY IMPLEMENTED

All previously identified gaps have been resolved. The tool description prompt building system is now fully functional.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Tool Description System                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────┐    ┌─────────────────────────────────────────┐ │
│  │  Predefined Tools       │    │  ToolDescriptionRegistry                │ │
│  │  (sdk/resources/        │───▶│  (Singleton)                            │ │
│  │   predefined/tools/)    │    │  - registerAllPredefinedToolDescriptions│ │
│  │                         │    │  - get(toolId)                          │ │
│  │  Each tool exports:     │    │  - has(toolId)                          │ │
│  │  - *_TOOL_DESCRIPTION   │    │                                          │ │
│  │  - schema               │    └─────────────────────────────────────────┘ │
│  │  - handler              │                      │                         │
│  └─────────────────────────┘                      │                         │
│                                                   ▼                         │
│  ┌─────────────────────────┐    ┌─────────────────────────────────────────┐ │
│  │  Runtime Tool Objects   │    │  Tool Description Generator             │ │
│  │  (Tool from @wf-agent│───▶│  (sdk/core/utils/tools/)                │ │
│  │  /types)                │    │                                          │ │
│  │                         │    │  getToolDescriptionData(tool)            │ │
│  │  - id, name, type       │    │  ├── Check registry first                │ │
│  │  - description          │    │  └── Fallback: convert from Tool         │ │
│  │  - parameters (JSON     │    │                                          │ │
│  │    Schema)              │    │  generateToolDescription()               │ │
│  └─────────────────────────┘    │  generateToolListDescription()           │ │
│                                 │  generateToolAvailabilitySection()       │ │
│                                 └─────────────────────────────────────────┘ │
│                                                   │                         │
│                                                   ▼                         │
│  ┌─────────────────────────┐    ┌─────────────────────────────────────────┐ │
│  │  System Prompt Builder  │◀───│  Available Tools Fragment               │ │
│  │  (system-prompt-builder │    │  (sdk/resources/dynamic/                │ │
│  │  .ts)                   │    │  prompts/fragments/)                    │ │
│  │                         │    │                                          │ │
│  │  buildSystemPrompt()    │    │  generateToolDocumentation()             │ │
│  │  buildCoderSystemPrompt │    │  generateCompactToolsContent()           │ │
│  │  WithTools()            │    │  generateToolDescriptionMessage()        │ │
│  └─────────────────────────┘    └─────────────────────────────────────────┘ │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Tool Description Registry

**File**: `sdk/core/utils/tools/tool-description-registry.ts`

```typescript
export class ToolDescriptionRegistry {
  private descriptions = new Map<string, ToolDescriptionData>();
  
  register(description: ToolDescriptionData): void;
  get(id: string): ToolDescriptionData | undefined;
  has(id: string): boolean;
  getAll(): ToolDescriptionData[];
}

export const toolDescriptionRegistry = ToolDescriptionRegistry.getInstance();
```

### 2. Tool Parameter Converter

**File**: `sdk/core/utils/tools/tool-parameter-converter.ts`

Converts JSON Schema format to human-readable `ToolParameterDescription[]`:

```typescript
export function convertToolParameters(
  parameters: ToolParameterSchema
): ToolParameterDescription[];

export function convertToolParametersToString(
  parameters: ToolParameterSchema
): string;
```

### 3. Tool Description Generator

**File**: `sdk/core/utils/tools/tool-description-generator.ts`

```typescript
export type ToolDescriptionFormat =
  | "default"
  | "single-line"
  | "list"
  | "table"
  | "detailed"
  | "compact";

export function getToolDescriptionData(tool: Tool): ToolDescriptionData;

export function generateToolDescription(
  tool: Tool,
  format: ToolDescriptionFormat
): string;

export function generateToolListDescription(
  tools: Tool[],
  format: ToolDescriptionFormat,
  options?: { includeHeader?: boolean; separator?: string; groupByCategory?: boolean }
): string;

export function generateToolAvailabilitySection(
  tools: Tool[],
  format: ToolDescriptionFormat
): string;

export function generateToolTableRow(tool: Tool): string;
export function generateToolTable(tools: Tool[]): string;
```

### 4. Predefined Tool Descriptions Registration

**File**: `sdk/resources/predefined/tools/tool-descriptions.ts`

```typescript
export const ALL_PREDEFINED_TOOL_DESCRIPTIONS = [
  READ_FILE_TOOL_DESCRIPTION,
  WRITE_FILE_TOOL_DESCRIPTION,
  // ... all 21 tool descriptions
];

export function registerAllPredefinedToolDescriptions(): void;
export function initializeToolDescriptions(): void;
```

### 5. System Prompt Builder

**File**: `sdk/resources/predefined/prompts/system/system-prompt-builder.ts`

```typescript
export function buildSystemPrompt(options: SystemPromptBuildOptions): string;

export function buildCoderSystemPromptWithTools(
  tools?: Tool[],
  format?: ToolDescriptionFormat
): string;

export function buildAssistantSystemPromptWithTools(
  tools?: Tool[],
  format?: ToolDescriptionFormat
): string;
```

## Usage Example

```typescript
import { 
  buildCoderSystemPromptWithTools,
  initializeSystemPromptRegistries 
} from "@wf-agent/sdk/resources/predefined/prompts/system";
import { 
  initializeToolDescriptions 
} from "@wf-agent/sdk/resources/predefined/tools";

// Initialize (call once at application startup)
initializeToolDescriptions();
initializeSystemPromptRegistries();

// Get available tools
const tools = await toolService.getAvailableTools();

// Build system prompt with complete tool descriptions
const systemPrompt = buildCoderSystemPromptWithTools(tools, "detailed");
```

## Output Format Examples

### Detailed Format

```
read_file: Read file contents from the filesystem. Output always includes line numbers...

Parameters:
  - path (string, required): Absolute or relative path to the file
  - offset (integer, optional, default: 1): Starting line number (1-indexed)...
  - limit (integer, optional): Number of lines to read...

Tips:
  - Call this tool multiple times in parallel to read different files simultaneously
  - Use offset and limit for large files to read in chunks
```

### Compact Format

```
read_file (3 params): Read file contents from the filesystem...
bash (2 params): Execute shell commands in foreground or background...
```

### Grouped by Category

```
## Filesystem Tools

read_file: Read file contents...
write_file: Write file contents...

## Shell Tools

bash: Execute shell commands...

## Interaction Tools

ask_followup_question: Ask the user a question...
```

## Files Changed

| File | Change |
|------|--------|
| `sdk/core/utils/tools/tool-description-generator.ts` | 📝 Updated: Enhanced implementation with ToolDescriptionData support |
| `sdk/core/utils/tools/tool-description-registry.ts` | ➕ New: Tool description registry |
| `sdk/core/utils/tools/tool-parameter-converter.ts` | ➕ New: JSON Schema → ToolParameterDescription converter |
| `sdk/core/utils/tools/index.ts` | 📝 Updated: Export new modules |
| `sdk/resources/predefined/tools/tool-descriptions.ts` | ➕ New: Predefined tool descriptions registration |
| `sdk/resources/predefined/tools/index.ts` | 📝 Updated: Export registration functions |
| `sdk/resources/predefined/prompts/system/system-prompt-builder.ts` | ➕ New: System prompt builder with tool integration |
| `sdk/resources/predefined/prompts/system/index.ts` | 📝 Updated: Export new builder functions |
| `sdk/resources/dynamic/prompts/fragments/available-tools.ts` | 📝 Updated: Use enhanced generator |
| `sdk/core/utils/tools/__tests__/tool-description-generator.test.ts` | 📝 Updated: Tests for new implementation |

## Migration Notes

### Removed Files
- ~~`enhanced-tool-description-generator.ts`~~ - Merged into `tool-description-generator.ts`

### API Changes

The function names have been unified:
- ✅ `generateToolDescription()` - Now uses ToolDescriptionData when available
- ✅ `generateToolListDescription()` - Now supports `groupByCategory` option
- ✅ `generateToolAvailabilitySection()` - New function for system prompts
- ✅ `getToolDescriptionData()` - New function to get rich tool descriptions

All functions maintain backward compatibility with existing code.
