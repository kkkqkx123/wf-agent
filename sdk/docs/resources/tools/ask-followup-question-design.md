# Ask Follow-up Question Tool Design

## Overview

The `ask_followup_question` tool enables interactive communication between the AI agent and users. Unlike traditional tools that execute autonomously, this tool requires user input to proceed, making it essential for clarification, decision-making, and gathering missing information.

## Design Philosophy

### Core Principles

1. **Event-Driven Architecture**: Leverage the existing user interaction event system (`USER_INTERACTION_REQUESTED` / `USER_INTERACTION_RESPONDED`)
2. **Separation of Concerns**: SDK handles workflow logic; apps layer handles UI implementation
3. **Flexible Interaction Model**: Support multiple questions with structured options in a single tool call
4. **Graceful Degradation**: Work without UI (return formatted text) when interaction infrastructure is unavailable

## Data Flow Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         LLM Execution Loop                           │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
                    ┌──────────────────────────────┐
                    │  LLM calls ask_followup_     │
                    │  question with parameters    │
                    └──────────────────────────────┘
                                    │
                                    ▼
                    ┌──────────────────────────────┐
                    │  ToolCallExecutor detects    │
                    │  interactive tool            │
                    └──────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  Build interaction request from tool params:   │
          │  - questions[] with options[]                  │
          │  - additional_info placeholder                 │
          └─────────────────────────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  Emit USER_INTERACTION_REQUESTED event         │
          │  with structured payload                       │
          └─────────────────────────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  Apps layer (CLI/Web/VSCode) listens to event  │
          │  and renders UI                                │
          └─────────────────────────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  User selects options + inputs custom text     │
          └─────────────────────────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  Apps layer emits USER_INTERACTION_RESPONDED   │
          │  event with user selections                    │
          └─────────────────────────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  ToolCallExecutor receives response            │
          │  and formats as tool result                    │
          └─────────────────────────────────────────────────┘
                                    │
                                    ▼
          ┌─────────────────────────────────────────────────┐
          │  Add formatted result to conversation history  │
          │  for LLM to continue processing                │
          └─────────────────────────────────────────────────┘
```

## Tool Schema Design

### Parameter Structure

```typescript
interface AskFollowupQuestionParams {
  /**
   * Array of questions to present to the user (max 3)
   * Each question includes its own preset options for selection
   */
  questions: Array<{
    /** The question text */
    text: string;

    /**
     * Preset options for THIS specific question (1-4 items)
     * Options are bound to the question for clarity and flexibility
     */
    options: string[];
  }>;

  /**
   * Label for additional information field
   * Displayed after all questions for free-form user input
   */
  additionalInfoLabel?: string; // Default: "Additional comments or information"
}
```

### Example Usage by LLM

```json
{
  "questions": [
    {
      "text": "Which file path should I use for the configuration?",
      "options": ["./src/config.json", "./config/app.json", "./app.config.json"]
    },
    {
      "text": "What's your preferred approach for this task?",
      "options": ["Implement immediately", "Plan first, then implement", "Just provide guidance"]
    }
  ],
  "additionalInfoLabel": "Any specific requirements or constraints?"
}
```

**Note**: Each question has its own set of relevant options. This keeps context clear and prevents confusion.

## Response Format

### User Response Structure

```typescript
interface UserInteractionResponse {
  /** Interaction ID from the request */
  interactionId: string;

  /** User's answers to each question (array maintains order) */
  answers: Array<{
    /** Question text (included for clarity) */
    question: string;

    /** Selected option index (-1 if custom input) */
    selectedOptionIndex: number;

    /** Custom text input (if selectedOptionIndex === -1) */
    customInput?: string;

    /** Final answer value */
    answer: string;
  }>;

  /** Additional information provided by user (optional) */
  additionalInfo?: string;
}
```

### Formatted Tool Result

The tool result sent back to LLM includes both questions and answers for clarity:

```
User Responses:

Q1: Which file path should I use for the configuration?
A1: ./src/config.json (selected from options)

Q2: What's your preferred approach for this task?
A2: Just provide guidance (custom input)

Additional Information:
Please ensure backward compatibility with v1.x

