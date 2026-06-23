# LLM Formatter Architecture: parseNativeResponse Design

## Overview

This document explains the architectural design decisions behind the `parseNativeResponse` method signature and its `config` parameter usage across different LLM providers.

## Core Design Decision

### Method Signature

```typescript
protected abstract parseNativeResponse(
  data: unknown,
  config: FormatterConfig
): LLMResult;
```

**Key Point**: The `config` parameter is **required** in the method signature, but **not all implementations use it**.

## Why This Design?

### 1. Unified Interface for Mode-Aware Parsing

The formatter architecture supports two modes:

- **Native Mode**: Uses provider's native function-calling API (e.g., OpenAI `tools`, Anthropic `tools`)
- **Text Mode**: Describes tools in prompt text (XML/JSON formats) for models without native support

Both modes share a common entry point:

```typescript
// In BaseFormatter
protected parseModeAwareResponse(data: unknown, config: FormatterConfig): LLMResult {
  if (this.isTextBasedToolMode(config)) {
    return this.parseTextModeResponse(data, config); // Always needs config
  } else {
    return this.parseNativeResponse(data, config);   // Config may not be needed
  }
}
```

**Design Rationale**: Maintaining a consistent signature simplifies the calling code and allows polymorphic behavior.

### 2. Provider-Specific API Differences

Different LLM providers have different response structures:

| Provider | Response Includes `model`? | Needs `config`? | Reason |
|----------|---------------------------|-----------------|--------|
| OpenAI | ✅ Yes | ❌ No | `data.model` available |
| Anthropic | ✅ Yes | ❌ No | `data.model` available |
| Gemini Native | ❌ No | ✅ Yes | Must use `config.profile.model` |

**Gemini Example**:
```typescript
// Gemini Native API response does NOT include model field
protected parseNativeResponse(data: unknown, config: FormatterConfig): LLMResult {
  const dataRecord = data as Record<string, unknown>;
  
  // Must get model from config because Gemini API doesn't provide it
  const model = config.profile.model;
  
  return {
    id: dataRecord["id"] as string,
    model: model,  // <-- From config, not response
    content: this.extractContent(dataRecord["candidates"]),
    // ...
  };
}
```

### 3. Future-Proofing

Keeping `config` in the signature allows future implementations to:
- Access timeout settings
- Read custom provider options
- Implement provider-specific logic without interface changes

## Implementation Patterns

### Pattern A: Config Not Used (OpenAI, Anthropic)

```typescript
protected parseNativeResponse(data: unknown, _config: FormatterConfig): LLMResult {
  // All information extracted from response data
  const dataRecord = data as Record<string, unknown>;
  
  return {
    id: dataRecord["id"] as string,
    model: dataRecord["model"] as string,  // From response
    content: this.extractContent(dataRecord["content"]),
    toolCalls: this.extractToolCalls(dataRecord["content"]),
  };
}
```

**Note**: `_config` prefix indicates intentional non-usage (TypeScript convention).

### Pattern B: Config Required (Gemini)

```typescript
protected parseNativeResponse(data: unknown, config: FormatterConfig): LLMResult {
  const dataRecord = data as Record<string, unknown>;
  
  // Config required - response lacks model field
  const model = config.profile.model;
  
  return {
    id: dataRecord["id"] as string,
    model: model,  // From config
    content: this.extractContent(dataRecord["candidates"]),
    // ...
  };
}
```

## Alternative Designs Considered

### Alternative 1: Optional Config Parameter

```typescript
protected abstract parseNativeResponse(
  data: unknown,
  config?: FormatterConfig  // Make optional
): LLMResult;
```

**Rejected Because**:
- Breaks consistency with `parseTextModeResponse` (always needs config)
- Callers must always pass config anyway (for mode-aware dispatch)
- Doesn't solve the lint warning issue (still need `_` prefix when unused)

### Alternative 2: Separate Methods Per Provider

```typescript
// Different signatures for different needs
protected abstract parseNativeResponseWithConfig(data: unknown, config: FormatterConfig): LLMResult;
protected abstract parseNativeResponseSimple(data: unknown): LLMResult;
```

**Rejected Because**:
- Destroys polymorphism - can't call uniformly
- Forces conditional logic in caller
- Violates Liskov Substitution Principle

### Alternative 3: Context Object Pattern

```typescript
interface ParseContext {
  data: unknown;
  config?: FormatterConfig;
  metadata?: Record<string, any>;
}

protected abstract parseNativeResponse(context: ParseContext): LLMResult;
```

**Rejected Because**:
- Over-engineering for current needs
- Adds indirection without clear benefit
- Makes simple cases more complex

## Current Design Benefits

✅ **Consistency**: All formatters implement the same interface  
✅ **Simplicity**: Callers don't need conditional logic  
✅ **Flexibility**: Providers can use config when needed  
✅ **Extensibility**: Easy to add new providers with different requirements  
✅ **Type Safety**: TypeScript enforces correct signature  

## Trade-offs

⚠️ **Lint Warnings**: Some implementations have unused `config` parameter  
⚠️ **Clarity**: May confuse developers why some methods don't use config  

**Mitigation**: 
- Use `_config` naming convention for intentionally unused parameters
- Document the design decision clearly (this document)
- ESLint rule can be configured to allow this pattern

## Best Practices

### For New Provider Implementations

1. **Check if response includes all needed fields**
   - If yes → use `_config` prefix
   - If no → use `config` to access missing data

2. **Document why config is/isn't used**
   ```typescript
   // Config not used: OpenAI response includes model field
   protected parseNativeResponse(data: unknown, _config: FormatterConfig): LLMResult {
     // ...
   }
   
   // Config required: Gemini response lacks model field
   protected parseNativeResponse(data: unknown, config: FormatterConfig): LLMResult {
     // ...
   }
   ```

3. **Use helper methods for common patterns**
   ```typescript
   // In BaseFormatter
   protected getModelFromResponseOrConfig(
     data: unknown,
     config: FormatterConfig
   ): string {
     const dataRecord = data as Record<string, unknown>;
     return (dataRecord["model"] as string) || config.profile.model;
   }
   ```

## Conclusion

The current design prioritizes **architectural consistency** and **extensibility** over eliminating all lint warnings. The `config` parameter in `parseNativeResponse` is a deliberate design choice that:

1. Maintains a uniform interface across all providers
2. Accommodates provider-specific API differences (especially Gemini)
3. Simplifies the mode-aware parsing dispatch logic
4. Allows future enhancements without breaking changes

This is a case where **pragmatic design** (accepting some unused parameters) is better than **theoretical purity** (complex conditional interfaces).
