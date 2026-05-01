# EventResourceAPI Design Issues Analysis

**Date**: 2026-05-01  
**File Analyzed**: `sdk/api/shared/resources/events/event-resource-api.ts`  
**Status**: Design Review Required

## Executive Summary

The `EventResourceAPI` has several critical design issues that compromise type safety, maintainability, and architectural clarity. The main problems include mixing event types from different modules, lacking proper type constraints, bypassing official type definitions, and using unsafe type casting patterns.

---

## Critical Issues Identified

### 1. Mixed Event Types from Different Modules ❌

**Location**: Lines 25-94 (`ALL_EVENT_TYPES` array)

**Problem**: The API mixes events from multiple domains without clear separation:

```typescript
const ALL_EVENT_TYPES: EventType[] = [
  // Workflow Execution events
  "WORKFLOW_EXECUTION_STARTED",
  "WORKFLOW_EXECUTION_COMPLETED",
  // ... more workflow events
  
  // Node events
  "NODE_STARTED",
  "NODE_COMPLETED",
  // ... more node events
  
  // Agent-specific events
  "AGENT_CUSTOM_EVENT",
  
  // System events
  "TOKEN_LIMIT_EXCEEDED",
  "ERROR",
  "VARIABLE_CHANGED",
  
  // Tool events
  "TOOL_CALL_STARTED",
  "TOOL_ADDED",
  
  // Interaction events
  "USER_INTERACTION_REQUESTED",
  "HUMAN_RELAY_REQUESTED",
  
  // Skill events
  "SKILL_LOAD_STARTED",
  // ... more skill events
];
```

**Impact**:
- Violates single responsibility principle
- Unclear module ownership boundaries
- Makes it difficult to extend or maintain specific event categories
- Tightly couples unrelated modules through shared event handling

**Root Cause**: Events are treated as a flat list rather than organized by domain/module.

---

### 2. Lack of Type Constraints ❌

**Location**: Lines 137, 207, 219, 272, etc.

**Problem**: The API uses `BaseEvent` throughout, losing all discriminated union type information:

```typescript
// ❌ Current implementation - loses type information
private eventHistory: BaseEvent[] = [];

protected async getResource(id: string): Promise<BaseEvent | null> {
  return this.eventHistory.find(...) || null;
}

async getEvents(filter?: EventFilter): Promise<BaseEvent[]> {
  let events = this.eventHistory;
  // ... filtering logic
  return events;
}
```

**Impact**:
- Cannot access event-specific properties without runtime type checks
- No compile-time safety for event-specific fields (e.g., `checkpointId`, `agentLoopId`)
- Forces consumers to use type assertions or manual type guards
- Loses benefits of TypeScript's discriminated unions

**Example of Lost Type Safety**:
```typescript
// ❌ Consumer must manually check type
const events = await api.getEvents();
events.forEach(event => {
  if (event.type === 'CHECKPOINT_CREATED') {
    // TypeScript doesn't know checkpointId exists here
    const checkpointId = (event as any).checkpointId; // Unsafe!
  }
});

// ✅ With proper typing, TypeScript would narrow automatically
const events: Event[] = await api.getEvents();
events.forEach(event => {
  if (event.type === 'CHECKPOINT_CREATED') {
    // TypeScript knows this is CheckpointCreatedEvent
    const checkpointId = event.checkpointId; // Type-safe!
  }
});
```

---

### 3. API Layer Bypassing Type Definitions ❌

**Location**: Lines 238-240, 362-364

**Problem**: Uses unsafe type casting instead of proper type narrowing:

```typescript
// ❌ Unsafe casting pattern
if (
  filter.nodeId &&
  "nodeId" in event &&
  (event as { nodeId?: string }).nodeId !== filter.nodeId
) {
  return false;
}

// ❌ Another unsafe cast in search
if ("nodeId" in event) {
  searchableFields.push((event as { nodeId?: string }).nodeId);
}
```

**Impact**:
- Bypasses TypeScript's type system
- No compile-time validation that properties exist
- Duplicates logic that should be derived from type definitions
- Risk of accessing non-existent properties at runtime

**Better Approach**: Use type guard functions defined in the types package:

```typescript
// ✅ Type guard function (should be in @wf-agent/types)
function hasNodeId(event: Event): event is Event & { nodeId: string } {
  return 'nodeId' in event && typeof event.nodeId === 'string';
}

// ✅ Safe usage with type narrowing
if (filter.nodeId && hasNodeId(event) && event.nodeId !== filter.nodeId) {
  return false;
}
```

---

### 4. Hardcoded Event Type List Risks Drift ❌

**Location**: Lines 25-94

**Problem**: The `ALL_EVENT_TYPES` array is manually maintained and may drift from the official `EventType` definition in `@wf-agent/types`.

**Current State**:
- `packages/types/src/events/base.ts` defines `EventType` union type
- `packages/types/src/events/index.ts` exports `Event` union type with all event interfaces
- `event-resource-api.ts` hardcodes the same list as strings

**Risks**:
- New event types added to `@wf-agent/types` might not be added to `ALL_EVENT_TYPES`
- Typos in hardcoded strings won't be caught at compile time
- No automated way to verify completeness
- Maintenance burden increases with each new event type

**Verification Gap**:
```typescript
// ❌ No compile-time check that ALL_EVENT_TYPES matches EventType
const ALL_EVENT_TYPES: EventType[] = [
  "WORKFLOW_EXECUTION_STARTED",  // Could have typo: "WORKFLOW_EXECUTION_STARTD"
  // ...
];

// ✅ Better: Derive from type or use exhaustive check
function assertAllEventTypesCovered(types: readonly EventType[]): void {
  // Compile-time exhaustive check
  const _check: EventType[] = types as EventType[];
}
```

---

### 5. Inconsistent Event ID Generation ❌

**Location**: Line 210

**Problem**: Creates composite IDs instead of using proper unique identifiers:

```typescript
// ❌ Composite key that may not be unique
`${event.type}-${event.executionId}-${event.timestamp}` === id
```

**Issues**:
- Not guaranteed to be unique (multiple events can have same type + executionId + timestamp)
- Implementation detail leaked into API contract
- Fragile: depends on timestamp precision
- Should use proper event IDs if they exist in the type definition

**Check Current Type Definition**:
```typescript
// packages/types/src/events/base.ts
export interface BaseEvent {
  type: EventType;
  timestamp: Timestamp;
  workflowId?: ID;
  executionId?: ID;
  metadata?: Metadata;
  // ❌ No 'id' field!
}
```

**Recommendation**: Add unique `id` field to `BaseEvent`:
```typescript
export interface BaseEvent {
  id: ID;  // ✅ Unique identifier
  type: EventType;
  timestamp: Timestamp;
  workflowId?: ID;
  executionId?: ID;
  metadata?: Metadata;
}
```

---

## Architectural Concerns

### 6. Missing Module Separation ⚠️

**Problem**: A single `EventResourceAPI` handles all events regardless of their source module.

**Current Architecture**:
```
sdk/api/shared/resources/events/event-resource-api.ts
├── Workflow events (from sdk/workflow)
├── Agent events (from sdk/agent)
├── Tool events (from sdk/core/tools)
├── System events (from sdk/core)
└── Skill events (from apps/cli-app/skills)
```

**Issues**:
- Shared API becomes a bottleneck for module-specific features
- Difficult to add module-specific query methods
- Unclear which module owns event lifecycle
- Violates modular architecture principles

**Recommended Architecture**:
```
sdk/api/shared/resources/events/
├── base-event-api.ts          # Common functionality
└── event-types.ts             # Type utilities

sdk/api/workflow/resources/events/
└── workflow-event-api.ts      # Workflow-specific queries

sdk/agent/resources/events/
└── agent-event-api.ts         # Agent-specific queries

sdk/api/tool/resources/events/
└── tool-event-api.ts          # Tool-specific queries
```

---

## Impact Assessment

| Issue | Severity | Impact Area | Effort to Fix |
|-------|----------|-------------|---------------|
| Mixed event types | High | Maintainability, Clarity | Medium |
| Lack of type constraints | Critical | Type Safety, DX | Low |
| Unsafe type casting | High | Runtime Safety | Low |
| Hardcoded event list | Medium | Maintenance | Low |
| Composite event IDs | Medium | Data Integrity | Medium |
| Missing module separation | Medium | Architecture | High |