--- End of User Response ---
```

## Implementation Strategy

### Phase 1: SDK Core Changes

#### 1.1 Update Tool Schema

**File**: `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/schema.ts`

```typescript
export const askFollowupQuestionSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    questions: {
      type: "array",
      description: "Array of 1-3 questions, each with its own preset options",
      items: {
        type: "object",
        properties: {
          text: {
            type: "string",
            description: "A clear, specific question to ask the user",
          },
          options: {
            type: "array",
            description: "Preset options for this specific question (1-4 options)",
            items: {
              type: "string",
              description: "An option that users can select as an answer",
            },
            minItems: 1,
            maxItems: 4,
          },
        },
        required: ["text", "options"],
      },
      minItems: 1,
      maxItems: 3, // Limit to 3 questions + 1 additional info = max 4 inputs
    },
    additionalInfoLabel: {
      type: "string",
      description: "Label for the additional information field",
    },
  },
  required: ["questions"],
};
```

#### 1.2 Update Tool Description

**File**: `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/description.ts`

```typescript
export const ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION: ToolDescriptionData = {
  id: "ask_followup_question",
  type: "STATELESS",
  category: "interaction",
  description: `Ask the user 1-3 questions to gather information needed to complete the task.

Use this tool when you need clarification, user preferences, or decisions before proceeding. Each question should include 1-4 relevant preset options. An additional information field is automatically included for free-form feedback.

Best Practices:
- Ask clear, specific questions (max 3)
- Provide 1-4 relevant preset options FOR EACH QUESTION
- Keep options directly related to the question
- Use additional info field for open-ended feedback`,
  parameters: [
    {
      name: "questions",
      type: "array",
      required: true,
      description: "Array of 1-3 questions, each with 1-4 preset options",
    },
    {
      name: "additionalInfoLabel",
      type: "string",
      required: false,
      description: "Custom label for the additional information field",
    },
  ],
  tips: [
    "Limit to 3 questions maximum to avoid overwhelming users",
    "Each question should have its own relevant options",
    "Keep questions concise and focused",
    "Options should be directly related to their question",
  ],
};
```

#### 1.3 Implement Interactive Handler

**File**: `sdk/resources/predefined/tools/stateless/interaction/ask-followup-question/handler.ts`

```typescript
import type { ToolOutput } from "@wf-agent/types";
import type { EventRegistry } from "../../../../core/registry/event-registry.js";
import { buildUserInteractionRequestedEvent } from "../../../../core/utils/event/builders/interaction-events.js";
import { generateId, now, diffTimestamp } from "@wf-agent/common-utils";
import { EventEmitter } from "events";

/**
 * Context for interactive tool execution
 */
interface InteractiveToolContext {
  eventManager?: EventRegistry;
  executionId?: string;
  nodeId?: string;
  timeout?: number;
}

/**
 * Create the ask_followup_question tool handler
 *
 * This handler supports both interactive mode (with event system)
 * and fallback mode (returns formatted text).
 */
