# Timeout Management Architecture Design

## Overview

This document describes the unified timeout management architecture for the wf-agent SDK. It addresses the fragmented timeout implementations across different modules (LLM, Tools, Workflow Join operations, etc.) and proposes a centralized, consistent approach.

## Problem Statement

### Current Issues

1. **Fragmented Implementations**
   - LLM module: HTTP-level timeout via profile configuration
   - Tool executor: No timeout, relies solely on AbortSignal
   - Workflow Join: Second-based timeout with polling fallback
   - Pause manager: Dedicated setTimeout-based implementation
   - Task registry: Passive timeout tracking only

2. **Inconsistent Patterns**
   ```typescript
   // Different units and approaches
   timeout: 30000              // LLM (milliseconds in config)
   timeoutMs = timeout * 1000  // Join (seconds to milliseconds)
   setTimeout(...)             // PauseTimeoutManager
   // No timeout                // Tool Executor
   ```

3. **Missing Integration with Interruption System**
   - Timeout operations rarely bind to parent execution's InterruptionState
   - Parent execution stop doesn't automatically cancel child timeouts
   - Resource leaks when timeouts aren't properly cleaned up

4. **Poor Observability**
   - Timeout events scattered across modules
   - No unified metrics collection
   - Difficult to trace timeout root causes

5. **Inefficient Waiting Mechanisms**
   - Polling-based waits (100ms intervals) waste CPU
   - No event-driven alternatives when EventRegistry unavailable
   - Lack of adaptive timeout strategies

## Design Goals

1. **Unified Interface**: Single API for all timeout operations
2. **Interruption Integration**: Automatic binding to InterruptionState
3. **Resource Safety**: Guaranteed cleanup and no memory leaks
4. **Observability**: Comprehensive metrics and logging
5. **Flexibility**: Support multiple timeout strategies (absolute, idle, hierarchical)
6. **Backward Compatibility**: Gradual migration path for existing code

## Architecture

### Component Structure

```
sdk/
├── core/
│   ├── state-managers/
│   │   └── timeout-manager.ts          # Core timeout state management
│   ├── registry/
│   │   └── timeout-registry.ts         # Global timeout registration & tracking
│   ├── utils/
│   │   └── timeout/
│   │       ├── index.ts                # Public API exports
│   │       ├── timeout-utils.ts        # Helper functions
│   │       └── timeout-strategies.ts   # Strategy implementations
│   └── types/
│       └── timeout.ts                  # Type definitions (also in packages/types)
└── docs/
    └── architecture/
        └── timeout-management.md       # This document

packages/types/src/
└── timeout/
    ├── index.ts                        # Type exports
    ├── timeout-types.ts                # Core type definitions
    └── timeout-config.ts               # Configuration types
```

### Core Components

#### 1. TimeoutManager (State Manager)

**Location**: `sdk/core/state-managers/timeout-manager.ts`

**Responsibilities**:
- Manage individual timeout lifecycle
- Track timeout state (active/expired/cancelled)
- Execute timeout callbacks
- Integrate with InterruptionState
- Support checkpoint serialization

**Key Features**:
```typescript
class TimeoutManager implements StateManager<TimeoutSnapshot> {
  // Per-timeout tracking
  private timeouts: Map<string, TimeoutEntry> = new Map();
  
  // Configuration
  private config: Required<TimeoutManagerConfig>;
  
  // Methods
  register(options: TimeoutRegistration): TimeoutHandle;
  cancel(handle: TimeoutHandle): void;
  refresh(handle: TimeoutHandle): void;
  getRemainingTime(handle: TimeoutHandle): number;
  
  // StateManager interface
  size(): number;
  isEmpty(): boolean;
  clear(): void;
  serialize(): TimeoutSnapshot;
  restore(snapshot: TimeoutSnapshot): void;
}
```

