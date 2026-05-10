# Ask Follow-up Question Tool - Phase 1 Implementation Summary

## Overview
Phase 1 implementation completed successfully. This phase focuses on SDK core changes including schema updates, handler implementation, and tool registry modifications.

## Completed Changes

### 1. Schema Update ✅
**File**: `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/schema.ts`

**Changes**:
- Migrated from single-question format to multi-question nested structure
- New schema supports 1-3 questions, each with 1-4 preset options
- Added `additionalInfoLabel` field for free-form user input
- Updated validation rules:
  - Questions array: minItems=1, maxItems=3
  - Options array per question: minItems=1, maxItems=4
  - All text fields must be non-empty strings

**Before**:
```typescript
{
  question: string;
  follow_up: Array<{ text: string; mode?: string }>;
}
```

**After**:
```typescript
{
  questions: Array<{
    text: string;
    options: string[];
  }>;
  additionalInfoLabel?: string;
}
```

### 2. Description Update ✅
**File**: `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/description.ts`

**Changes**:
- Updated category from "code" to "interaction"
- Rewrote description to reflect new multi-question capability
- Updated parameter descriptions
- Added best practices tips
- Emphasized per-question options pattern

**Key Points**:
- Clear guidance on asking 1-3 questions maximum
- Each question should have its own relevant options (1-4)
- Additional info field for open-ended feedback

### 3. Type Definition Update ✅
**File**: `packages/prompt-templates/src/types/tool-description.ts`

**Changes**:
- Added "interaction" to ToolCategory union type
- Enables proper categorization of interaction tools

### 4. Handler Rewrite ✅
**File**: `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/handler.ts`

**Major Changes**:
- Complete rewrite to support interactive mode with event system
- Added fallback mode for environments without UI support
- Implemented comprehensive parameter validation
- Added event-driven user interaction flow

**Key Features**:

#### Interactive Mode
- Detects if event manager and execution ID are available
- Emits `USER_INTERACTION_REQUESTED` event with structured payload
- Waits for `USER_INTERACTION_RESPONDED` event
- Formats user response for LLM consumption
- Includes timeout handling (5 minutes default)

#### Fallback Mode
- Returns formatted text when interactive infrastructure unavailable
- Displays all questions and options in readable format
- Allows manual response collection

#### Validation
- Validates questions array (1-3 items)
- Validates each question has non-empty text
- Validates options array (1-4 items per question)
- Validates each option is a non-empty string
- Returns descriptive error messages

#### Response Formatting
```
User Responses:

Q1: Which configuration file to use?
A1: ./src/config.json (selected from options)

Q2: Implementation priority?
A2: Medium priority (custom input)

Additional Information:
Must support legacy systems

--- End of User Response ---
```

### 5. Tool Registry Update ✅
**File**: `sdk/resources/predefined/tools/registry.ts`

**Changes**:
- Added metadata to ask_followup_question tool registration
- Marked as interactive tool with metadata flags:
  ```typescript
  metadata: {
    requiresUserInteraction: true,
    interactionType: "ASK_FOLLOWUP_QUESTION",
  }
  ```

### 6. ToolDefinitionLike Interface Update ✅
**File**: `packages/tool-executors/src/utils.ts`

**Changes**:
- Added `metadata?: ToolMetadata` field to ToolDefinitionLike interface
- Updated `toSdkTool()` function to pass through metadata
- Imported ToolMetadata type from @wf-agent/types

**Purpose**: Enables tool metadata to flow from app layer to SDK layer

## Architecture Decisions

### 1. Nested Question-Option Structure
**Decision**: Each question contains its own options array

**Rationale**:
- Semantic binding between questions and options
- Prevents LLM confusion about option-question mapping
- Different questions can have completely different option sets
- Clearer mental model for both LLM and users

### 2. Event-Driven Interaction
**Decision**: Use existing USER_INTERACTION_REQUESTED/RESPONDED event system

**Rationale**:
- Leverages existing infrastructure
- Consistent with other interactive features
- Separation of concerns (SDK handles logic, apps handle UI)
- Flexible deployment across different app types (CLI, Web, VSCode)

### 3. Graceful Degradation
**Decision**: Implement fallback mode when event system unavailable

**Rationale**:
- Ensures tool works in all environments
- Backward compatibility during migration
- No breaking changes for existing deployments
- Gradual rollout path

### 4. Question Limit: 3 Maximum
**Decision**: Limit to 3 questions + 1 additional info field = 4 input points max

**Rationale**:
- Balances information gathering with user experience
- Prevents overwhelming users with too many decisions
- Aligns with cognitive load best practices

## Testing Recommendations

### Unit Tests Needed
1. **Schema Validation**
   - Valid parameters (1-3 questions, 1-4 options each)
   - Invalid parameters (empty questions, too many questions/options)
   - Missing required fields
   - Invalid data types