export function createAskFollowupQuestionHandler() {
  return async (
    params: Record<string, unknown>,
    context?: InteractiveToolContext,
  ): Promise<ToolOutput> => {
    const startTime = now();

    try {
      const { questions, additionalInfoLabel } = params as {
        questions: Array<{
          text: string;
          options: string[];
        }>;
        additionalInfoLabel?: string;
      };

      // Validate parameters
      if (!questions || !Array.isArray(questions) || questions.length === 0) {
        return {
          success: false,
          content: "",
          error: "Missing or invalid 'questions' parameter. Must be a non-empty array.",
        };
      }

      if (questions.length > 3) {
        return {
          success: false,
          content: "",
          error: "Too many questions. Maximum 3 questions allowed per call.",
        };
      }

      // Validate each question
      for (let i = 0; i < questions.length; i++) {
        const q = questions[i];

        if (!q.text || typeof q.text !== "string" || q.text.trim().length === 0) {
          return {
            success: false,
            content: "",
            error: `Question at index ${i} must have a non-empty 'text' field.`,
          };
        }

        if (!q.options || !Array.isArray(q.options) || q.options.length === 0) {
          return {
            success: false,
            content: "",
            error: `Question at index ${i} must have at least 1 option.`,
          };
        }

        if (q.options.length > 4) {
          return {
            success: false,
            content: "",
            error: `Question at index ${i} has too many options. Maximum 4 options allowed.`,
          };
        }

        // Validate each option is a non-empty string
        for (let j = 0; j < q.options.length; j++) {
          if (
            !q.options[j] ||
            typeof q.options[j] !== "string" ||
            q.options[j].trim().length === 0
          ) {
            return {
              success: false,
              content: "",
              error: `Option ${j + 1} in question ${i + 1} must be a non-empty string.`,
            };
          }
        }
      }

      // Check if interactive mode is available
      if (!context?.eventManager || !context?.executionId) {
        // Fallback mode: return formatted text
        return createFallbackResponse(questions, additionalInfoLabel);
      }

      // Interactive mode: trigger user interaction event
      const interactionId = generateId();
      const timeout = context.timeout || 300000; // 5 minutes default

      // Build interaction request payload
      const interactionRequest = {
        interactionId,
        operationType: "ASK_FOLLOWUP_QUESTION" as const,
        questions: questions.map((q, idx) => ({
          index: idx,
          text: q.text,
          options: q.options.map((opt, optIdx) => ({
            index: optIdx,
            value: opt,
          })),
        })),
        additionalInfoLabel: additionalInfoLabel || "Additional comments or information",
        metadata: {
          executionId: context.executionId,
          nodeId: context.nodeId,
        },
      };

      // Emit USER_INTERACTION_REQUESTED event
      const requestedEvent = buildUserInteractionRequestedEvent({
        executionId: context.executionId,
        interactionId,
        operationType: "ASK_FOLLOWUP_QUESTION",
        prompt: JSON.stringify(interactionRequest),
        timeout,
        nodeId: context.nodeId,
      });

      await context.eventManager.emit(requestedEvent);

      // Wait for USER_INTERACTION_RESPONDED event
      const response = await waitForInteractionResponse(
        context.eventManager,
        interactionId,
        timeout,
      );

      if (!response) {
        return {
          success: false,
          content: "",
          error: "Timeout waiting for user response",
        };
      }

      // Parse user response
      const userAnswers = response.inputData as {
        answers: Array<{
          question: string;
          selectedOptionIndex: number;
          customInput?: string;
          answer: string;
        }>;
        additionalInfo?: string;
      };

      // Format tool result for LLM
      const formattedResult = formatUserResponse(questions, userAnswers, additionalInfoLabel);

      const executionTime = diffTimestamp(startTime, now());

      return {
        success: true,
        content: formattedResult,
        metadata: {
          executionTime,
          interactionId,
        },
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}

/**
 * Create fallback response when interactive mode is unavailable
 */
function createFallbackResponse(
  questions: Array<{ text: string; options: string[] }>,
  additionalInfoLabel?: string,
): ToolOutput {
  const lines: string[] = [];
  lines.push("Interactive mode unavailable. Please provide the following information:");
  lines.push("");

  questions.forEach((q, idx) => {
    lines.push(`Q${idx + 1}: ${q.text}`);
    q.options.forEach((opt, optIdx) => {
      lines.push(`  ${optIdx + 1}. ${opt}`);
    });
    lines.push(`  ${q.options.length + 1}. [Custom input]`);
    lines.push("");
  });

  lines.push(`${additionalInfoLabel || "Additional comments"}:`);
  lines.push("[Please provide your responses above]");

  return {
    success: true,
    content: lines.join("\n"),
    metadata: { fallback: true },
  };
}

/**
 * Wait for user interaction response
 */
async function waitForInteractionResponse(
  eventManager: EventRegistry,
  interactionId: string,
  timeout: number,
): Promise<{ inputData: unknown } | null> {
  return new Promise(resolve => {
    let timeoutId: NodeJS.Timeout | null = null;
    let resolved = false;

    const listener = (event: any) => {
      if (event.interactionId === interactionId && !resolved) {
        resolved = true;
        if (timeoutId) clearTimeout(timeoutId);
        eventManager.off("USER_INTERACTION_RESPONDED", listener);
        resolve(event);
      }
    };

    eventManager.on("USER_INTERACTION_RESPONDED", listener);

    timeoutId = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        eventManager.off("USER_INTERACTION_RESPONDED", listener);
        resolve(null);
      }
    }, timeout);
  });
}

/**
 * Format user response for LLM consumption
 */