**Integration with InterruptionState**:
```typescript
// Automatically cancel timeout when parent execution is interrupted
if (options.interruptionState) {
  const unsubscribe = options.interruptionState.onResumed(() => {
    // Refresh timeout on resume
    this.refresh(handle);
  });
  
  if (options.interruptionState.shouldStop()) {
    this.cancel(handle);
    return;
  }
}
```

#### 2. TimeoutRegistry (Global Registry)

**Location**: `sdk/core/registry/timeout-registry.ts`

**Responsibilities**:
- Centralized timeout registration across all modules
- Batch operations by tag or execution ID
- Cross-execution metrics aggregation
- Lifecycle management (cleanup on execution end)

**Key Features**:
```typescript
class TimeoutRegistry {
  // Per-execution TimeoutManager instances
  private managers: Map<string, TimeoutManager> = new Map();
  
  // Global metrics
  private metricsCollector: TimeoutMetricsCollector;
  
  // Methods
  getManager(executionId: string): TimeoutManager;
  cancelByExecutionId(executionId: string): void;
  cancelByTag(tag: string): void;
  getStats(): TimeoutStats;
  cleanup(executionId: string): void;
}
```

**Usage Pattern**:
```typescript
// Get or create manager for execution
const manager = timeoutRegistry.getManager(executionId);

// Register timeout
const handle = manager.register({
  id: 'llm-call-123',
  duration: 30000,
  onTimeout: () => cancelLLMCall(),
  interruptionState: executionEntity.getInterruptionState(),
  tag: 'llm',
  metadata: { nodeId: 'node-1' }
});

// Cleanup when execution ends
timeoutRegistry.cleanup(executionId);
```

#### 3. Timeout Strategies

**Location**: `sdk/core/utils/timeout/timeout-strategies.ts`

**Supported Strategies**:

1. **Absolute Timeout**: Fixed duration from registration
   ```typescript
   register({ duration: 30000, ... })
   ```

2. **Idle Timeout**: Triggers after period of inactivity
   ```typescript
   registerIdleTimeout({
     idleDuration: 60000,
     activityDetector: () => isStillProcessing()
   })
   ```

3. **Hierarchical Timeout**: Parent-child timeout relationships
   ```typescript
   const parentHandle = register({
     duration: 120000,
     children: [child1Handle, child2Handle]
   });
   ```

4. **Warning + Final Timeout**: Two-stage timeout with warning
   ```typescript
   register({
     duration: 300000,
     warningThreshold: 60000,
     onWarning: emitWarning,
     onTimeout: cancelOperation
   })
   ```

#### 4. Timeout Utilities

**Location**: `sdk/core/utils/timeout/timeout-utils.ts`

**Helper Functions**:
```typescript
// Combine timeout with AbortSignal
function combineTimeoutWithSignal(
  duration: number,
  signal?: AbortSignal
): { signal: AbortSignal; clearTimeout: () => void };

// Create timeout promise
function createTimeoutPromise<T>(
  promise: Promise<T>,
  duration: number,
  message?: string
): Promise<T>;

// Adaptive timeout calculation
function calculateAdaptiveTimeout(
  baseTimeout: number,
  retryCount: number,
  maxTimeout: number
): number;
```

## Type Definitions

### Core Types

**Location**: `packages/types/src/timeout/timeout-types.ts`