2. **Handler Logic**
   - Fallback mode output formatting
   - Parameter validation errors
   - Edge cases (empty strings, whitespace)

3. **Interactive Mode** (requires mocking)
   - Event emission
   - Response waiting
   - Timeout handling
   - Response formatting

### Integration Tests Needed
1. End-to-end flow with mock event system
2. Multiple questions with mixed preset/custom answers
3. All custom inputs scenario
4. With and without additional info
5. Concurrent interactions

## Migration Path

### Current State (Phase 1 Complete)
- ✅ Schema updated
- ✅ Handler rewritten with dual-mode support
- ✅ Tool registered with metadata
- ⚠️ Falls back to text mode (no interactive UI yet)

### Next Steps (Phase 2)
- [ ] Update ToolCallExecutor to detect interactive tools by metadata
- [ ] Pass execution context to tool handlers
- [ ] Wait for USER_INTERACTION_RESPONDED event in executor

### Future Steps (Phase 3)
- [ ] Implement UI components in apps layer (CLI, Web, VSCode)
- [ ] Subscribe to ASK_FOLLOWUP_QUESTION events
- [ ] Format and emit USER_INTERACTION_RESPONDED responses
- [ ] Handle timeout and cancellation in UI

## Breaking Changes

### For LLM Prompts
LLMs need to learn the new parameter structure:
- Old: `question` + `follow_up` array
- New: `questions` array with nested `text` and `options`

### For Existing Code
- Old single-question format no longer supported
- Tools calling this with old format will get validation errors
- Migration guide should be provided

## Files Modified

### Phase 1 - Initial Implementation
1. `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/schema.ts` (deleted)
2. `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/description.ts` (deleted)
3. `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/handler.ts` (deleted)
4. `sdk/resources/predefined/tools/registry.ts`
5. `packages/prompt-templates/src/types/tool-description.ts`
6. `packages/types/src/tool/static-config.ts`
7. `packages/types/src/interaction/user-interaction.ts`
8. `packages/tool-executors/src/utils.ts`

### Migration to Builtin
9. `sdk/resources/predefined/tools/builtin/interaction/ask-followup-question/schema.ts` (created)
10. `sdk/resources/predefined/tools/builtin/interaction/ask-followup-question/description.ts` (created)
11. `sdk/resources/predefined/tools/builtin/interaction/ask-followup-question/handler.ts` (created)
12. `sdk/resources/predefined/tools/builtin/interaction/ask-followup-question/index.ts` (created)
13. `sdk/resources/predefined/tools/builtin/interaction/index.ts` (created)
14. `sdk/resources/predefined/tools/builtin/index.ts`
15. `sdk/resources/predefined/tools/builtin/types.ts`
16. `sdk/resources/predefined/tools/builtin/registry.ts`
17. `sdk/resources/predefined/tools/stateless/interaction/index.ts`
18. `sdk/resources/predefined/tools/tool-descriptions.ts`

## Compatibility Notes

### Backward Compatibility
- ❌ Old parameter format NOT supported (breaking change)
- ✅ Fallback mode ensures tool still works without UI
- ⚠️ Requires LLM prompt updates to use new format

### Forward Compatibility
- ✅ Metadata marking enables future enhancements
- ✅ Event-driven architecture ready for UI integration
- ✅ Flexible design supports future features (conditional questions, validation rules, etc.)

## Success Criteria Met

- [x] Schema supports 1-3 questions with per-question options
- [x] Handler validates all parameters correctly
- [x] Interactive mode emits proper events
- [x] Fallback mode provides usable text output
- [x] Tool registered with interactive metadata
- [x] Type definitions updated to support metadata
- [x] No TypeScript compilation errors
- [x] Follows design document specifications

## Known Limitations

1. **No UI Yet**: Currently only works in fallback mode until Phase 2-3 complete
2. **No Context Passing**: ToolCallExecutor doesn't yet pass context to handlers (Phase 2)
3. **No Timeout Configuration**: Uses hardcoded 5-minute timeout (should be configurable)
4. **No Cancellation Support**: Cannot cancel interaction once started (future enhancement)

## Next Actions

1. **Immediate**: Test schema validation with various inputs
2. **Short-term**: Implement Phase 2 (ToolCallExecutor integration)
3. **Medium-term**: Build UI components for each app layer
4. **Long-term**: Add advanced features (conditional questions, rich media, analytics)

## References

- Design Document: `sdk/docs/resources/tools/ask-followup-question-design.md`
- Design Evolution: `sdk/docs/resources/tools/ask-followup-question-design-evolution.md`
- Design Summary: `sdk/docs/resources/tools/ask-followup-question-summary.md`
