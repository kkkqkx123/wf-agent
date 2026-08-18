# ADD_MESSAGE and UPDATE_VARIABLES Implementation Analysis

## Overview

The `ADD_MESSAGE` and `UPDATE_VARIABLES` operation types provide mechanisms for user interaction nodes to modify workflow state based on user input. Both operations support template-based content generation using the `{{input}}` placeholder.

## Operation Types Comparison

| Feature | ADD_MESSAGE | UPDATE_VARIABLES |
|---------|-------------|------------------|
| Purpose | Add user messages to LLM conversation | Update workflow variables |
| Target | Conversation history | Variable scopes |
| Use Case | Continue dialogue with user feedback | Store user decisions/inputs |
| Requires | Message config with template | Variable configs with expressions |
| Side Effect | Modifies conversation state | Modifies variable state |

## Core Implementation

Located in: `sdk/workflow/execution/handlers/node-handlers/user-interaction-handler.ts`

### 1. Template Processing

Both operations use a shared placeholder replacement mechanism:

```typescript
/**
 * Replace the {{input}} placeholder with user input
 */
function replaceInputPlaceholder(template: string, inputData: unknown): string {
  if (typeof template !== "string") {
    return String(template);
  }
  
  // Replace all occurrences of {{input}}
  return template.replace(/\{\{input\}\}/g, String(inputData));
}
```

**Features:**
- Supports multiple `{{input}}` placeholders in one template
- Converts non-string input to string representation
- Global replacement (all occurrences)

**Example:**
```typescript
// Template: "User said: {{input}}. Please respond to: {{input}}"
// Input: "Hello"
// Result: "User said: Hello. Please respond to: Hello"
```

### 2. Expression Evaluation

For `UPDATE_VARIABLES`, expressions can be evaluated:

```typescript
/**
 * Calculate the value of the expression (simple implementation)
 */
function evaluateExpression(expression: string, inputData: unknown): unknown {
  // If the expression is just {{input}}, return the input directly
  if (expression === "{{input}}") {
    return inputData;
  }

  // If the expression contains {{input}}, return the result after replacement
  if (expression.includes("{{input}}")) {
    return replaceInputPlaceholder(expression, inputData);
  }

  // Otherwise, simply return the expression (which may be a constant value)
  return expression;
}
```

**Supported Patterns:**
1. Direct input: `"{{input}}"` → returns raw input value
2. Template expression: `"prefix_{{input}}_suffix"` → returns string
3. Constant value: `"static_value"` → returns constant

**Limitations:**
- No arithmetic operations
- No complex JavaScript evaluation
- String-based processing only

---

## ADD_MESSAGE Implementation

### Configuration Structure

From `packages/types/src/node/configs/interaction-configs.ts`:

```typescript
interface UserInteractionNodeConfig {
  operationType: "ADD_MESSAGE";
  
  message: {
    role: "user";  // Fixed to 'user' role
    contentTemplate: string;  // Template with {{input}} placeholder
  };
  
  prompt: string;        // Displayed to user for input
  timeout?: number;      // Default: 30000ms
  metadata?: Record<string, unknown>;
}
```

### Processing Logic

```typescript
/**
 * Handle message addition
 */
function processMessageAdd(
  config: UserInteractionNodeConfig,
  inputData: unknown,
  conversationManager?: UserInteractionHandlerContext["conversationManager"],
): { role: string; content: string } {
  // 1. Validate configuration
  if (!config.message) {
    throw new ExecutionError("No message defined for ADD_MESSAGE operation");
  }

  // 2. Replace {{input}} placeholder in content template
  const content = replaceInputPlaceholder(config.message.contentTemplate, inputData);

  // 3. Add message to conversation manager
  if (conversationManager) {
    conversationManager.addMessage({
      role: config.message.role,
      content,
    });
  }

  // 4. Return processed message
  return {
    role: config.message.role,
    content,
  };
}
```

### Execution Flow

```
┌─────────────────────────────────────┐
│ 1. User Interaction Node Triggered  │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 2. Display Prompt to User           │
│    "Please provide your feedback:"  │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 3. Wait for User Input              │
│    Input: "This looks good"         │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 4. Process Message Addition         │
│    Template: "User feedback: {{input}}"
│    Result: "User feedback: This looks good"
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 5. Add to Conversation Manager      │
│    conversationManager.addMessage({ │
│      role: "user",                  │
│      content: "User feedback: ..."  │
│    })                               │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 6. Return Result                    │
│    { role: "user", content: ... }   │
└─────────────────────────────────────┘
```

