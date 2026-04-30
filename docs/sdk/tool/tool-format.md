I'll analyze the tool format selection integration in the `tool-format-selector.ts` file.


Let me search for more specific usage of these functions:


Now let me check how these are used in the actual formatters:


Now let me check the base formatter to understand how these functions are integrated:


Now let me check where these functions are called from higher-level code:


Let me search for usage in the agent and graph modules:


Now let me check the types to understand the ToolCallFormatConfig structure:


Now let me create a comprehensive analysis document:

## Tool Format Selection Integration Analysis

Based on my analysis of the `tool-format-selector.ts` file and its usage throughout the codebase, here's a comprehensive overview of how tool format selection is integrated:

### **1. Core Functions Overview**

The `tool-format-selector.ts` module provides 7 key utility functions for managing tool call formats:

#### **A. Template Selection**
- **`getToolFormatTemplates(format, compact)`**: Returns appropriate prompt templates based on format
  - Maps each format to specific templates for tool lists, single tools, and parameters
  - Supports compact mode for context-limited scenarios
  - Formats supported: `function_call`, `xml`, `json_wrapped`, `json_raw`

#### **B. Parser Configuration**
- **`getToolCallParserOptions(format, customMarkers)`**: Generates parser options for text-based formats
  - Returns preferred parsing formats (XML, JSON, raw)
  - Configures custom markers for wrapped JSON
  - Enables fallback strategies for unknown formats

#### **C. Configuration Resolution**
- **`resolveToolCallFormatConfig(config)`**: Normalizes and applies defaults to format configuration
  - Merges user config with format-specific defaults
  - Falls back to `function_call` if no config provided
  - Uses `getDefaultFormatConfig()` from types package