function formatUserResponse(
  questions: Array<{ text: string; options: string[] }>,
  userAnswers: {
    answers: Array<{
      questionIndex: number;
      selectedOptionIndex: number;
      customInput?: string;
      answer: string;
    }>;
    additionalInfo?: string;
  },
  additionalInfoLabel?: string,
): string {
  const lines: string[] = [];
  lines.push("User Responses:");
  lines.push("");

  userAnswers.answers.forEach(answer => {
    const question = questions[answer.questionIndex];
    if (!question) return;

    lines.push(`Q${answer.questionIndex + 1}: ${question.text}`);

    if (answer.selectedOptionIndex >= 0) {
      const option = question.options[answer.selectedOptionIndex];
      lines.push(
        `A${answer.questionIndex + 1}: ${option || answer.answer} (selected from options)`,
      );
    } else {
      lines.push(
        `A${answer.questionIndex + 1}: ${answer.customInput || answer.answer} (custom input)`,
      );
    }
    lines.push("");
  });

  if (userAnswers.additionalInfo && userAnswers.additionalInfo.trim()) {
    lines.push(`${additionalInfoLabel || "Additional Information"}:`);
    lines.push(userAnswers.additionalInfo);
    lines.push("");
  }

  lines.push("--- End of User Response ---");

  return lines.join("\n");
}
```

#### 1.4 Update Tool Registry

**File**: `sdk/resources/predefined/tools/registry.ts`

Update the tool registration to pass context:

```typescript
// In createPredefinedTools function
tools.push({
  id: "ask_followup_question",
  type: "STATELESS",
  description: renderToolDescription(ASK_FOLLOWUP_QUESTION_TOOL_DESCRIPTION),
  parameters: askFollowupQuestionSchema,
  execute: createAskFollowupQuestionHandler(),
  // Mark as interactive tool
  metadata: {
    requiresUserInteraction: true,
    interactionType: "ASK_FOLLOWUP_QUESTION",
  },
});
```

### Phase 2: ToolCallExecutor Integration

#### 2.1 Detect Interactive Tools

**File**: `sdk/core/executors/tool-call-executor.ts`

Add logic to detect and handle interactive tools specially:

```typescript
// In executeSingleToolCall method, after getting toolConfig
const isInteractiveTool = toolConfig?.metadata?.requiresUserInteraction === true;

if (isInteractiveTool) {
  // Pass context to handler for interactive mode
  const interactiveContext = {
    eventManager: this.eventManager,
    executionId,
    nodeId,
    timeout: executionOptions.timeout,
  };

  const result = await this.toolService.execute(
    toolCall.name,
    JSON.parse(toolCall.arguments),
    executionOptions,
    executionId,
    interactiveContext, // Pass context
  );

  // ... rest of execution logic
}
```

#### 2.2 Update ToolRegistry Interface

**File**: `sdk/core/registry/tool-registry.ts`

Update the execute method signature to support context passing:

```typescript
async execute(
  toolId: ID,
  parameters: Record<string, unknown>,
  options?: ToolExecutionOptions,
  executionId?: string,
  context?: Record<string, unknown> // Add context parameter
): Promise<Result<ToolExecutionOutput, Error>> {
  // ... existing code

  // Pass context to handler if it accepts it
  if (tool.execute.length > 1) {
    result = await tool.execute(parameters, context);
  } else {
    result = await tool.execute(parameters);
  }

  // ... rest of logic
}
```

### Phase 3: Apps Layer Integration Guide

#### 3.1 Event Subscription Pattern

All apps (CLI, Web, VSCode) should follow this pattern:

```typescript
// In app initialization
const api = createAPIFactory(dependencies);