### Usage Example

**Workflow Configuration:**

```toml
[[nodes]]
id = "user_feedback"
type = "USER_INTERACTION"

[nodes.config]
operationType = "ADD_MESSAGE"
prompt = "Please review the generated code and provide feedback:"
timeout = 60000

[nodes.config.message]
role = "user"
contentTemplate = "Code review feedback: {{input}}"
```

**Execution:**
1. Workflow pauses at `user_feedback` node
2. Displays prompt: "Please review the generated code and provide feedback:"
3. User enters: "The error handling needs improvement"
4. System adds message to conversation:
   ```json
   {
     "role": "user",
     "content": "Code review feedback: The error handling needs improvement"
   }
   ```
5. Next LLM node receives updated conversation with user feedback

### Key Characteristics

✅ **Strengths:**
- Seamless integration with LLM conversation flow
- Template-based customization
- Automatic conversation state management
- Clear separation between prompt and stored message

⚠️ **Considerations:**
- Role is fixed to "user" (cannot add assistant messages)
- Requires conversation manager to be available
- Content is always converted to string
- No validation of message content

---

## UPDATE_VARIABLES Implementation

### Configuration Structure

```typescript
interface UserInteractionNodeConfig {
  operationType: "UPDATE_VARIABLES";
  
  variables: Array<{
    variableName: string;       // Name of variable to update
    expression: string;         // Expression with {{input}} placeholder
    scope: VariableScope;       // 'global' | 'thread' | 'subgraph' | 'loop'
  }>;
  
  prompt: string;               // Displayed to user
  timeout?: number;             // Default: 30000ms
  metadata?: Record<string, unknown>;
}
```

### Processing Logic

```typescript
/**
 * Handling variable updates
 */
async function processVariableUpdate(
  config: UserInteractionNodeConfig,
  inputData: unknown,
  workflowExecution: WorkflowExecution,
): Promise<Record<string, unknown>> {
  // 1. Validate configuration
  if (!config.variables || config.variables.length === 0) {
    throw new ExecutionError(
      "No variables defined for UPDATE_VARIABLES operation",
      workflowExecution.id
    );
  }

  const results: Record<string, unknown> = {};

  // 2. Process each variable update
  for (const variableConfig of config.variables) {
    // Replace {{input}} placeholder in expression
    const expression = replaceInputPlaceholder(variableConfig.expression, inputData);

    // Evaluate expression to get final value
    const value = evaluateExpression(expression, inputData);

    // Update variable in workflow execution
    if (!workflowExecution.variableScopes.workflowExecution) {
      workflowExecution.variableScopes.workflowExecution = {};
    }
    workflowExecution.variableScopes.workflowExecution[variableConfig.variableName] = value;

    // Track result
    results[variableConfig.variableName] = value;
  }

  // 3. Return all updated variables
  return results;
}
```

### Execution Flow

```
┌─────────────────────────────────────┐
│ 1. User Interaction Node Triggered  │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 2. Display Prompt to User           │
│    "Select deployment environment:" │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 3. Wait for User Input              │
│    Input: "production"              │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 4. Process Variable Updates         │
│    For each variable config:        │
│    - Replace {{input}} in expression│
│    - Evaluate expression            │
│    - Update workflow variable       │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 5. Update Workflow State            │
│    workflowExecution                │
│      .variableScopes                │
│      .workflowExecution             │
│      .deployEnv = "production"      │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ 6. Return Results                   │
│    { deployEnv: "production" }      │
└─────────────────────────────────────┘
```

### Usage Examples

#### Example 1: Single Variable Update

**Configuration:**
```toml
[[nodes]]
id = "select_environment"
type = "USER_INTERACTION"

[nodes.config]
operationType = "UPDATE_VARIABLES"
prompt = "Select deployment environment (dev/staging/prod):"

[[nodes.config.variables]]
variableName = "deployEnv"
expression = "{{input}}"
scope = "workflowExecution"
```

**Execution:**
- User input: `"production"`
- Variable updated: `workflowExecution.deployEnv = "production"`

#### Example 2: Multiple Variables from Single Input

