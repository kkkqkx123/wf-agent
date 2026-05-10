# Ask Follow-up Question Tool - Usage Example

## Quick Start

This guide shows how to use the `ask_followup_question` tool in your workflows.

## Basic Usage

### Single Question with Options

```typescript
// In your agent's system prompt or tool call
const response = await callTool("ask_followup_question", {
  questions: [
    {
      text: "Which file path should I use for the configuration?",
      options: [
        "./src/config.json",
        "./config/app.json",
        "./app.config.json"
      ]
    }
  ],
  additionalInfoLabel: "Any specific requirements?"
});
```

**CLI Output:**
```
============================================================
📝 Follow-up Questions
============================================================

Q1: Which file path should I use for the configuration?
  1. ./src/config.json
  2. ./config/app.json
  3. ./app.config.json
  4. [Custom input]

Enter your choice (1-4): 1
✓ Selected: ./src/config.json

Additional comments or information:
(Press Enter to skip)
> 

✅ Response submitted successfully
```

### Multiple Questions

```typescript
const response = await callTool("ask_followup_question", {
  questions: [
    {
      text: "Which database should we use?",
      options: ["PostgreSQL", "MySQL", "MongoDB"]
    },
    {
      text: "What's the deployment environment?",
      options: ["Development", "Staging", "Production"]
    },
    {
      text: "Implementation priority?",
      options: ["High", "Medium", "Low"]
    }
  ],
  additionalInfoLabel: "Additional constraints or requirements"
});
```

**CLI Output:**
```
============================================================
📝 Follow-up Questions
============================================================

Q1: Which database should we use?
  1. PostgreSQL
  2. MySQL
  3. MongoDB
  4. [Custom input]

Enter your choice (1-4): 1
✓ Selected: PostgreSQL

Q2: What's the deployment environment?
  1. Development
  2. Staging
  3. Production
  4. [Custom input]

Enter your choice (1-4): 3
✓ Selected: Production

Q3: Implementation priority?
  1. High
  2. Medium
  3. Low
  4. [Custom input]

Enter your choice (1-4): 2
✓ Selected: Medium

Additional constraints or requirements:
(Press Enter to skip)
> Must support high availability

✅ Response submitted successfully
```

### Custom Input

Users can provide custom answers by selecting the "Custom input" option:

```
Q1: What's your preferred framework?
  1. React
  2. Vue
  3. Angular
  4. [Custom input]

Enter your choice (1-4): 4
Please enter your custom response:
> Svelte

✓ Custom input: Svelte
```

## Best Practices

### 1. Keep Questions Focused

❌ **Bad:**
```typescript
{
  text: "Tell me about your project requirements, tech stack preferences, deployment strategy, and any other considerations?",
  options: ["...", "..."]
}
```

✅ **Good:**
```typescript
{
  questions: [
    { text: "What's the primary programming language?", options: ["TypeScript", "Python", "Go"] },
    { text: "Which framework do you prefer?", options: ["React", "Vue", "Angular"] },
    { text: "Where will this be deployed?", options: ["AWS", "Azure", "GCP"] }
  ]
}
```

### 2. Provide Relevant Options

Each question should have options directly related to that specific question:

✅ **Good:**
```typescript
{
  questions: [
    {
      text: "Database type?",
      options: ["SQL", "NoSQL", "Graph"]
    },
    {
      text: "Authentication method?",
      options: ["OAuth2", "JWT", "API Key"]
    }
  ]
}
```

❌ **Bad:**
```typescript
{
  questions: ["Database type?", "Authentication method?"],
  // Don't share options across different questions!
}
```

### 3. Limit to 3 Questions Maximum

The tool enforces a maximum of 3 questions per call to avoid overwhelming users:

✅ **Good:**
```typescript
{
  questions: [
    { text: "Question 1?", options: ["A", "B"] },
    { text: "Question 2?", options: ["C", "D"] },
    { text: "Question 3?", options: ["E", "F"] }
  ]
}
```

❌ **Bad:**
```typescript
{
  questions: [
    { text: "Q1?", options: ["A"] },
    { text: "Q2?", options: ["B"] },
    { text: "Q3?", options: ["C"] },
    { text: "Q4?", options: ["D"] } // This will fail validation!
  ]
}
```

### 4. Use Additional Info for Open-ended Feedback

The `additionalInfoLabel` field is perfect for gathering unstructured feedback:

```typescript
{
  questions: [
    { text: "Implementation approach?", options: ["Incremental", "Big Bang", "Hybrid"] }
  ],
  additionalInfoLabel: "Any concerns or special considerations?"
}
```

## Error Handling

### Validation Errors

The tool validates parameters and returns clear error messages:

```typescript
// Error: Too many questions
{
  questions: [/* 4+ questions */]
}
// → "Too many questions. Maximum 3 questions allowed per call."

// Error: Missing options
{
  questions: [{ text: "Question?", options: [] }]
}
// → "Question at index 0 must have at least 1 option."

// Error: Too many options
{
  questions: [{ text: "Question?", options: ["A", "B", "C", "D", "E"] }]
}
// → "Question at index 0 has too many options. Maximum 4 options allowed."
```

### Timeout Handling

If the user doesn't respond within the timeout period (default 5 minutes):

```
⏱️  Interaction timed out
```

The SDK receives a `USER_INTERACTION_FAILED` event with an error message.

## Integration Examples

### Agent Loop Integration

```toml
[[nodes]]
id = "clarification_node"
type = "AGENT_LOOP"
config = """
{
  "agent_loop_id": "default",
  "system_prompt": "You are a helpful assistant. When you need clarification from the user, use the ask_followup_question tool. Ask clear, specific questions with relevant options.",
  "tools": ["ask_followup_question", "read_file", "write_to_file"]
}
"""
```

### Workflow TOML Example

```toml
[workflow]
id = "interactive_workflow"
name = "Interactive Configuration Workflow"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "agent"
type = "AGENT_LOOP"
config = """
{
  "agent_loop_id": "default",
  "system_prompt": "Help the user configure their project. Use ask_followup_question to gather requirements.",
  "tools": ["ask_followup_question"]
}
"""

[[edges]]
from = "start"
to = "agent"
```

## Response Format

After user interaction, the tool returns a formatted response to the LLM:

```
User Responses:

Q1: Which database should we use?
A1: PostgreSQL (selected from options)

Q2: What's the deployment environment?
A2: Production (selected from options)

Q3: Implementation priority?
A3: Medium priority (custom input)

Additional Information:
Must support high availability

--- End of User Response ---
```

The LLM can then continue processing with this information.

## Troubleshooting

### Issue: Questions not displaying in CLI

**Solution:** Ensure the CLI app is running and the SDK is properly initialized:
```bash
cd apps/cli-app
pnpm build
node dist/index.js workflow execute <workflow-id>
```

### Issue: Timeout too short/long

**Solution:** The timeout is configurable in the SDK's tool handler. Default is 5 minutes (300000ms).

### Issue: Custom input not working

**Solution:** Make sure to select the last option (e.g., "4. [Custom input]") to enable custom text entry.

## Next Steps

- Try creating workflows that use `ask_followup_question`
- Experiment with different question structures
- Monitor user feedback to improve question clarity
- Consider implementing similar interactive tools for other use cases
