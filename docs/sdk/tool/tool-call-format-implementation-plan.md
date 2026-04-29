# Tool Call Format Implementation Analysis and Plan

## Overview

This document provides a comprehensive analysis of the current state of tool call format support in the SDK and outlines a detailed implementation plan to achieve full XML/JSON mode support as described in the LimCode comparison analysis.

**Analysis Date**: 2026-04-29  
**Status**: Partial Implementation (~25% complete)

---

## Current State Assessment

### ✅ Completed Components

1. **Type Definitions** (Phase 1 - Complete)
   - `ToolCallFormat` type in `packages/types/src/llm/tool-call-format.ts`
   - `ToolCallFormatConfig` interface with comprehensive configuration options
   - Helper functions: `migrateToolMode()`, `getDefaultFormatConfig()`, `validateToolCallFormatConfig()`
   - Integration into `LLMProfile` and `FormatterConfig`

2. **Parsing Capability** (Infrastructure Ready)
   - `ToolCallParser` class in `sdk/core/llm/formatters/tool-call-parser.ts`
   - Methods: `parseXMLToolCalls()`, `parseJSONToolCalls()`, `parseRawJsonToolCalls()`, `parseFromText()`
   - Exposed through `BaseFormatter` helper methods

3. **Format Selection Utilities**
   - `tool-format-selector.ts` with `resolveToolCallFormatConfig()`
   - Template selection based on format type

### ❌ Missing Components

1. **Tool Declaration Generation** (Phase 2 - Not Started)
   - No `ToolDeclarationFormatter` implementation
   - Current system uses `ToolDescriptionGenerator` for text-based descriptions only
   - Cannot generate XML/JSON format tool declarations for prompts

2. **Formatter Mode Awareness** (Phase 3 - Not Started)
   - All formatters (`OpenAIChatFormatter`, `AnthropicFormatter`, etc.) only support native function-calling
   - No conditional logic based on `toolCallFormat` configuration
   - Formatters don't inject tool declarations into system messages
   - Response parsing doesn't use `ToolCallParser` for XML/JSON modes

3. **History Conversion** (Phase 4 - Not Started)
   - No `HistoryConverter` implementation
   - Cannot convert message history between native and text-based formats
   - Tool calls in history remain in native format even when using XML/JSON mode

---

## Implementation Architecture

### Design Principles

1. **Separation of Concerns**
   - Keep Formatter职责单一 (single responsibility): format conversion only
   - Tool declaration generation should be separate from message formatting
   - History conversion should be a preprocessing step before formatter invocation

2. **Layered Architecture**
   ```
   Execution Layer (ExecutionCoordinator/PromptAssembler)
       ↓ [Pre-processes messages, injects tool declarations]
   Formatter Layer (BaseFormatter subclasses)
       ↓ [Converts to API-specific format]
   HTTP Client Layer
   ```

3. **Backward Compatibility**
   - Default behavior remains native function-calling
   - XML/JSON modes are opt-in via configuration
   - Existing code should continue working without changes

---

## Detailed Implementation Plan

### Phase 2: Tool Declaration Generation

#### 2.1 Create ToolDeclarationFormatter

**Location**: `packages/common-utils/src/tool/declaration-formatter.ts`

**Purpose**: Convert tool schemas and tool calls to XML/JSON format strings for prompt injection.

**API Design**:

```typescript
export interface ToolDeclarationOptions {
  /** Output format */
  format: 'xml' | 'json';
  /** Include parameter details */
  includeParameters?: boolean;
  /** Include description */
  includeDescription?: boolean;
  /** Custom XML tags (for XML format) */
  xmlTags?: ToolCallXmlTags;
  /** Custom markers (for JSON format) */
  markers?: ToolCallFormatMarkers;
}

export class ToolDeclarationFormatter {
  /**
   * Convert tool schemas to declaration string
   * Used for injecting into system prompt
   */
  static formatTools(tools: ToolSchema[], options: ToolDeclarationOptions): string;

  /**
   * Convert a single tool call to text format
   * Used for converting assistant messages in history
   */
  static formatToolCall(toolCall: LLMToolCall, options: ToolDeclarationOptions): string;

  /**
   * Convert tool result to text format
   * Used for converting tool messages in history
   */
  static formatToolResult(message: LLMMessage, options: ToolDeclarationOptions): string;

  /**
   * Convert array of tool calls to text
   */
  static formatToolCalls(toolCalls: LLMToolCall[], options: ToolDeclarationOptions): string;
}
```