```typescript
/**
 * Timeout registration options
 */
export interface TimeoutRegistration {
  /** Unique identifier for this timeout */
  id: string;
  
  /** Timeout duration in milliseconds */
  duration: number;
  
  /** Callback invoked when timeout expires */
  onTimeout: () => void | Promise<void>;
  
  /** Optional: Warning threshold (ms before timeout) */
  warningThreshold?: number;
  
  /** Optional: Warning callback */
  onWarning?: () => void | Promise<void>;
  
  /** Optional: Bind to interruption state for automatic cancellation */
  interruptionState?: InterruptionState;
  
  /** Optional: Tag for batch operations */
  tag?: string;
  
  /** Optional: Execution context ID */
  executionId?: string;
  
  /** Optional: Metadata for observability */
  metadata?: Record<string, unknown>;
}

/**
 * Timeout handle for managing registered timeouts
 */
export interface TimeoutHandle {
  /** Timeout ID */
  id: string;
  
  /** Check if timeout is still active */
  isActive(): boolean;
  
  /** Get remaining time in milliseconds */
  getRemainingTime(): number;
  
  /** Cancel this timeout */
  cancel(): void;
}

/**
 * Timeout entry internal state
 */
export interface TimeoutEntry {
  id: string;
  startTime: number;
  duration: number;
  timerId?: NodeJS.Timeout;
  warningTimerId?: NodeJS.Timeout;
  status: 'active' | 'expired' | 'cancelled';
  warningEmitted: boolean;
  interruptionUnsubscribe?: () => void;
  onTimeout: () => void | Promise<void>;
  onWarning?: () => void | Promise<void>;
  metadata?: Record<string, unknown>;
}

/**
 * Timeout snapshot for checkpoint support
 */
export interface TimeoutSnapshot {
  version: number;
  timestamp: number;
  timeouts: Array<{
    id: string;
    startTime: number;
    duration: number;
    status: 'active' | 'expired' | 'cancelled';
    warningEmitted: boolean;
    metadata?: Record<string, unknown>;
  }>;
}

/**
 * Timeout statistics
 */
export interface TimeoutStats {
  activeTimeouts: number;
  totalRegistered: number;
  timedOutCount: number;
  cancelledCount: number;
  averageDuration: number;
  byTag: Record<string, number>;
  byModule: Record<string, number>;
}
```

### Configuration Types

**Location**: `packages/types/src/timeout/timeout-config.ts`

```typescript
/**
 * TimeoutManager configuration
 */
export interface TimeoutManagerConfig {
  /** Default timeout duration (ms) */
  defaultTimeout?: number;
  
  /** Maximum allowed timeout duration (ms) */
  maxTimeout?: number;
  
  /** Enable warning emissions */
  enableWarnings?: boolean;
  
  /** Default warning threshold (ms before timeout) */
  defaultWarningThreshold?: number;
  
  /** Enable metrics collection */
  enableMetrics?: boolean;
}

/**
 * TimeoutRegistry configuration
 */
export interface TimeoutRegistryConfig {
  /** Default manager configuration */
  defaultManagerConfig?: TimeoutManagerConfig;
  
  /** Auto-cleanup on execution end */
  autoCleanup?: boolean;
  
  /** Metrics collection interval (ms) */
  metricsInterval?: number;
}
```

## Integration Examples

### 1. LLM Module Integration

**Current**:
```typescript
// BaseLLMClient uses HTTP client timeout
this.httpClient = new HttpClient({
  timeout: profile.timeout || 30000,
});
```

**Proposed**:
```typescript
// Use TimeoutManager for entire LLM call lifecycle
const timeoutManager = timeoutRegistry.getManager(executionId);

const handle = timeoutManager.register({
  id: `llm-${requestId}`,
  duration: profile.timeout || 30000,
  onTimeout: () => {
    messageStream.abort();
    logger.warn('LLM call timed out', { requestId });
  },
  interruptionState: executionEntity.getInterruptionState(),
  tag: 'llm',
  metadata: { profileId, nodeId }
});

try {
  const result = await llmClient.generate(request);
  handle.cancel(); // Success, cancel timeout
  return result;
} catch (error) {
  handle.cancel(); // Error, cancel timeout
  throw error;
}
```

### 2. Tool Executor Integration

**Current**:
```typescript
// No timeout, only relies on AbortSignal
async executeToolCalls(toolCalls, options?: { abortSignal?: AbortSignal }) {
  // Tools can hang indefinitely
}
```

