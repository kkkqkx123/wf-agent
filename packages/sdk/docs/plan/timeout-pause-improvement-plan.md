# Timeout & Pause Mechanism Improvement Plan

## Background

Based on the analysis of workflow and agent timeout/pause mechanisms, the following issues were identified:

1. **LLM timeout not integrated with TimeoutManager** — relies on HTTP client timeout only
2. **Tool execution has no dedicated timeout** — no per-tool timeout protection
3. **Agent loop has no wall-clock timeout** — only `maxIterations` limits iterations
4. **Agent loop pause has no timeout monitor** — can hang indefinitely
5. **TimeoutConfig values not actually enforced** — defined but unused
6. **`withExecutionTimeout` uses polling** — 1s interval is inefficient

## Completed Cleanup

| File | Action | Reason |
|------|--------|--------|
| `shared/registry/timeout-registry.ts` | Deleted | Already a stub; TimeoutManager lives on ExecutionEntity |
| `shared/types/timeout.ts` — TimeoutRegistryConfig, ResolvedTimeoutRegistryConfig, DEFAULT_TIMEOUT_REGISTRY_CONFIG | Removed | Types no longer needed after registry deletion |
| `docs/architecture/timeout-management.md` | Deleted | Outdated design doc referencing TimeoutRegistry |
| `docs/architecture/timeout-observability-usage.md` | Deleted | Entirely about TimeoutRegistry usage |
| `docs/refactoring/timeout-improvements-summary.md` | Deleted | References TimeoutRegistry modifications |
| `docs/refactoring/timeout-quick-reference.md` | Deleted | Has TimeoutRegistry section |
| `docs/refactoring/timeout-unification-refactoring-plan.md` | Deleted | Outdated plan referencing TimeoutRegistry |
| `shared/registry/REDUNDANCY_ANALYSIS.md` | Updated | Marked timeout-registry section as resolved |

## Improvement Plan

### Phase 1: Tool Execution Timeout (High Priority)

**Problem**: `ToolExecutionCoordinator` executes tools with no per-tool timeout. A hanging tool blocks the entire execution.

**Proposal**:
- Add optional `toolTimeout` config to `ToolExecutionCoordinatorDependencies`
- In `executeSingleApprovedTool`, register a `TimeoutManager` timeout before executing each tool
- On timeout, cancel the tool via `AbortController` and return a timeout error result
- Default: no timeout (backward compatible), configurable via workflow/agent config

**Key files**:
- `agent/execution/coordinators/tool-execution-coordinator.ts`
- `shared/state-managers/timeout-manager.ts` (already exists)

### Phase 2: Agent Loop Wall-Clock Timeout (High Priority)

**Problem**: Agent loop has no maximum execution time limit.

**Proposal**:
- Add `maxExecutionTime` to `AgentLoopRuntimeConfig`
- In `AgentLoopCoordinator.executeRootAgent`, register a TimeoutManager timeout before starting the loop
- On timeout, call `entity.interrupt("STOP")` to stop the agent loop
- The timeout should be registered on the execution entity's timeoutManager

**Key files**:
- `agent/execution/coordinators/agent-loop-coordinator.ts`
- `agent/execution/executors/agent-loop-executor.ts`

### Phase 3: Agent Loop Pause Timeout Monitor (Medium Priority)

**Problem**: When an agent loop is paused, there is no auto-cancel mechanism.

**Proposal**:
- Add a pause timeout monitor similar to `PauseTimeoutManager` for workflow
- In `AgentLoopCoordinator.pause()`, start monitoring with a configurable max pause duration
- When the timeout expires, cancel the agent loop
- Use the execution entity's `timeoutManager` to register the timeout

**Key files**:
- `agent/execution/coordinators/agent-loop-coordinator.ts`
- `agent/entities/agent-loop-entity.ts` (needs `timeoutManager` access)

### Phase 4: LLM Timeout Migration to TimeoutManager (Medium Priority)

**Problem**: LLM calls use HTTP client timeout (profile.timeout), not TimeoutManager. This means they don't benefit from InterruptionState binding or pause-aware timeout tracking.

**Proposal**:
- In the shared `LLMExecutionCoordinator.registerLLM`, register a TimeoutManager timeout alongside the HTTP call
- The timeout should be bound to the execution's InterruptionState
- Use `combineTimeoutWithSignal` to merge the timeout signal with the execution's abort signal
- Keep the HTTP client timeout as a fallback (shorter value)

**Key files**:
- `shared/coordinators/llm-execution-coordinator.ts`
- `workflow/execution/coordinators/llm-execution-coordinator.ts`

### Phase 5: Enforce TimeoutConfig Values (Low Priority)

**Problem**: `TimeoutConfig` defines many timeout values (workflowExecutionCompletion, nodeCompletion, etc.) but they are not enforced in the execution flow.

**Proposal**:
- Audit which TimeoutConfig values are meaningful
- Add timeout checks at the corresponding execution boundaries
- For values that are not useful, remove them from the config to reduce confusion

**Key files**:
- `api/shared/config/processors/timeout.ts`
- `workflow/execution/coordinators/workflow-execution-coordinator.ts`

### Phase 6: Eliminate Polling in withExecutionTimeout (Low Priority)

**Problem**: The "dynamic timeout mode" in `withExecutionTimeout` uses 1s polling to check `getEffectiveElapsed()`.

**Proposal**:
- Replace the polling mechanism with `TimeoutManager`'s pause/resume callbacks
- The `getEffectiveElapsed()` callback pattern can be replaced by registering the timeout on the TimeoutManager which already handles pause/resume tracking

**Key files**:
- `api/shared/utils/timeout-execution.ts`

## Current Architecture Diagram

```
ExecutionEntity
  └── timeoutManager: TimeoutManager    ← Centralized timeout management
        ├── register()                    ← Register timeout with callbacks
        ├── cancel()                      ← Cancel a timeout
        ├── refresh()                     ← Reset a timeout
        ├── pause() / resume()            ← Pause/resume all timeouts (pause-aware)
        ├── bindToInterruptionState()     ← Auto-cancel on stop, refresh on resume
        ├── serialize() / restore()       ← Checkpoint support
        └── emitEvent()                   ← Observability events

InterruptionState
  ├── requestPause() / resume() / requestStop()
  ├── getAbortSignal()
  ├── registerChild()                     ← Cascade propagation
  └── onInterrupted() / onResumed()       ← Callback registration

PauseTimeoutManager (workflow only)
  └── Uses workflowExecutionEntity.timeoutManager to monitor paused workflows
```

## Future State After Improvements

```
ExecutionEntity
  └── timeoutManager: TimeoutManager
        ├── register() ← LLM call timeout ✅
        ├── register() ← Tool execution timeout ✅
        ├── register() ← Agent loop wall-clock timeout ✅
        ├── register() ← Agent loop pause timeout ✅
        ├── register() ← Workflow pause timeout ✅ (already done)
        └── register() ← Workflow execution timeout ✅
```