**Implementation Details**:

**XML Format Example**:
```xml
<tools>
  <tool name="calculator">
    <description>Performs arithmetic calculations</description>
    <parameters>
      - a (required) [number]: First operand
      - b (required) [number]: Second operand
      - operation (required) [string]: Operation: add, subtract, multiply, divide
    </parameters>
  </tool>
</tools>
```

**Tool Call XML Format**:
```xml
<tool_use>
  <tool_name>calculator</tool_name>
  <parameters>
    <a>10</a>
    <b>5</b>
    <operation>add</operation>
  </parameters>
</tool_use>
```

**JSON Format Example**:
```json
<<<TOOL_CALL>>>
{
  "tool": "calculator",
  "parameters": {
    "a": 10,
    "b": 5,
    "operation": "add"
  }
}
<<<END_TOOL_CALL>>>
```

#### 2.2 Integration with Prompt Templates

**Location**: `packages/prompt-templates/src/templates/tools/`

**Task**: Create new prompt templates for XML and JSON tool declarations.

**Files to Create**:
- `tools-xml-declaration.ts` - Template for XML format tool list
- `tools-json-declaration.ts` - Template for JSON format tool list
- `tool-usage-xml-instructions.ts` - Instructions for using XML format
- `tool-usage-json-instructions.ts` - Instructions for using JSON format

**Integration Point**: 
The `PromptAssembler` or execution coordinator should:
1. Check `toolCallFormat` configuration
2. Select appropriate template based on format
3. Generate tool declarations using `ToolDeclarationFormatter`
4. Inject into system message or as separate message

---

### Phase 3: Formatter Mode Awareness

#### 3.1 Enhance BaseFormatter

**Location**: `sdk/core/llm/formatters/base.ts`

**Current State**: Abstract methods don't consider format mode.

**Proposed Changes**:

Add protected helper methods to base class:

```typescript
export abstract class BaseFormatter {
  // ... existing methods ...

  /**
   * Check if using text-based tool format (XML/JSON)
   */
  protected isTextBasedToolMode(config: FormatterConfig): boolean {
    const format = config.toolCallFormat?.format || 'function_call';
    return format === 'xml' || format === 'json_wrapped' || format === 'json_raw';
  }

  /**
   * Get tool call format from config
   */
  protected getToolCallFormat(config: FormatterConfig): ToolCallFormat {
    return config.toolCallFormat?.format || 'function_call';
  }

  /**
   * Build request with mode awareness
   * Subclasses should override this instead of buildRequest
   */
  protected buildModeAwareRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult {
    if (this.isTextBasedToolMode(config)) {
      return this.buildTextModeRequest(request, config);
    }
    return this.buildNativeRequest(request, config);
  }

  /**
   * Parse response with mode awareness
   * Subclasses should override this instead of parseResponse
   */
  protected parseModeAwareResponse(
    data: unknown,
    config: FormatterConfig
  ): LLMResult {
    if (this.isTextBasedToolMode(config)) {
      return this.parseTextModeResponse(data, config);
    }
    return this.parseNativeResponse(data, config);
  }

  // Methods for subclasses to implement
  protected abstract buildNativeRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult;

  protected abstract buildTextModeRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult;

  protected abstract parseNativeResponse(
    data: unknown,
    config: FormatterConfig
  ): LLMResult;

  protected abstract parseTextModeResponse(
    data: unknown,
    config: FormatterConfig
  ): LLMResult;
}
```

**Migration Strategy**:
- Keep existing `buildRequest()` and `parseResponse()` as wrappers
- Gradually migrate subclasses to use new pattern
- Maintain backward compatibility during transition

#### 3.2 Implement OpenAIChatFormatter Text Mode Support

**Location**: `sdk/core/llm/formatters/openai-chat.ts`

**Key Changes**:

