# Agent Trigger System

## 1. Overview

The agent trigger system provides a mechanism to execute agent loops automatically when specific conditions are met. Triggers follow a **match-action** pattern: when a trigger condition is matched, the corresponding action is executed.

## 2. Architecture

```
AgentTrigger (interface)
├── triggerId: ID
├── type: TriggerType
├── condition: TriggerCondition  // When to trigger
├── action: TriggerAction        // What to execute
└── config: TriggerConfig        // Behavior configuration

Trigger Types:
├── EVENT: Trigger on specific events
├── SCHEDULE: Trigger on schedule (cron)
├── CONDITION: Trigger on state condition
└── MESSAGE: Trigger on message pattern
```

### Trigger Execution Flow

```
executeAgentTriggers(entity, context):
  1. Get all triggers from entity config
  2. For each trigger:
     a. Evaluate trigger condition
     b. If condition matches:
        - Execute trigger action
        - Emit TRIGGER_MATCHED event
     c. If condition doesn't match:
        - Skip (no action)
  3. Return trigger execution results
```

## 3. Trigger Handler

The `executeAgentTriggers()` function in the trigger handlers module:

```
executeAgentTriggers(entity, iterationResult):
  1. Check entity config for triggers
  2. For each trigger:
     a. Evaluate condition based on trigger type
     b. If matched:
        - Execute the action (e.g., start agent loop, send message)
        - Track trigger state via TriggerStateManager
        - Emit events
  3. Return execution results
```

### Trigger Handler Module Structure

```
packages/sdk/agent/execution/handlers/trigger-handlers/
├── index.ts              → Module exports
└── trigger-handler.ts    → executeAgentTriggers() implementation
```

## 4. Trigger State Management

The `TriggerStateManager` (shared) tracks trigger runtime state:

```
TriggerStateManager
├── getTriggerState(triggerId) → TriggerState
├── setTriggerState(triggerId, state) → void
├── isTriggerMatched(triggerId) → boolean
├── recordTriggerMatch(triggerId) → void
├── createSnapshot() → TriggerStateSnapshot
└── restoreFromSnapshot(snapshot) → void
```

### Trigger State

```
TriggerState
├── lastMatched: timestamp?
├── matchCount: number
├── isActive: boolean
├── cooldownUntil: timestamp?
└── metadata: Record<string, unknown>
```

## 5. Trigger Configuration

### Static Trigger Definition

```typescript
interface AgentTriggerStatic {
  triggerId: ID;
  type: TriggerType;
  condition: TriggerCondition;
  action: {
    type: "RUN_AGENT" | "RUN_WORKFLOW" | "CALLBACK" | "EMIT_EVENT";
    config: Record<string, unknown>;
  };
  config?: {
    cooldown?: number;       // Min time between triggers (ms)
    maxMatches?: number;     // Max trigger executions
    enabled?: boolean;
  };
}
```

### Runtime Trigger

```typescript
interface AgentTrigger {
  triggerId: ID;
  type: TriggerType;
  condition: TriggerCondition;
  action: (context: TriggerContext) => Promise<void>;  // Callback
  config?: TriggerConfig;
}
```

### Trigger Builder

The `AgentTriggerBuilder` provides a fluent API for building trigger configurations:

```
AgentTriggerBuilder
├── triggerId(id) → this
├── type(type) → this
├── condition(condition) → this
├── action(action) → this
├── config(config) → this
└── build() → AgentTrigger
```

## 6. Trigger Integration with Agent Loop

### Trigger Execution Timing

Triggers are executed after each iteration (in `AFTER_ITERATION` phase):

```
AgentIterationCoordinator.executeIteration():
  ...
  8. Execute AFTER_ITERATION hooks
  9. executeAgentTriggers(entity, iterationResult)
     ├── Check each trigger condition
     ├── Execute matched trigger actions
     └── Update trigger state
```

### Trigger Types

| Type | Description | Condition Example |
|------|-------------|------------------|
| `EVENT` | Trigger on specific agent events | `AGENT_COMPLETED` |
| `SCHEDULE` | Trigger on time-based schedule | Cron expression |
| `CONDITION` | Trigger on state condition | `iteration > 5` |
| `MESSAGE` | Trigger on message pattern | Message contains keyword |

## 7. Trigger Events

The trigger system emits events for trigger lifecycle:

| Event | Description |
|-------|-------------|
| `TRIGGER_MATCHED` | Trigger condition was matched |
| `TRIGGER_ACTION_EXECUTED` | Trigger action was executed |
| `TRIGGER_ERROR` | Trigger execution failed |

## 8. Trigger Cooldown and Rate Limiting

The trigger system supports cooldown and rate limiting:

```
TriggerConfig:
├── cooldown: number        // Cooldown period in ms
├── maxMatches: number      // Max total matches
├── maxMatchesPerMinute: number  // Rate limit
└── enabled: boolean        // Enable/disable
```

When a trigger is in cooldown, the condition is evaluated but the action is skipped until the cooldown period expires.