**Configuration:**
```toml
[[nodes]]
id = "configure_deployment"
type = "USER_INTERACTION"

[nodes.config]
operationType = "UPDATE_VARIABLES"
prompt = "Enter deployment region (e.g., us-east-1):"

[[nodes.config.variables]]
variableName = "region"
expression = "{{input}}"
scope = "workflowExecution"

[[nodes.config.variables]]
variableName = "regionLabel"
expression = "Deploying to: {{input}}"
scope = "workflowExecution"

[[nodes.config.variables]]
variableName = "isProduction"
expression = "{{input}}" == "us-east-1" ? true : false
scope = "workflowExecution"
```

**Note:** The third example shows a limitation - complex expressions are not evaluated. It would store the literal string `"{{input}}" == "us-east-1" ? true : false"`.

#### Example 3: Constant Value Assignment

**Configuration:**
```toml
[[nodes]]
id = "set_status"
type = "USER_INTERACTION"

[nodes.config]
operationType = "UPDATE_VARIABLES"
prompt = "Press Enter to confirm continuation:"

[[nodes.config.variables]]
variableName = "userConfirmed"
expression = "true"
scope = "workflowExecution"
```

**Execution:**
- User input: (any value, even empty)
- Variable updated: `workflowExecution.userConfirmed = "true"` (string)

### Variable Scopes

Current implementation uses `workflowExecution` scope by default. The type system supports:

```typescript
type VariableScope = 
  | "global"          // Global across all workflows
  | "thread"          // Thread-level (conversation)
  | "subgraph"        // Sub-workflow level
  | "loop"            // Loop iteration level
  | "workflowExecution"; // Current execution
```

**Note:** The current handler implementation simplifies this and primarily uses `workflowExecution` scope. Full scope support would require integration with the variable management system.

### Key Characteristics

✅ **Strengths:**
- Batch update multiple variables from single input
- Template-based value transformation
- Persistent storage in workflow state
- Accessible to subsequent nodes

⚠️ **Considerations:**
- Limited expression evaluation (no JavaScript eval)
- All values stored as strings unless input is structured
- Scope support is simplified in current implementation
- No validation of variable names or values
- Overwrites existing variables without warning

---

## Integration with User Interaction Handler

### Main Handler Function

```typescript
export async function userInteractionHandler(
  workflowExecution: WorkflowExecution,
  node: Node,
  context: UserInteractionHandlerContext,
): Promise<UserInteractionExecutionResult> {
  const config = node.config as UserInteractionNodeConfig;
  const interactionId = generateId();
  const startTime = now();

  // 1. Create interaction request
  const request = createInteractionRequest(config, interactionId);

  // 2. Create interaction context
  const interactionContext = createInteractionContext(
    workflowExecution,
    node,
    request.timeout,
    context.conversationManager,
  );

  // 3. Get user input (with timeout/cancel support)
  const inputData = await getUserInput(
    request,
    interactionContext as UserInteractionContext,
    context.userInteractionHandler,
  );

  // 4. Process user input based on operation type
  const results = await processUserInput(
    config,
    inputData,
    workflowExecution,
    context.conversationManager,
  );

  const executionTime = diffTimestamp(startTime, now());

  return {
    interactionId,
    operationType: config.operationType,
    results,
    executionTime,
  };
}
```

### Operation Type Routing

```typescript
async function processUserInput(
  config: UserInteractionNodeConfig,
  inputData: unknown,
  workflowExecution: WorkflowExecution,
  conversationManager?: UserInteractionHandlerContext["conversationManager"],
): Promise<unknown> {
  switch (config.operationType) {
    case "UPDATE_VARIABLES":
      return await processVariableUpdate(config, inputData, workflowExecution);

    case "ADD_MESSAGE":
      return processMessageAdd(config, inputData, conversationManager);

    default:
      throw new ExecutionError(`Unknown operation type: ${config.operationType}`);
  }
}
```

---

## Context Management

### Interaction Context Creation