#### **D. Feature Detection**
- **`requiresPromptToolDescriptions(format)`**: Determines if tools need descriptions in prompts
  - Returns `false` for `function_call` (uses API's native tool schema)
  - Returns `true` for text-based formats (XML/JSON)

- **`requiresCustomParsing(format)`**: Checks if custom parsing is needed
  - Returns `false` for `function_call` (API handles parsing)
  - Returns `true` for text-based formats (manual extraction required)

#### **E. Validation & UI Helpers**
- **`validateToolFormatCompatibility(profileFormat, promptFormat)`**: Validates format consistency
  - Detects mismatches between profile and prompt configurations
  - Special handling for JSON variants (wrapped vs raw)
  
- **`getToolFormatDisplayName(format)`**: Human-readable format names
- **`getToolFormatDescription(format)`**: Detailed format descriptions
- **`getAvailableToolFormats()`**: Lists all supported formats with metadata

---

### **2. Integration Points**

#### **A. Formatter Layer (Primary Usage)**

**OpenAI Chat Formatter** (`openai-chat.ts`):
```typescript
// Line 193: Parse text-mode responses
const toolCalls = ToolCallParser.parseFromText(
  content,
  getToolCallParserOptions(format, config.toolCallFormat?.markers)
);

// Lines 350-368: Build text-mode requests
const toolDeclarations = ToolDeclarationFormatter.formatTools(
  request.tools || [],
  {
    format: format === 'json_wrapped' || format === 'json_raw' ? 'json' : 'xml',
    xmlTags: config.toolCallFormat?.xmlTags,
    markers: config.toolCallFormat?.markers,
    includeDescription: config.toolCallFormat?.includeDescription,
  }
);
```

**Anthropic Formatter** (`anthropic.ts`):
```typescript
// Line 206: Parse text-mode responses
const toolCalls = ToolCallParser.parseFromText(
  content,
  getToolCallParserOptions(config.toolCallFormat.format, config.toolCallFormat.markers)
);

// Lines 79-87: Build text-mode requests with similar pattern
```

Both formatters use a **mode-aware routing pattern**:
- **Native mode** (`function_call`): Uses provider's native tool API
- **Text mode** (XML/JSON): Injects tool descriptions into prompts + manual parsing

#### **B. Base Formatter Infrastructure** (`base.ts`)

The base class provides the routing logic:
```typescript
// Lines 455-477: Mode-aware request/response handling
protected buildModeAwareRequest(request, config) {
  if (this.isTextBasedToolMode(config)) {
    return this.buildTextModeRequest(request, config);
  }
  return this.buildNativeRequest(request, config);
}

protected parseModeAwareResponse(data, config) {
  if (this.isTextBasedToolMode(config)) {
    return this.parseTextModeResponse(data, config);
  }
  return this.parseNativeResponse(data, config);
}
```

#### **C. Type System Integration**

**ToolCallFormatConfig** (`packages/types/src/llm/tool-call-format.ts`):
```typescript
interface ToolCallFormatConfig {
  format: ToolCallFormat;
  markers?: ToolCallFormatMarkers;      // For json_wrapped
  xmlTags?: ToolCallXmlTags;            // For xml
  includeDescription: boolean;
  descriptionStyle: "detailed" | "compact" | "minimal";
  includeExamples?: boolean;
  includeRules?: boolean;
}
```

**LLM Profile** (`packages/types/src/llm/profile.ts`):
```typescript
interface LLMProfile {
  // ... other fields
  toolCallFormat?: ToolCallFormatConfig;
}
```

---

### **3. Architecture Flow**

```
User Configuration (Profile/Node Config)
         ↓
ToolCallFormatConfig (with format, markers, xmlTags, etc.)
         ↓
FormatterConfig (passed to formatters)
         ↓
┌─────────────────────────────────────┐
│  BaseFormatter.buildRequest()       │
│  ├─ isTextBasedToolMode()?          │
│  │   ├─ YES → buildTextModeRequest  │
│  │   │   ├─ getToolFormatTemplates  │
│  │   │   ├─ ToolDeclarationFormatter│
│  │   │   └─ HistoryConverter        │
│  │   └─ NO  → buildNativeRequest    │
│  └─ HTTP Request sent               │
└─────────────────────────────────────┘
         ↓
LLM Response
         ↓
┌─────────────────────────────────────┐
│  BaseFormatter.parseResponse()      │
│  ├─ isTextBasedToolMode()?          │
│  │   ├─ YES → parseTextModeResponse │
│  │   │   └─ getToolCallParserOptions│
│  │   │       └─ ToolCallParser      │
│  │   └─ NO  → parseNativeResponse   │
│  └─ LLMResult returned              │
└─────────────────────────────────────┘
```

---

### **4. Key Design Patterns**

#### **A. Strategy Pattern**
Different formats use different strategies:
- **Native**: Delegate to provider's API
- **Text-based**: Manual prompt injection + parsing

#### **B. Template Method Pattern**
Base formatter defines the workflow, subclasses implement format-specific logic:
```typescript
abstract buildNativeRequest()     // Must implement
buildTextModeRequest()            // Optional override
abstract parseNativeResponse()    // Must implement
parseTextModeResponse()           // Optional override
```

#### **C. Configuration Cascading**
```
Global Defaults → Profile Config → Node Config → Runtime Override
```

---

### **5. Current Limitations & Observations**

1. **Unused Functions**: 
   - `getToolFormatTemplates()` is exported but not actively used in current code
   - `requiresPromptToolDescriptions()` and `requiresCustomParsing()` are exported but not called internally
   - These appear to be **utility functions for external consumers** or future extensibility

2. **Tight Coupling**: 
   - Formatters directly access `config.toolCallFormat?.format` instead of using helper functions
   - Could benefit from using `getToolCallFormat()` consistently

3. **Missing Integration**:
   - No validation calls to `validateToolFormatCompatibility()` in the execution path
   - Configuration errors would only surface at runtime during parsing

4. **Template System Gap**:
   - `getToolFormatTemplates()` returns templates but they're not used by current formatters
   - Formatters use `ToolDeclarationFormatter` from `@wf-agent/prompt-templates` instead

---

### **6. Recommendations**

1. **Use Helper Functions Consistently**: Replace direct format checks with `isTextBasedToolMode()` and `getToolCallFormat()`

2. **Add Validation Hooks**: Call `validateToolFormatCompatibility()` during config resolution to catch mismatches early

3. **Integrate Template System**: Either use `getToolFormatTemplates()` in formatters or remove it if redundant

4. **Document Extension Points**: Clarify when to override `buildTextModeRequest()` vs using default behavior

5. **Add Format Migration Warnings**: Use `migrateToolMode()` to detect legacy configs and warn users

---

### **Summary**

The tool format selector provides a **well-structured abstraction layer** for multi-format tool calling support. It's primarily integrated through:
- **Configuration resolution** in formatter setup
- **Parser option generation** for text-based formats
- **Mode-aware routing** in the base formatter

The architecture successfully separates concerns between format selection, template management, and parsing logic, though some utility functions remain underutilized in the current implementation.