// Subscribe to interaction requests
api.userInteractions.onInteractionRequested(async event => {
  if (event.operationType === "ASK_FOLLOWUP_QUESTION") {
    const requestData = JSON.parse(event.prompt);

    // Render UI based on requestData
    const userResponse = await renderAskFollowupQuestionUI(requestData);

    // Emit response event
    await api.events.emit("USER_INTERACTION_RESPONDED", {
      interactionId: event.interactionId,
      inputData: userResponse,
      timestamp: Date.now(),
    });
  }
});
```

#### 3.2 UI Component Specification

Apps should implement a UI component with:

1. **Question Cards**: Each question displays with its own set of options
2. **Option Radio Buttons**: 1-4 preset options per question + custom input field
3. **Additional Info Textarea**: Multi-line input for free-form feedback
4. **Submit Button**: Validates all questions answered before submission

**Example UI Structure**:

```
┌─────────────────────────────────────────────────────┐
│  Ask Follow-up Questions                             │
├─────────────────────────────────────────────────────┤
│                                                      │
│  Q1: Which file path should I use?                  │
│  ○ ./src/config.json                                │
│  ○ ./config/app.json                                │
│  ○ ./app.config.json                                │
│  ○ Custom: [_______________________]                │
│                                                      │
├─────────────────────────────────────────────────────┤
│                                                      │
│  Q2: What's your preferred approach?                │
│  ○ Implement immediately                            │
│  ○ Plan first, then implement                       │
│  ○ Just provide guidance                            │
│  ○ Custom: [_______________________]                │
│                                                      │
├─────────────────────────────────────────────────────┤
│                                                      │
│  Additional comments or information:                │
│  ┌───────────────────────────────────────────┐     │
│  │                                           │     │
│  │                                           │     │
│  └───────────────────────────────────────────┘     │
│                                                      │
│  [Submit Response]                                   │
└─────────────────────────────────────────────────────┘
```

#### 3.3 Response Format for Apps

Apps must emit responses in this format:

```typescript
{
  interactionId: "xxx", // From the request event
  inputData: {
    answers: [
      {
        questionIndex: 0, // Index into the questions array
        selectedOptionIndex: 0, // 0-based index into question.options, -1 for custom input
        customInput: undefined, // Only present if selectedOptionIndex === -1
        answer: "./src/config.json" // Final answer value
      },
      {
        questionIndex: 1,
        selectedOptionIndex: -1,
        customInput: "Hybrid approach",
        answer: "Hybrid approach"
      }
    ],
    additionalInfo: "Please test thoroughly"
  }
}
```

**Note**: The SDK will map `questionIndex` back to the actual question text when formatting the response for the LLM.

## Testing Strategy

### Unit Tests

1. **Schema Validation**: Test parameter validation rules
2. **Fallback Mode**: Verify formatted text output without event system
3. **Interactive Mode**: Mock event emission and response handling
4. **Error Handling**: Timeout, invalid responses, malformed data

### Integration Tests

1. **End-to-End Flow**: Complete interaction from LLM call to response
2. **Multiple Questions**: Test with 1-5 questions
3. **Custom Inputs**: Verify custom input handling
4. **Concurrent Interactions**: Multiple simultaneous interactions

### Apps Layer Tests

Each app should test:

1. UI rendering correctness
2. Event subscription/unsubscription
3. Response formatting
4. Timeout handling
5. User cancellation

## Migration Path

### Backward Compatibility

The current implementation returns formatted text. To migrate:

1. **Phase 1**: Deploy updated schema and handler with fallback mode
2. **Phase 2**: Update ToolCallExecutor to pass context
3. **Phase 3**: Implement UI in each app layer
4. **Phase 4**: Enable interactive mode by default

### Deprecation Notice

The old single-question format will be deprecated. Provide migration guide:

**Old Format** (deprecated):

```json
{
  "question": "What is the path?",
  "follow_up": [{ "text": "./path1" }, { "text": "./path2" }]
}
```

**New Format**:

```json
{
  "questions": ["What is the path?"],
  "options": ["./path1", "./path2"]
}
```

## Future Enhancements

1. **Mode Switching**: Integrate with agent mode system (code/architect/etc.)
2. **Conditional Questions**: Show/hide questions based on previous answers
3. **Validation Rules**: Add regex patterns, required fields, etc.
4. **Multi-step Wizards**: Chain multiple interaction rounds
5. **Rich Media**: Support images, code snippets in questions
6. **Analytics**: Track interaction metrics (response time, option selection rates)

## References

- [Tool Approval Coordinator](../../core/coordinators/tool-approval-coordinator.ts)
- [User Interaction Resource API](../../api/workflow/resources/user-interaction/user-interaction-resource-api.ts)
- [Interaction Events](../../core/utils/event/builders/interaction-events.ts)
- [Roo-Code Implementation](../../../../ref/roo-code/src/tools/AskFollowupQuestionTool.ts)