**Proposed**:
```typescript
async executeToolCalls(
  toolCalls,
  options?: { 
    abortSignal?: AbortSignal;
    toolTimeout?: number; // Per-tool timeout
  }
) {
  const timeoutManager = timeoutRegistry.getManager(executionId);
  
  const executionPromises = toolCalls.map(async (toolCall) => {
    // Register per-tool timeout
    const handle = timeoutManager.register({
      id: `tool-${toolCall.id}`,
      duration: options?.toolTimeout || DEFAULT_TOOL_TIMEOUT,
      onTimeout: () => {
        logger.warn('Tool execution timed out', { 
          toolCallId: toolCall.id,
          toolName: toolCall.name 
        });
        // Mark tool as timed out
      },
      interruptionState: executionEntity.getInterruptionState(),
      tag: 'tool',
      metadata: { toolName: toolCall.name }
    });
    
    try {
      const result = await executeSingleTool(toolCall, options);
      handle.cancel();
      return result;
    } catch (error) {
      handle.cancel();
      throw error;
    }
  });
  
  return Promise.allSettled(executionPromises);
}
```

### 3. Workflow Join Operation

**Current**:
```typescript
// Uses polling fallback when EventRegistry unavailable
async function waitForCompletionByPolling(...) {
  while (pendingExecutions.size > 0) {
    // Check status
    await new Promise(resolve => setTimeout(resolve, 100)); // Inefficient
  }
}
```

**Proposed**:
```typescript
async function join(
  childExecutionIds: string[],
  joinStrategy: JoinStrategy,
  timeout: number = 0
): Promise<JoinResult> {
  const timeoutManager = timeoutRegistry.getManager(parentExecutionId);
  
  // Register join timeout
  let timeoutHandle: TimeoutHandle | undefined;
  if (timeout > 0) {
    timeoutHandle = timeoutManager.register({
      id: `join-${parentExecutionId}`,
      duration: timeout * 1000, // Convert seconds to ms
      onTimeout: () => {
        throw new ExecutionError(
          `Join operation timed out after ${timeout}s`,
          undefined,
          parentExecutionId
        );
      },
      interruptionState: parentEntity.getInterruptionState(),
      tag: 'join',
      metadata: { childCount: childExecutionIds.length, strategy: joinStrategy }
    });
  }
  
  try {
    // Wait using event-driven approach (no polling)
    const result = await waitForCompletion(...);
    
    // Success, cancel timeout
    timeoutHandle?.cancel();
    
    return result;
  } catch (error) {
    // Error or timeout, cancel timeout
    timeoutHandle?.cancel();
    throw error;
  }
}
```

### 4. Pause Timeout Manager Refactor

**Current**:
```typescript
class PauseTimeoutManager {
  private entries: Map<string, PauseTimeoutEntry> = new Map();
  
  startMonitoring(executionId: string): void {
    entry.timerId = setTimeout(() => this.handleTimeout(...), maxDuration);
  }
}
```

**Proposed**:
```typescript
class PauseTimeoutManager {
  constructor(private timeoutRegistry: TimeoutRegistry) {}
  
  startMonitoring(executionId: string): void {
    const manager = this.timeoutRegistry.getManager(executionId);
    
    manager.register({
      id: `pause-${executionId}`,
      duration: this.config.maxPauseDuration,
      warningThreshold: this.config.warningThreshold,
      onWarning: () => this.emitWarning(executionId),
      onTimeout: () => this.handleTimeout(executionId),
      tag: 'pause',
      metadata: { executionId }
    });
  }
  
  stopMonitoring(executionId: string): void {
    // TimeoutRegistry handles cleanup
    this.timeoutRegistry.cleanup(executionId);
  }
}
```

## Migration Plan

### Phase 1: Foundation (Week 1-2)

1. **Create Type Definitions**
   - Add timeout types to `packages/types/src/timeout/`
   - Export from main types index

2. **Implement Core Components**
   - `TimeoutManager` state manager
   - `TimeoutRegistry` global registry
   - Basic timeout strategies