```typescript
export class OpenAIChatFormatter extends BaseFormatter {
  // ... existing code ...

  protected buildNativeRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult {
    // Current implementation - uses native tools field
    const body = this.buildRequestBody(request, config);
    // ... rest of current logic
  }

  protected buildTextModeRequest(
    request: LLMRequest,
    config: FormatterConfig
  ): BuildRequestResult {
    // 1. Generate tool declarations in XML/JSON format
    const toolDeclarations = ToolDeclarationFormatter.formatTools(
      request.tools || [],
      {
        format: this.getToolCallFormat(config),
        xmlTags: config.toolCallFormat?.xmlTags,
        markers: config.toolCallFormat?.markers,
      }
    );

    // 2. Convert history messages to text format
    const convertedMessages = HistoryConverter.convertToTextMode(
      request.messages,
      this.getToolCallFormat(config),
      config.toolCallFormat
    );

    // 3. Inject tool declarations into system message
    const messagesWithTools = this.injectToolDeclarations(
      convertedMessages,
      toolDeclarations,
      this.getToolCallFormat(config)
    );

    // 4. Build request WITHOUT tools field
    const body: Record<string, unknown> = {
      model: config.profile.model,
      messages: this.convertMessages(messagesWithTools),
      stream: config.stream || false,
      // ... other parameters but NO tools field
    };

    // ... rest of request building
  }

  protected parseNativeResponse(
    data: unknown,
    config: FormatterConfig
  ): LLMResult {
    // Current implementation - parses native tool_calls
    return this.parseResponse(data, config);
  }

  protected parseTextModeResponse(
    data: unknown,
    config: FormatterConfig
  ): LLMResult {
    const dataRecord = data as Record<string, unknown>;
    const choices = dataRecord["choices"] as Array<Record<string, unknown>> | undefined;
    const choice = choices?.[0];
    const message = choice?.["message"] as Record<string, unknown> | undefined;

    const content = (message?.["content"] as string) || "";

    // Parse tool calls from content using ToolCallParser
    const toolCalls = ToolCallParser.parseFromText(
      content,
      getToolCallParserOptions(config.toolCallFormat)
    );

    return {
      id: dataRecord["id"] as string,
      model: dataRecord["model"] as string,
      content,
      message: {
        role: "assistant",
        content,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      },
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      // ... rest of result
    };
  }

  private injectToolDeclarations(
    messages: LLMMessage[],
    toolDeclarations: string,
    format: ToolCallFormat
  ): LLMMessage[] {
    // Find or create system message
    const systemMsgIndex = messages.findIndex(m => m.role === 'system');
    const instructions = this.getToolUsageInstructions(format);

    if (systemMsgIndex >= 0) {
      // Append to existing system message
      const updated = [...messages];
      updated[systemMsgIndex] = {
        ...updated[systemMsgIndex],
        content: `${updated[systemMsgIndex].content}\n\n${instructions}\n\n${toolDeclarations}`,
      };
      return updated;
    } else {
      // Prepend new system message
      return [
        {
          role: 'system',
          content: `${instructions}\n\n${toolDeclarations}`,
        },
        ...messages,
      ];
    }
  }

  private getToolUsageInstructions(format: ToolCallFormat): string {
    // Return appropriate instructions based on format
    // Could use prompt templates here
    if (format === 'xml') {
      return 'Use XML format for tool calls: <tool_use>...</tool_use>';
    } else if (format === 'json_wrapped') {
      return 'Use JSON format with markers: <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>>';
    }
    return '';
  }
}
```

#### 3.3 Implement AnthropicFormatter Text Mode Support

**Location**: `sdk/core/llm/formatters/anthropic.ts`

**Similar Pattern**:
- Override `buildTextModeRequest()` to inject tools into system field
- Override `parseTextModeResponse()` to parse from content.text
- Handle Anthropic-specific message structure

**Special Considerations for Anthropic**:
- System message goes in `system` field, not messages array
- Content can be array of blocks (text, tool_use, etc.)
- Need to handle both native and text modes carefully

#### 3.4 Implement GeminiFormatter Text Mode Support

**Location**: `sdk/core/llm/formatters/gemini-native.ts`

**Pattern**: Similar to OpenAI but with Gemini-specific structures.

---

### Phase 4: History Conversion

#### 4.1 Create HistoryConverter

**Location**: `sdk/core/messages/history-converter.ts`

**Purpose**: Convert message history between native and text-based formats.

**API Design**:

