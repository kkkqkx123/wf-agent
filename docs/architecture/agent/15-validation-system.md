# Agent Validation System

## 1. Overview

The validation system provides multi-layer validation for agent loop configurations, ensuring correctness before execution.

## 2. Validation Layers

### Configuration Validation

`validateAgentLoopConfig()` validates the agent loop configuration against a Zod schema:

```
validateAgentLoopConfig(config: AgentLoopConfigFile):
  1. Parse config against AgentLoopDefinitionSchema
  2. If schema validation fails:
     - Return ValidationError[] with field paths and messages
     - Each error is a ConfigurationValidationError
  3. If schema validation passes:
     - Return success with the validated config
```

### Schema Validation

The `AgentLoopDefinitionSchema` (from `@wf-agent/types`) validates:

```
AgentLoopDefinitionSchema:
├── id: string (optional)
├── name: string (optional)
├── version: string (optional)
├── description: string (optional)
├── profileId: string (optional)
├── systemPrompt: string (optional)
├── systemPromptTemplateId: string (optional)
├── maxIterations: number (optional, positive integer)
├── initialMessages: message[] (optional)
├── availableTools: AgentToolConfig (optional)
├── stream: boolean (optional)
├── hooks: AgentHookStatic[] (optional)
├── triggers: AgentTriggerStatic[] (optional)
├── dynamicContext: DynamicContextConfig (optional)
├── checkpoint: AgentCheckpointConfig (optional)
└── metadata: AgentLoopMetadata (optional)
```

### Validation Warnings

`getAgentLoopValidationWarnings()` provides additional non-critical warnings:

```
getAgentLoopValidationWarnings(config):
  1. Check maxIterations > 100 → warning about high iteration count
  2. Check hooks.length > 10 → warning about performance impact
  3. Return array of warning messages
```

### Tool Call Protocol Validation

`validateAgentToolCallProtocol()` validates tool call protocol compatibility:

```
validateAgentToolCallProtocol(config):
  1. Check tool format compatibility with LLM profile
     - Tool format (JSON, OpenAI, Anthropic, Gemini)
     - Provider compatibility
  2. Check tool schema validity
  3. Check tool names and parameters
  4. Return ProtocolValidationResult:
     - valid: boolean
     - warnings: string[]
     - errors: string[]
```

### ProtocolValidationResult

```typescript
interface ProtocolValidationResult {
  /** Whether the protocol configuration is valid */
  valid: boolean;
  /** Warning messages (non-critical issues) */
  warnings: string[];
  /** Error messages (critical issues) */
  errors: string[];
}
```

## 3. Validation Categories

### Configuration Validation

| Check | Description | Severity |
|-------|-------------|----------|
| Schema validation | Zod schema validation | Error |
| Max iterations | Positive integer check | Error |
| Tool format compatibility | LLM profile compatibility | Warning |
| Hook count | Performance impact | Warning |

### Runtime Validation

| Check | Description | Severity |
|-------|-------------|----------|
| Entity state | Valid state transitions | Error |
| Tool existence | Tool registered in registry | Error |
| LLM profile | Profile exists and is accessible | Error |
| Checkpoint config | Valid checkpoint configuration | Warning |

### Protocol Validation

| Check | Description | Severity |
|-------|-------------|----------|
| Tool format | Compatible with LLM provider | Error |
| Tool schema | Valid tool schema format | Error |
| Parameter types | Correct parameter types | Error |

## 4. Validation Flow

### Pre-Execution Validation

```
AgentLoopExecutor.execute():
  1. Validate agent loop configuration
  2. Validate tool format compatibility
  3. Return validation errors if any
```

### Runtime Validation

During execution, validation occurs at key points:

```
AgentExecutionCoordinator.execute():
  At each iteration:
    - Validate entity state transition
    - Validate tool call protocol
    - Validate LLM profile availability

ToolExecutionCoordinator.executeTools():
  For each tool call:
    - Validate tool exists in registry
    - Validate tool call parameters
    - Validate tool approval status
```

## 5. Error Handling for Validation

Validation errors are returned as structured error types:

```typescript
class ValidationError extends Error {
  type: "validation";
  errors: string[];
}

class ConfigurationValidationError extends ValidationError {
  configType: string;
  field?: string;
}
```

Validation warnings are non-blocking — execution can proceed with warnings, but errors block execution.

## 6. Validation Integration

The validation system integrates with the command layer:

```
RunAgentLoopCommand.validate():
  1. Run validateAgentLoopConfig(config)
  2. If validation errors → return validationFailure(errors)
  3. Run getAgentLoopValidationWarnings(config)
  4. Return validationSuccess with warnings
```

And with the executor layer:

```
AgentLoopExecutor.execute():
  1. validateAgentToolCallProtocol(config)
  2. If protocol errors → reject with error
  3. If protocol warnings → log warnings, proceed
  4. Continue execution
```