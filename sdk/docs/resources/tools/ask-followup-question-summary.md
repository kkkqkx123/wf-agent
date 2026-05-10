# Ask Follow-up Question Tool - Design Summary

## Key Design Decisions

### 1. Nested Question-Option Structure

**Design**:
```typescript
questions: Array<{
  text: string;        // The question
  options: string[];   // Options specific to THIS question (1-4 items)
}>
```

**Rationale**:
- ✅ Questions and options are semantically bound together
- ✅ Prevents LLM confusion about which options belong to which question
- ✅ Each question can have different, contextually relevant options
- ✅ Clearer mental model for both LLM and users

**Why NOT shared options?**
- ❌ Separating questions and options increases cognitive load
- ❌ Different questions often need completely different option sets
- ❌ Risk of LLM mismatching options to wrong questions

### 2. Questions Included in Response

**Why**: Prevents LLM confusion about which answer maps to which question.

**Format**:
```
User Responses:

Q1: Which file path should I use?
A1: ./src/config.json (selected from options)

Q2: What's your preferred approach?
A2: Hybrid approach (custom input)
```

### 3. Array vs List for Answers

**Decision**: Use **Array** (ordered list)

**Rationale**:
- ✅ Maintains question-answer correspondence by index
- ✅ Predictable iteration order
- ✅ Easy to validate completeness
- ❌ Unordered list would require explicit mapping

### 4. Question Limit: 3 Maximum

**Calculation**:
- 3 questions + 1 additional info field = **4 input points max**
- Balances information gathering with user experience
- Prevents overwhelming users with too many decisions

**Comparison**:
- Old design: Up to 5 questions (too many)
- New design: Max 3 questions + free-form feedback (optimal)

## Data Flow Example

### LLM Request
```json
{
  "questions": [
    {
      "text": "Which configuration file to use?",
      "options": ["./src/config.json", "./config/app.json"]
    },
    {
      "text": "Implementation priority?",
      "options": ["High priority", "Low priority"]
    }
  ],
  "additionalInfoLabel": "Any constraints?"
}
```

### User Response
```typescript
{
  interactionId: "abc123",
  inputData: {
    answers: [
      {
        questionIndex: 0,
        selectedOptionIndex: 0,
        answer: "./src/config.json"
      },
      {
        questionIndex: 1,
        selectedOptionIndex: -1,
        customInput: "Medium priority",
        answer: "Medium priority"
      }
    ],
    additionalInfo: "Must support legacy systems"
  }
}
```

### Tool Result to LLM
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

### UI Design Principles

### Per-Question Options Pattern
```
┌──────────────────────────────────────┐
│ Q1: Which configuration file?        │
│ ○ ./src/config.json                  │
│ ○ ./config/app.json                  │
│ ○ Custom: [_______]                  │
├──────────────────────────────────────┤
│ Q2: Implementation priority?         │
│ ○ High priority                      │
│ ○ Low priority                       │
│ ○ Custom: [_______]                  │
├──────────────────────────────────────┤
│ Additional info:                     │
│ [______________________________]     │
└──────────────────────────────────────┘
```

**Benefits**:
- Clear question-option association
- Contextually relevant options per question
- Reduces cognitive load for users
- Prevents option-question mismatch

## Migration Notes

### Breaking Changes
- Old single-question format deprecated
- Options now nested within each question object
- No more shared `options` array at top level

### Backward Compatibility
- Handler includes fallback mode (returns formatted text)
- Gradual migration path available
- Old tools will still work but won't get interactive UI

## Implementation Checklist

### SDK Layer
- [ ] Update schema to new structure
- [ ] Rewrite handler with simplified params
- [ ] Add context passing to ToolRegistry
- [ ] Update tool metadata marker

### ToolCallExecutor
- [ ] Detect interactive tools by metadata
- [ ] Pass execution context to handlers
- [ ] Wait for USER_INTERACTION_RESPONDED event

### Apps Layer
- [ ] Subscribe to ASK_FOLLOWUP_QUESTION events
- [ ] Implement shared options UI component
- [ ] Format response with question text included
- [ ] Handle timeout and cancellation

## Testing Scenarios

1. **Single question, preset option selected**
2. **Multiple questions, mixed preset/custom**
3. **All custom inputs**
4. **With additional info**
5. **Without additional info**
6. **Timeout handling**
7. **Fallback mode (no UI)**
8. **Invalid parameter validation**