```typescript
export interface HistoryConversionOptions {
  /** Target format */
  targetFormat: ToolCallFormat;
  /** Custom XML tags */
  xmlTags?: ToolCallXmlTags;
  /** Custom markers */
  markers?: ToolCallFormatMarkers;
}

export class HistoryConverter {
  /**
   * Convert entire message history to text-based format
   */
  static convertToTextMode(
    messages: LLMMessage[],
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage[];

  /**
   * Convert single assistant message with tool calls to text
   */
  static convertAssistantMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage;

  /**
   * Convert single tool result message to text
   */
  static convertToolResultMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage;

  /**
   * Check if message needs conversion
   */
  static needsConversion(
    message: LLMMessage,
    targetFormat: ToolCallFormat
  ): boolean;
}
```

**Implementation Details**:

```typescript
export class HistoryConverter {
  static convertToTextMode(
    messages: LLMMessage[],
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage[] {
    if (format === 'function_call') {
      // No conversion needed for native mode
      return messages;
    }

    return messages.map(msg => {
      if (msg.role === 'assistant' && msg.toolCalls?.length) {
        return this.convertAssistantMessage(msg, format, options);
      }
      if (msg.role === 'tool' && msg.toolCallId) {
        return this.convertToolResultMessage(msg, format, options);
      }
      return msg;
    });
  }

  static convertAssistantMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage {
    if (!message.toolCalls || message.toolCalls.length === 0) {
      return message;
    }

    const toolCallText = ToolDeclarationFormatter.formatToolCalls(
      message.toolCalls,
      {
        format: format === 'json_wrapped' ? 'json' : 'xml',
        xmlTags: options?.xmlTags,
        markers: options?.markers,
      }
    );

    // Combine existing content with tool calls
    const combinedContent = message.content
      ? `${message.content}\n\n${toolCallText}`
      : toolCallText;

    return {
      role: 'assistant',
      content: combinedContent,
      // Remove toolCalls field - they're now in content
    };
  }

  static convertToolResultMessage(
    message: LLMMessage,
    format: ToolCallFormat,
    options?: Partial<HistoryConversionOptions>
  ): LLMMessage {
    const resultText = ToolDeclarationFormatter.formatToolResult(
      message,
      {
        format: format === 'json_wrapped' ? 'json' : 'xml',
        xmlTags: options?.xmlTags,
        markers: options?.markers,
      }
    );

    return {
      role: 'user', // Tool results become user messages in text mode
      content: resultText,
      // Remove toolCallId field
    };
  }
}
```

**Example Conversions**:

**Native → XML**:
```typescript
// Before (native)
{
  role: 'assistant',
  content: 'Let me calculate that.',
  toolCalls: [{
    id: 'call_123',
    type: 'function',
    function: {
      name: 'calculator',
      arguments: '{"a":10,"b":5,"operation":"add"}'
    }
  }]
}

// After (XML)
{
  role: 'assistant',
  content: `Let me calculate that.

<tool_use>
  <tool_name>calculator</tool_name>
  <parameters>
    <a>10</a>
    <b>5</b>
    <operation>add</operation>
  </parameters>
</tool_use>`
}
```

**Native → JSON**:
```typescript
// After (JSON wrapped)
{
  role: 'assistant',
  content: `Let me calculate that.

<<<TOOL_CALL>>>
{
  "tool": "calculator",
  "parameters": {
    "a": 10,
    "b": 5,
    "operation": "add"
  }
}
<<<END_TOOL_CALL>>>`
}
```

---

## Implementation Workflow

### Recommended Order

1. **Week 1**: Implement `ToolDeclarationFormatter`
   - Create file structure
   - Implement XML generation
   - Implement JSON generation
   - Write unit tests

2. **Week 2**: Implement `HistoryConverter`
   - Create converter class
   - Implement message conversion logic
   - Write comprehensive tests
   - Test edge cases (empty content, multiple tool calls, etc.)

3. **Week 3**: Enhance `BaseFormatter` and implement OpenAI support
   - Add mode-aware methods to base class
   - Implement text mode for OpenAIChatFormatter
   - Test with mock LLM responses
   - Verify backward compatibility

4. **Week 4**: Implement Anthropic and Gemini support
   - Add text mode to AnthropicFormatter
   - Add text mode to GeminiFormatters
   - Cross-provider testing
   - Performance optimization

5. **Week 5**: Integration and Testing
   - End-to-end integration tests
   - Test with real LLM APIs (if available)
   - Performance benchmarking
   - Documentation updates

### Testing Strategy