---

## Recommended Improvements

### Priority 1: Type Safety Fixes (Immediate)

#### 1.1 Use Full Event Union Type

```typescript
import type { Event, EventType } from "@wf-agent/types";

export class EventResourceAPI extends ReadonlyResourceAPI<Event, string, EventFilter> {
  private eventHistory: Event[] = [];  // ✅ Use Event union type
  
  protected async getResource(id: string): Promise<Event | null> {
    return this.eventHistory.find(event => event.id === id) || null;
  }
  
  async getEvents(filter?: EventFilter): Promise<Event[]> {
    // Returns properly typed Event[]
  }
}
```

#### 1.2 Add Type Guard Functions to @wf-agent/types

Create `packages/types/src/events/type-guards.ts`:

```typescript
import type { Event } from "./index.js";

/**
 * Type guard for events with nodeId property
 */
export function hasNodeId(event: Event): event is Event & { nodeId: string } {
  return 'nodeId' in event && typeof event.nodeId === 'string';
}

/**
 * Type guard for events with checkpointId property
 */
export function hasCheckpointId(event: Event): event is Event & { checkpointId: string } {
  return 'checkpointId' in event && typeof event.checkpointId === 'string';
}

/**
 * Type guard for events with agentLoopId property
 */
export function hasAgentLoopId(event: Event): event is Event & { agentLoopId: string } {
  return 'agentLoopId' in event && typeof event.agentLoopId === 'string';
}

/**
 * Type guard for events with error property
 */
export function hasError(event: Event): event is Event & { error: unknown } {
  return 'error' in event;
}

// Export all type guards
export const eventTypeGuards = {
  hasNodeId,
  hasCheckpointId,
  hasAgentLoopId,
  hasError,
};
```

#### 1.3 Update Filter Logic to Use Type Guards

```typescript
protected override applyFilter(events: Event[], filter: EventFilter): Event[] {
  return events.filter(event => {
    if (filter.eventType && event.type !== filter.eventType) {
      return false;
    }
    
    if (filter.executionId && event.executionId !== filter.executionId) {
      return false;
    }
    
    if (filter.workflowId && event.workflowId !== filter.workflowId) {
      return false;
    }
    
    // ✅ Use type guard for safe property access
    if (filter.nodeId) {
      if (!hasNodeId(event) || event.nodeId !== filter.nodeId) {
        return false;
      }
    }
    
    if (filter.timestampRange?.start && event.timestamp < filter.timestampRange.start) {
      return false;
    }
    
    if (filter.timestampRange?.end && event.timestamp > filter.timestampRange.end) {
      return false;
    }
    
    return true;
  });
}
```

---

### Priority 2: Event ID Standardization (Short-term)

#### 2.1 Add ID Field to BaseEvent

Update `packages/types/src/events/base.ts`:

```typescript
export interface BaseEvent {
  /** Unique event identifier */
  id: ID;
  /** Event Type */
  type: EventType;
  /** Timestamp */
  timestamp: Timestamp;
  /** Workflow ID (optional) */
  workflowId?: ID;
  /** Execution ID (optional) */
  executionId?: ID;
  /** Event metadata */
  metadata?: Metadata;
}
```

#### 2.2 Update Event Creation to Generate IDs

Ensure all event creation code generates unique IDs:

```typescript
import { generateId } from "@wf-agent/common-utils";

const event: WorkflowExecutionStartedEvent = {
  id: generateId(),  // ✅ Unique ID
  type: "WORKFLOW_EXECUTION_STARTED",
  timestamp: now(),
  workflowId: workflow.id,
  executionId: execution.id,
};
```

#### 2.3 Simplify Resource Retrieval

```typescript
protected async getResource(id: string): Promise<Event | null> {
  // ✅ Simple ID lookup
  return this.eventHistory.find(event => event.id === id) || null;
}
```

---

### Priority 3: Event Type Management (Medium-term)

#### 3.1 Export Event Type List from @wf-agent/types

Add to `packages/types/src/events/index.ts`:

```typescript
/**
 * Complete list of all event types
 * Kept in sync with EventType union type
 */
export const EVENT_TYPE_LIST: readonly EventType[] = [
  "WORKFLOW_EXECUTION_STARTED",
  "WORKFLOW_EXECUTION_COMPLETED",
  // ... all event types
] as const;

/**
 * Verify that EVENT_TYPE_LIST includes all EventType values
 * This will cause a compile error if types drift
 */
type _VerifyExhaustive = typeof EVENT_TYPE_LIST[number] extends EventType ? true : false;
type _VerifyComplete = EventType extends typeof EVENT_TYPE_LIST[number] ? true : false;
```

#### 3.2 Use Exported List in EventResourceAPI

```typescript
import { EVENT_TYPE_LIST } from "@wf-agent/types";

// ✅ Use centralized list
for (const eventType of EVENT_TYPE_LIST) {
  const unsubscribe = eventManager.on(eventType, (event: Event) => {
    this.addEventToHistory(event);
  });
  listeners.push(unsubscribe);
}
```

---

### Priority 4: Module Separation (Long-term)

#### 4.1 Create Base Event API

` sdk/api/shared/resources/events/base-event-api.ts`:

```typescript
import { ReadonlyResourceAPI } from "../generic-resource-api.js";
import type { Event, EventType } from "@wf-agent/types";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";

export interface BaseEventFilter {
  eventType?: EventType;
  executionId?: string;
  workflowId?: string;
  timestampRange?: { start?: number; end?: number };
}

/**
 * Base event API with common functionality
 */
export abstract class BaseEventAPI extends ReadonlyResourceAPI<Event, string, BaseEventFilter> {
  protected eventHistory: Event[] = [];
  protected dependencies: APIDependencyManager;
  protected maxHistorySize: number;
  
  constructor(dependencies: APIDependencyManager, maxHistorySize: number = 1000) {
    super();
    this.dependencies = dependencies;
    this.maxHistorySize = maxHistorySize;
  }
  
  protected addEventToHistory(event: Event): void {
    this.eventHistory.push(event);
    if (this.eventHistory.length > this.maxHistorySize) {
      this.eventHistory = this.eventHistory.slice(-this.maxHistorySize);
    }
  }
  
  public dispose(): void {
    this.eventHistory = [];
  }
}
```

#### 4.2 Create Workflow-Specific Event API

`sdk/api/workflow/resources/events/workflow-event-api.ts`:

```typescript
import { BaseEventAPI } from "../../../shared/resources/events/base-event-api.js";
import type { 
  WorkflowExecutionStartedEvent,
  WorkflowExecutionCompletedEvent,
  // ... other workflow events
} from "@wf-agent/types";
import { hasNodeId } from "@wf-agent/types";

export interface WorkflowEventFilter {
  nodeId?: string;
  // Workflow-specific filters
}

export class WorkflowEventAPI extends BaseEventAPI {
  /**
   * Get workflow execution timeline
   */
  async getExecutionTimeline(executionId: string): Promise<Event[]> {
    return this.eventHistory
      .filter(event => event.executionId === executionId)
      .sort((a, b) => a.timestamp - b.timestamp);
  }
  
  /**
   * Get node execution events
   */
  async getNodeEvents(nodeId: string): Promise<Event[]> {
    return this.eventHistory.filter(event => 
      hasNodeId(event) && event.nodeId === nodeId
    );
  }
}
```

#### 4.3 Create Agent-Specific Event API

`sdk/agent/resources/events/agent-event-api.ts`:

```typescript
import { BaseEventAPI } from "../../../api/shared/resources/events/base-event-api.js";
import type { AgentCustomEvent } from "@wf-agent/types";
import { hasAgentLoopId } from "@wf-agent/types";

export interface AgentEventFilter {
  agentLoopId?: string;
  eventName?: string;
}

export class AgentEventAPI extends BaseEventAPI {
  /**
   * Get agent loop events
   */
  async getAgentLoopEvents(agentLoopId: string): Promise<AgentCustomEvent[]> {
    return this.eventHistory.filter((event): event is AgentCustomEvent =>
      hasAgentLoopId(event) && event.agentLoopId === agentLoopId
    ) as AgentCustomEvent[];
  }
  
  /**
   * Get events by custom event name
   */
  async getEventsByName(eventName: string): Promise<AgentCustomEvent[]> {
    return this.eventHistory.filter((event): event is AgentCustomEvent =>
      event.type === 'AGENT_CUSTOM_EVENT' && event.eventName === eventName
    ) as AgentCustomEvent[];
  }
}
```