```typescript
function createInteractionContext(
  workflowExecution: WorkflowExecution,
  node: Node,
  timeout: number,
  _conversationManager?: unknown,
): UserInteractionContext {
  const cancelToken = {
    cancelled: false,
    cancel: () => {
      cancelToken.cancelled = true;
    },
  };

  return {
    executionId: workflowExecution.id,
    workflowId: workflowExecution.workflowId,
    nodeId: node.id,
    
    // Variable access methods
    getVariable: (variableName: string, _scope?: VariableScope) => {
      return workflowExecution.variableScopes.workflowExecution?.[variableName];
    },
    
    setVariable: async (variableName: string, value: unknown, _scope?: VariableScope) => {
      if (!workflowExecution.variableScopes.workflowExecution) {
        workflowExecution.variableScopes.workflowExecution = {};
      }
      workflowExecution.variableScopes.workflowExecution[variableName] = value;
    },
    
    getVariables: (_scope?: VariableScope) => {
      return workflowExecution.variableScopes.workflowExecution || {};
    },
    
    timeout,
    cancelToken,
  };
}
```

**Key Features:**
- Provides variable CRUD operations to application handlers
- Manages cancellation token for timeout/interrupt support
- Simplified scope implementation (primarily workflowExecution)

---

## Error Handling

### Validation Errors

```typescript
// ADD_MESSAGE without message config
if (!config.message) {
  throw new ExecutionError("No message defined for ADD_MESSAGE operation");
}

// UPDATE_VARIABLES without variables
if (!config.variables || config.variables.length === 0) {
  throw new ExecutionError(
    "No variables defined for UPDATE_VARIABLES operation",
    workflowExecution.id
  );
}

// Unknown operation type
throw new ExecutionError(`Unknown operation type: ${config.operationType}`);
```

### Timeout Handling

Implemented via `Promise.race()` in `getUserInput()`:

```typescript
const timeoutPromise = new Promise((_, reject) => {
  setTimeout(() => {
    reject(new Error(`User interaction timeout after ${request.timeout}ms`));
  }, request.timeout);
});

return await Promise.race([
  handler.handle(request, context),  // User input
  timeoutPromise,                     // Timeout
  cancelPromise                       // Cancellation
]);
```

---

## Best Practices

### For ADD_MESSAGE

1. **Use Descriptive Prompts**: Guide users on expected input format
2. **Template Design**: Include context in content templates
3. **Timeout Settings**: Set appropriate timeouts for complex inputs
4. **Conversation Flow**: Consider how added messages affect LLM behavior

**Example:**
```toml
prompt = "Please explain what changes you'd like to make to the code:"
contentTemplate = "User requested changes: {{input}}. Please implement these modifications."
```

### For UPDATE_VARIABLES

1. **Clear Variable Names**: Use descriptive, consistent naming
2. **Input Validation**: Handle validation in application layer
3. **Single Responsibility**: One node per decision point
4. **Documentation**: Document expected input formats

**Example:**
```toml
prompt = "Enter the maximum number of retries (1-10):"

[[nodes.config.variables]]
variableName = "maxRetries"
expression = "{{input}}"
```

### General Recommendations

1. **Error Handling**: Implement robust error handling in custom handlers
2. **User Experience**: Provide clear instructions and examples
3. **Testing**: Test edge cases (empty input, special characters, long text)
4. **Logging**: Log interaction events for debugging and analytics

---

## Limitations and Future Enhancements

### Current Limitations

1. **Expression Evaluation**: No support for complex expressions or calculations
2. **Type Safety**: All values treated as strings unless explicitly parsed
3. **Scope Support**: Simplified variable scope implementation
4. **Validation**: No built-in input validation
5. **Multi-line Input**: Depends on handler implementation

### Potential Enhancements

1. **Advanced Expressions**: Support JavaScript expression evaluation (safely sandboxed)
2. **Type Coercion**: Automatic type conversion based on variable schema
3. **Input Validation**: Built-in validation rules (regex, range, enum)
4. **Rich Input Types**: Support for structured data (JSON, forms)
5. **Conditional Logic**: Dynamic variable updates based on conditions
6. **Full Scope Support**: Complete implementation of all variable scopes
7. **History Tracking**: Track variable change history
8. **Undo Support**: Ability to revert variable changes

---

## Summary

The `ADD_MESSAGE` and `UPDATE_VARIABLES` operations provide essential mechanisms for integrating user input into workflow execution:

- **ADD_MESSAGE**: Seamlessly incorporates user feedback into LLM conversations
- **UPDATE_VARIABLES**: Persists user decisions as workflow state for downstream nodes

Both operations share a common architecture:
1. Display prompt to user
2. Collect input via handler
3. Process through template/expression engine
4. Update workflow state (conversation or variables)
5. Return results for tracking

The implementation emphasizes simplicity and flexibility, allowing application layers to customize the user experience while maintaining consistent workflow state management.