#### Unit Tests
- `ToolDeclarationFormatter` format generation
- `HistoryConverter` message conversion
- Formatter mode switching logic
- Edge cases (malformed XML/JSON, empty tool lists, etc.)

#### Integration Tests
- Full request/response cycle with XML mode
- Full request/response cycle with JSON mode
- History preservation across multiple turns
- Mixed format scenarios

#### Regression Tests
- Ensure native function-calling still works
- Verify no breaking changes to existing APIs
- Test all formatter subclasses

---

## Key Challenges and Solutions

### Challenge 1: Maintaining Backward Compatibility

**Problem**: Existing code expects native function-calling behavior.

**Solution**:
- Make `toolCallFormat` optional with default `'function_call'`
- Keep existing method signatures unchanged
- Use feature flags or configuration for gradual rollout
- Deprecation warnings for old patterns (if any)

### Challenge 2: Streaming Response Handling

**Problem**: XML/JSON tool calls may span multiple streaming chunks.

**Solution**:
- Implement accumulator pattern (similar to LimCode's `StreamAccumulator`)
- Buffer partial tool calls until complete
- Use `ToolCallParser.parsePartial()` for best-effort parsing
- Emit incremental updates as tool calls are recognized

**Implementation**:
```typescript
class StreamingToolCallAccumulator {
  private buffer: string = '';
  private parsedCalls: LLMToolCall[] = [];

  append(chunk: string): { newCalls: LLMToolCall[]; remainingBuffer: string } {
    this.buffer += chunk;
    
    // Try to parse complete tool calls
    const newCalls = ToolCallParser.parseFromText(this.buffer, {
      allowPartial: false,
    });
    
    // Filter out already-parsed calls
    const trulyNew = newCalls.filter(
      nc => !this.parsedCalls.some(pc => pc.id === nc.id)
    );
    
    this.parsedCalls.push(...trulyNew);
    
    // Keep unparsed portion in buffer
    this.buffer = this.extractUnparsedPortion(this.buffer, newCalls);
    
    return { newCalls: trulyNew, remainingBuffer: this.buffer };
  }
}
```

### Challenge 3: Token Usage and Prompt Length

**Problem**: XML/JSON formats increase prompt length significantly.

**Solution**:
- Provide `compact` description style option
- Allow users to control parameter detail level
- Implement token estimation before sending request
- Warn users if prompt exceeds model limits
- Consider truncating less important tool descriptions

### Challenge 4: Error Handling and Recovery

**Problem**: Malformed XML/JSON from LLM responses.

**Solution**:
- Implement robust parsing with error recovery
- Log parsing failures for debugging
- Provide fallback mechanisms (retry with clearer instructions)
- Return partial results when possible
- Clear error messages for users

---

## Configuration Examples

### Example 1: Using XML Mode with OpenAI

```typescript
const profile: LLMProfile = {
  id: 'openai-xml',
  name: 'OpenAI with XML Tools',
  provider: 'OPENAI_CHAT',
  model: 'gpt-4',
  apiKey: process.env.OPENAI_API_KEY!,
  parameters: { temperature: 0.7 },
  toolCallFormat: {
    format: 'xml',
    includeDescription: true,
    descriptionStyle: 'compact',
    includeExamples: true,
    includeRules: true,
    xmlTags: DEFAULT_XML_TAGS,
  },
};
```

### Example 2: Using JSON Mode with Custom Markers

```typescript
const profile: LLMProfile = {
  id: 'anthropic-json',
  name: 'Anthropic with JSON Tools',
  provider: 'ANTHROPIC',
  model: 'claude-3-opus',
  apiKey: process.env.ANTHROPIC_API_KEY!,
  parameters: { max_tokens: 4096 },
  toolCallFormat: {
    format: 'json_wrapped',
    markers: {
      start: '[TOOL]',
      end: '[/TOOL]',
    },
    includeDescription: true,
    descriptionStyle: 'detailed',
  },
};
```

### Example 3: Migration from Legacy toolMode

```typescript
import { migrateToolMode } from '@wf-agent/types';

// Old way (if it existed)
const oldConfig = { toolMode: 'xml' };

// New way
const newConfig = {
  toolCallFormat: migrateToolMode(oldConfig.toolMode),
};
```

---

## Performance Considerations

### Impact Analysis

| Aspect | Native Mode | XML Mode | JSON Mode |
|--------|-------------|----------|-----------|
| Prompt Length | Baseline | +30-50% | +20-40% |
| Parsing Speed | Fast (API) | Medium | Medium |
| Generation Speed | N/A (API) | Fast | Fast |
| Model Compatibility | Limited | Universal | Universal |
| Reliability | High | Medium-High | Medium-High |

### Optimization Strategies

1. **Caching**: Cache generated tool declarations for repeated requests
2. **Lazy Generation**: Only generate declarations when tools are present
3. **Streaming Optimization**: Parse incrementally to reduce latency
4. **Batch Processing**: Process multiple tool calls together when possible

---

## Migration Guide for Users

### For Application Developers

**No Action Required** if using default native function-calling.

**To Enable XML/JSON Mode**:

1. Update `LLMProfile` configuration:
```typescript
profile.toolCallFormat = {
  format: 'xml', // or 'json_wrapped'
  // ... other options
};
```

2. Ensure your prompt templates support the chosen format:
```typescript
// If using custom prompts, include format-specific instructions
const prompt = `
You have access to these tools:
${toolDeclarations} // Generated automatically

When using tools, follow the ${format} format specified above.
`;
```

3. Test with your specific models to ensure reliable parsing.

### For SDK Contributors

**Adding Support for New Formatters**:

1. Extend `BaseFormatter` with mode-aware methods
2. Implement `buildTextModeRequest()` and `parseTextModeResponse()`
3. Add tests for both native and text modes
4. Update documentation

---

## Success Metrics

### Functional Metrics
- ✅ All formatters support native function-calling (existing)
- ✅ All formatters support XML mode (target)
- ✅ All formatters support JSON mode (target)
- ✅ History conversion preserves semantics (target)
- ✅ Streaming works for all modes (target)

### Quality Metrics
- Unit test coverage > 80%
- Integration test coverage for all providers
- Zero breaking changes to existing APIs
- Performance degradation < 10% for native mode

### Adoption Metrics
- Documented usage examples
- Migration guide completeness
- Community feedback incorporation

---

## Appendix: Reference Files

### Files to Create
1. `packages/common-utils/src/tool/declaration-formatter.ts`
2. `sdk/core/messages/history-converter.ts`
3. `packages/prompt-templates/src/templates/tools/tools-xml-declaration.ts`
4. `packages/prompt-templates/src/templates/tools/tools-json-declaration.ts`
5. `packages/prompt-templates/src/templates/tools/tool-usage-xml-instructions.ts`
6. `packages/prompt-templates/src/templates/tools/tool-usage-json-instructions.ts`

### Files to Modify
1. `sdk/core/llm/formatters/base.ts` - Add mode-aware methods
2. `sdk/core/llm/formatters/openai-chat.ts` - Implement text mode
3. `sdk/core/llm/formatters/anthropic.ts` - Implement text mode
4. `sdk/core/llm/formatters/gemini-native.ts` - Implement text mode
5. `sdk/core/llm/formatters/gemini-openai.ts` - Implement text mode
6. `sdk/core/llm/formatters/openai-response.ts` - Implement text mode

### Files Already Complete
1. `packages/types/src/llm/tool-call-format.ts` - Type definitions ✅
2. `packages/types/src/llm/profile.ts` - LLMProfile integration ✅
3. `sdk/core/llm/formatters/types.ts` - FormatterConfig integration ✅
4. `sdk/core/llm/formatters/tool-call-parser.ts` - Parsing capability ✅
5. `sdk/core/llm/formatters/tool-format-selector.ts` - Format selection ✅

---

## Conclusion

The SDK has solid foundation with type definitions and parsing infrastructure in place. The remaining work focuses on:

1. **Generation**: Creating XML/JSON format strings from tool schemas
2. **Integration**: Making formatters aware of and responsive to format configuration
3. **Conversion**: Transforming message history between formats

With an estimated **5 weeks** of focused development and comprehensive testing, the SDK can achieve full parity with LimCode's tool calling capabilities while maintaining superior architecture through separation of concerns and layered design.

The implementation plan prioritizes:
- **Backward compatibility** - No breaking changes
- **Incremental rollout** - Can enable per-profile
- **Testability** - Comprehensive test coverage
- **Maintainability** - Clean separation of responsibilities

This approach ensures the SDK remains robust, extensible, and production-ready throughout the enhancement process.