---

## Migration Plan

### Phase 1: Type Safety (Week 1)
1. Add type guard functions to `@wf-agent/types`
2. Update `EventResourceAPI` to use `Event[]` instead of `BaseEvent[]`
3. Replace unsafe casts with type guards
4. Add comprehensive tests for type narrowing

### Phase 2: Event IDs (Week 2)
1. Add `id` field to `BaseEvent` interface
2. Update all event creation code to generate IDs
3. Migrate event storage to use ID-based lookups
4. Add migration script for existing data (if needed)

### Phase 3: Event Type Management (Week 3)
1. Export `EVENT_TYPE_LIST` from `@wf-agent/types`
2. Update `EventResourceAPI` to use exported list
3. Add compile-time verification
4. Document event type addition process

### Phase 4: Module Separation (Week 4-5)
1. Create `BaseEventAPI` foundation
2. Extract workflow-specific API
3. Extract agent-specific API
4. Update consumers to use appropriate APIs
5. Deprecate monolithic `EventResourceAPI`

---

## Testing Recommendations

### Unit Tests for Type Guards

```typescript
describe('Event Type Guards', () => {
  test('hasNodeId correctly narrows node events', () => {
    const nodeEvent: NodeStartedEvent = {
      id: 'test-id',
      type: 'NODE_STARTED',
      timestamp: Date.now(),
      nodeId: 'node-1',
      nodeType: 'agent-loop',
    };
    
    expect(hasNodeId(nodeEvent)).toBe(true);
    if (hasNodeId(nodeEvent)) {
      expect(nodeEvent.nodeId).toBe('node-1'); // Type-safe access
    }
  });
  
  test('hasNodeId returns false for non-node events', () => {
    const workflowEvent: WorkflowExecutionStartedEvent = {
      id: 'test-id',
      type: 'WORKFLOW_EXECUTION_STARTED',
      timestamp: Date.now(),
      workflowId: 'wf-1',
    };
    
    expect(hasNodeId(workflowEvent)).toBe(false);
  });
});
```

### Integration Tests for Event Filtering

```typescript
describe('EventResourceAPI Filtering', () => {
  test('filters events by nodeId using type guards', async () => {
    const api = new EventResourceAPI(dependencies);
    
    // Add mixed events
    await api.dispatch(createNodeEvent('node-1'));
    await api.dispatch(createNodeEvent('node-2'));
    await api.dispatch(createWorkflowEvent());
    
    const filtered = await api.getEvents({ nodeId: 'node-1' });
    
    expect(filtered).toHaveLength(1);
    expect(filtered[0].type).toBe('NODE_STARTED');
    if (hasNodeId(filtered[0])) {
      expect(filtered[0].nodeId).toBe('node-1');
    }
  });
});
```

---

## Conclusion

The current `EventResourceAPI` design has significant issues that impact type safety, maintainability, and architectural clarity. The most critical problems are:

1. **Loss of type information** by using `BaseEvent` instead of `Event` union type
2. **Unsafe type casting** that bypasses TypeScript's type system
3. **Mixed responsibilities** across workflow, agent, and system modules

**Immediate Actions Required**:
- Switch to using `Event[]` type throughout the API
- Add and use type guard functions for safe property access
- Replace unsafe casts with proper type narrowing

**Long-term Improvements**:
- Add unique `id` field to `BaseEvent`
- Centralize event type management in `@wf-agent/types`
- Consider module-specific event APIs for better separation

These changes will significantly improve type safety, reduce runtime errors, and make the codebase more maintainable.

---

## References

- **Type Definitions**: `packages/types/src/events/`
- **EventResourceAPI**: `sdk/api/shared/resources/events/event-resource-api.ts`
- **Related Documentation**: 
  - `docs/sdk/tool/event-system-design.md` (if exists)
  - `docs/architecture/agent-loop-architecture.md`