3. **Add Utilities**
   - Timeout helper functions
   - Signal combination utilities
   - Adaptive timeout calculations

4. **Write Tests**
   - Unit tests for TimeoutManager
   - Integration tests for TimeoutRegistry
   - Edge case testing (interruption, cleanup)

### Phase 2: Module Integration (Week 3-4)

1. **Pause Timeout Manager** (Easiest migration)
   - Refactor to use TimeoutRegistry
   - Maintain backward compatibility
   - Add deprecation warnings for old API

2. **Workflow Join Operations**
   - Replace polling with event-driven waits
   - Integrate TimeoutManager
   - Remove fallback to polling mode

3. **LLM Module**
   - Add timeout support to MessageStream
   - Integrate with TimeoutManager
   - Support per-request timeout overrides

4. **Tool Executor**
   - Add optional tool-level timeout
   - Integrate with TimeoutManager
   - Maintain backward compatibility (no timeout by default)

### Phase 3: Advanced Features (Week 5-6)

1. **Hierarchical Timeouts**
   - Parent-child timeout relationships
   - Automatic propagation

2. **Idle Timeout Strategy**
   - Activity detection
   - Reset on activity

3. **Enhanced Observability**
   - Metrics integration
   - Timeout event emission
   - Dashboard support

4. **Documentation**
   - API documentation
   - Migration guide
   - Best practices

### Phase 4: Optimization & Cleanup (Week 7-8)

1. **Performance Optimization**
   - Timer pooling
   - Memory optimization
   - Lazy initialization

2. **Deprecate Old APIs**
   - Mark old timeout patterns as deprecated
   - Provide migration scripts
   - Update examples

3. **Final Testing**
   - Load testing
   - Stress testing
   - Production validation

## Benefits

### Immediate Benefits

1. **Consistency**: Unified timeout API across all modules
2. **Reliability**: Guaranteed cleanup and resource safety
3. **Observability**: Centralized metrics and logging
4. **Maintainability**: Single source of truth for timeout logic

### Long-term Benefits

1. **Extensibility**: Easy to add new timeout strategies
2. **Debuggability**: Better timeout tracing and diagnostics
3. **Performance**: Elimination of inefficient polling
4. **Safety**: Automatic integration with interruption system

## Risks & Mitigations

### Risk 1: Breaking Changes

**Mitigation**:
- Maintain backward compatibility during migration
- Provide deprecation warnings
- Offer migration scripts and guides

### Risk 2: Performance Overhead

**Mitigation**:
- Lazy initialization of TimeoutManager
- Efficient timer management
- Benchmarking before production deployment

### Risk 3: Complexity

**Mitigation**:
- Clear documentation and examples
- Simple default behavior
- Progressive enhancement for advanced features

## Success Metrics

1. **Code Quality**
   - Reduce timeout-related code by 50%
   - Eliminate all polling-based waits
   - 100% test coverage for timeout components

2. **Reliability**
   - Zero timeout-related memory leaks
   - 100% cleanup success rate
   - <1% timeout false positives

3. **Performance**
   - <5ms overhead for timeout registration
   - <1ms overhead for timeout cancellation
   - No measurable impact on normal operations

4. **Adoption**
   - 100% of new code uses TimeoutManager
   - 80% of existing code migrated within 3 months
   - Positive developer feedback on usability

## Conclusion

The unified timeout management architecture provides a robust, consistent, and observable solution for timeout handling across the wf-agent SDK. By centralizing timeout logic and integrating with the existing interruption system, we achieve better reliability, maintainability, and developer experience.

The phased migration approach ensures minimal disruption while delivering immediate benefits. The architecture is designed for extensibility, allowing future enhancements without breaking changes.

**Next Steps**:
1. Review and approve this design document
2. Begin Phase 1 implementation (type definitions and core components)
3. Schedule weekly progress reviews
4. Plan integration timeline with module owners
