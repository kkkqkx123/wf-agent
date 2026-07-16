# Tool Call Protocol Locking and Cross-Boundary Consistency Plan

> Analysis date: 2026-07-16
> Scope: protocol determination, locking, logging, metrics, cross-boundary consistency

---

## 1. Current Architecture Summary

### 1.1 Protocol Determination Chain

```
LLMProfile.toolCallFormat  (profile-level default, dynamic)
    ↓
LLMRequest.toolCallFormat  (request-level override, optional — NOT used today)
    ↓
LLMClientImpl.getFormatterConfig():
    → request.toolCallFormat ?? this.profile.toolCallFormat  (always falls back to profile)
    ↓
BaseFormatter.buildModeAwareRequest() / parseModeAwareResponse()
    ├── "native"       → buildNativeRequest()     → API function calling
    ├── "xml"          → buildTextModeRequest()   → XML in system prompt
    ├── "json_wrapped" → buildTextModeRequest()   → JSON + markers in system prompt
    └── "json_raw"     → buildTextModeRequest()   → raw JSON in system prompt
```

### 1.2 Key Findings

| # | Finding | Severity |
|---|---------|----------|
| F1 | `LLMRequest.toolCallFormat` override path is broken — `LLMExecutor.executeLLMCall()` never sets it | High |
| F2 | No protocol locking — profile can change mid-execution, causing silent protocol switch | High |
| F3 | No static validation — `AgentLoopDefinition` / workflow definitions have no `toolCallFormat` field, profile consistency across workflow nodes is not checked | Medium |
| F4 | `validateToolFormatCompatibility()` exists but is never called in the execution path | Medium |
| F5 | No cross-boundary protocol handling — sub-agent, workflow fork, triggered sub-workflow inherit parent's protocol implicitly with no conversion mechanism | Medium |
| F6 | No metrics for protocol-related events | Low |
| F7 | KV cache invalidation on protocol switch — no caching or warm-up strategy | Low |

---

## 2. Proposed Changes

### 2.1 Type-Level Additions

#### 2.1.1 Add `toolCallFormat` to `AgentLoopDefinition`

```typescript
// packages/types/src/agent/definition.ts
export interface AgentLoopDefinition {
  // ... existing fields ...

  /**
   * Tool call format configuration.
   *
   * Specifies the expected tool call protocol for this agent.
   * If set, must be compatible with the referenced LLMProfile.toolCallFormat.
   * If not set, inherits from LLMProfile.toolCallFormat.
   *
   * Purpose: allows static definition to declare the expected protocol,
   * enabling pre-check at load time and locking at runtime.
   */
  toolCallFormat?: ToolCallFormatConfig;
}
```

#### 2.1.2 Add `toolCallFormat` to `AgentLoopRuntimeConfig`

```typescript
// packages/types/src/agent/runtime-config.ts (or equivalent)
export interface AgentLoopRuntimeConfig {
  // ... existing fields ...

  /**
   * Locked tool call format.
   * Set at execution start from the resolved protocol (definition → profile → default).
   * Immutable during execution unless overridden by global policy.
   */
  toolCallFormat?: ToolCallFormatConfig;
}
```

#### 2.1.3 Add protocol violation policy to global SDK config

```typescript
// packages/types/src/sdk-config.ts (or equivalent)
export type ToolCallProtocolViolationPolicy =
  | "ignore"       // Silently use the new protocol (no warning)
  | "warn"         // Log warning, then use the new protocol
  | "fail"         // Throw an error, interrupting the execution
  | "auto_convert";// Convert history between formats automatically

export interface ToolCallProtocolConfig {
  /** Global default policy for protocol violation */
  violationPolicy: ToolCallProtocolViolationPolicy;

  /** Whether to lock protocol at execution start (default: true) */
  lockProtocol: boolean;

  /** Whether to enable cross-boundary protocol conversion (default: true) */
  enableCrossBoundaryConversion: boolean;
}
```

#### 2.1.4 Add `toolCallFormat` to workflow node definitions

Each workflow node that references an `LLMProfile` (agent node, LLM call node, etc.) should optionally declare its expected `toolCallFormat`. This enables pre-check across all nodes in a workflow definition.

### 2.2 Protocol Locking at Execution Start

#### 2.2.1 Resolution order

When an agent loop starts, the protocol is resolved with this priority:

1. `AgentLoopDefinition.toolCallFormat` (if set)
2. `LLMProfile.toolCallFormat` (profile-level)
3. `DEFAULT_TOOL_CALL_FORMAT_CONFIG` (native)

The resolved value is locked into the `AgentLoopEntity` state at **execution start** and remains immutable for the duration of that execution.

```typescript
// In AgentLoopExecutor.prepareExecution()
const resolvedProtocol = resolveToolCallFormatConfig({
  toolCallFormat: entity.config.toolCallFormat ?? profile.toolCallFormat,
});

// Lock into entity state
entity.lockToolCallFormat(resolvedProtocol);
```

#### 2.2.2 Runtime enforcement

In `LLMClientImpl.getFormatterConfig()`, after the existing merge, enforce the lock:

```typescript
protected getFormatterConfig(request: LLMRequest, stream: boolean = false): FormatterConfig {
  const effectiveFormat = request.toolCallFormat ?? this.profile.toolCallFormat;

  // If the entity has a locked protocol, enforce it
  if (request.lockedToolCallFormat) {
    const compatibility = validateToolFormatCompatibility(
      request.lockedToolCallFormat.format,
      effectiveFormat?.format || "native",
    );
    if (!compatibility.compatible) {
      handleProtocolViolation(request.lockedToolCallFormat, effectiveFormat);
    }
    // Override with locked protocol
    return {
      profile: this.profile,
      stream,
      toolCallFormat: request.lockedToolCallFormat,
    };
  }

  return {
    profile: this.profile,
    stream,
    toolCallFormat: effectiveFormat,
  };
}
```

#### 2.2.3 Add `lockedToolCallFormat` to `LLMRequest`

```typescript
// packages/types/src/llm/request.ts
export interface LLMRequest {
  // ... existing fields ...

  /**
   * Locked tool call format for this execution.
   * Set by the executor at execution start.
   * Overrides profile-level toolCallFormat.
   * The LLMClient must use this format regardless of profile changes.
   */
  lockedToolCallFormat?: ToolCallFormatConfig;
}
```

### 2.3 Static Pre-Check

#### 2.3.1 Agent definition validation

When loading an `AgentLoopDefinition`, validate:

1. If `toolCallFormat` is set, check it is a valid `ToolCallFormatConfig`
2. If `profileId` is also set, validate **compatibility** between the definition's `toolCallFormat` and the profile's `toolCallFormat`
3. Emit a warning (or error, per policy) on mismatch

```typescript
// In AgentLoopValidator or config processor
function validateAgentToolCallProtocol(
  definition: AgentLoopDefinition,
  profile: LLMProfile,
): ValidationResult {
  if (!definition.toolCallFormat && !profile.toolCallFormat) {
    return { valid: true }; // No explicit config, defaults to native
  }

  const defFormat = definition.toolCallFormat?.format || "native";
  const profileFormat = profile.toolCallFormat?.format || "native";

  return validateToolFormatCompatibility(defFormat, profileFormat);
}
```

#### 2.3.2 Workflow definition validation

For workflow definitions, validate **consistency across all nodes** that reference an LLM profile:

1. Collect all nodes with `profileId` references (agent nodes, LLM nodes)
2. Resolve each node's effective `toolCallFormat` (node-level → profile-level → default)
3. Check that **all nodes use the same protocol**
4. If node-level `toolCallFormat` is set, also check compatibility with the referenced profile
5. Report all inconsistencies at load time

```typescript
// In workflow graph validator
function validateWorkflowToolCallProtocolConsistency(
  nodes: WorkflowNode[],
  profileResolver: (profileId: string) => LLMProfile | undefined,
): ValidationResult[] {
  const results: ValidationResult[] = [];
  const protocols = new Set<string>();

  for (const node of nodes) {
    if (!node.profileId) continue;

    const profile = profileResolver(node.profileId);
    const nodeFormat = node.toolCallFormat?.format
      || profile?.toolCallFormat?.format
      || "native";

    protocols.add(nodeFormat);

    // Check node-profile compatibility
    if (node.toolCallFormat && profile?.toolCallFormat) {
      const compat = validateToolFormatCompatibility(
        node.toolCallFormat.format,
        profile.toolCallFormat.format,
      );
      if (!compat.compatible) {
        results.push({
          nodeId: node.id,
          valid: false,
          message: `Node ${node.id}: ${compat.reason}`,
        });
      }
    }
  }

  if (protocols.size > 1) {
    results.push({
      valid: false,
      message: `Inconsistent protocols across workflow nodes: ${[...protocols].join(", ")}`,
    });
  }

  return results;
}
```

#### 2.3.3 Load-time vs runtime

| Check | When | Action on failure |
|------|------|-------------------|
| Agent definition protocol validation | Config load / agent creation | Warning (configurable to error) |
| Workflow node protocol consistency | Workflow definition load | Error (configurable) |
| Runtime protocol switch | Per-iteration (LLM call) | Per `violationPolicy` (see §2.4) |

### 2.4 Protocol Violation Handling

#### 2.4.1 `handleProtocolViolation` function

```typescript
// packages/sdk/services/llm/formatters/tool-format-selector.ts (or new module)

export interface ProtocolViolationContext {
  lockedFormat: ToolCallFormatConfig;
  attemptedFormat: ToolCallFormatConfig | undefined;
  executionId: string;
  entityId?: string;
  profileId: string;
  iteration?: number;
}

export function handleProtocolViolation(
  context: ProtocolViolationContext,
  policy: ToolCallProtocolViolationPolicy,
): void {
  const logger = getLogger();

  switch (policy) {
    case "ignore":
      // Silently use the locked protocol, no logging
      return;

    case "warn":
      logger.warn("Tool call protocol violation detected", {
        lockedFormat: context.lockedFormat.format,
        attemptedFormat: context.attemptedFormat?.format,
        executionId: context.executionId,
        entityId: context.entityId,
        profileId: context.profileId,
        iteration: context.iteration,
      });
      // Record to metrics
      recordProtocolViolationMetric(context, "warn");
      // Continue with locked protocol
      return;

    case "fail":
      logger.error("Tool call protocol violation — interrupting execution", {
        lockedFormat: context.lockedFormat.format,
        attemptedFormat: context.attemptedFormat?.format,
        executionId: context.executionId,
      });
      recordProtocolViolationMetric(context, "fail");
      throw new ProtocolViolationError(
        `Tool call protocol conflict: locked "${context.lockedFormat.format}" ` +
        `but profile "${context.profileId}" attempted "${context.attemptedFormat?.format}". ` +
        `Execution interrupted per fail policy.`,
      );

    case "auto_convert":
      logger.info("Auto-converting tool call protocol", {
        from: context.attemptedFormat?.format,
        to: context.lockedFormat.format,
        executionId: context.executionId,
      });
      recordProtocolViolationMetric(context, "auto_convert");
      // The locked format is already enforced by the formatter.
      // HistoryConverter will handle the conversion on the next request.
      return;
  }
}
```

#### 2.4.2 Global configuration

The violation policy can be set at multiple levels:

```typescript
// Resolution order (lowest to highest priority):
// 1. Global SDK default: "warn" (stored in SDKConfig)
// 2. Per-agent: AgentLoopDefinition.violationPolicy
// 3. Per-execution: AgentLoopRuntimeConfig.violationPolicy
```

### 2.5 Logging Additions

#### 2.5.1 Log events

| Event | Log Level | Payload | Location |
|-------|-----------|---------|----------|
| Protocol locked at execution start | `info` | `{ executionId, entityId, profileId, format, source }` | `AgentLoopExecutor.prepareExecution()` |
| Protocol violation (warn) | `warn` | `{ lockedFormat, attemptedFormat, executionId, profileId, iteration }` | `handleProtocolViolation()` |
| Protocol violation (fail) | `error` | `{ lockedFormat, attemptedFormat, executionId, profileId, iteration }` | `handleProtocolViolation()` |
| Protocol violation (auto_convert) | `info` | `{ from, to, executionId, entityId }` | `handleProtocolViolation()` |
| Cross-boundary conversion | `info` | `{ sourceType, sourceId, targetType, targetId, fromFormat, toFormat }` | `CrossBoundaryConverter` |
| Static pre-check warning | `warn` | `{ definitionId, profileId, defFormat, profileFormat }` | `AgentLoopValidator` |
| Workflow protocol inconsistency | `error` | `{ workflowId, nodeIds, formats }` | Workflow graph validator |

#### 2.5.2 Logger context enrichments

Add `toolCallFormat` to the structured logger context for all LLM-related log entries:

```typescript
// In LLMClientImpl
const config = this.getFormatterConfig(request, stream);
logger.debug("LLM call", {
  profileId: this.profile.id,
  toolCallFormat: config.toolCallFormat?.format,
  stream,
  messageCount: request.messages.length,
});
```

### 2.6 Metrics Additions

#### 2.6.1 New metrics

Add to `AgentMetricsCollector` (or create a dedicated `ToolCallProtocolMetricsCollector`):

| Metric Name | Type | Labels | Description |
|-------------|------|--------|-------------|
| `agent.protocol.locked` | Counter | `format`, `profile_id`, `source` | Protocol locked at execution start |
| `agent.protocol.violation` | Counter | `locked_format`, `attempted_format`, `policy` | Protocol violation events |
| `agent.protocol.conversion` | Counter | `from_format`, `to_format`, `boundary_type` | Cross-boundary protocol conversions |
| `agent.protocol.static_mismatch` | Counter | `definition_id`, `profile_id` | Static pre-check mismatches |
| `workflow.protocol.inconsistency` | Counter | `workflow_id`, `node_count` | Workflow node protocol inconsistencies |

#### 2.6.2 Implementation

```typescript
// In AgentMetricsCollector
recordProtocolLocked(format: string, profileId: string, source: "definition" | "profile" | "default"): void {
  this.incrementCounter("agent.protocol.locked", {
    format,
    profile_id: profileId,
    source,
  });
}

recordProtocolViolation(
  lockedFormat: string,
  attemptedFormat: string,
  policy: ToolCallProtocolViolationPolicy,
): void {
  this.incrementCounter("agent.protocol.violation", {
    locked_format: lockedFormat,
    attempted_format: attemptedFormat,
    policy,
  });
}

recordProtocolConversion(
  fromFormat: string,
  toFormat: string,
  boundaryType: "sub_agent" | "workflow_fork" | "triggered_subworkflow",
): void {
  this.incrementCounter("agent.protocol.conversion", {
    from_format: fromFormat,
    to_format: toFormat,
    boundary_type: boundaryType,
  });
}
```

### 2.7 Cross-Boundary Protocol Handling

#### 2.7.1 Boundary types

| Boundary | Description | Parent-child relationship |
|----------|-------------|--------------------------|
| Sub-agent | Agent loop spawns a child agent loop | `AgentLoopEntity → AgentLoopEntity` |
| Workflow fork | Workflow node creates parallel child executions | `WorkflowExecution → WorkflowExecution[]` |
| Triggered sub-workflow | Agent loop triggers a workflow execution | `AgentLoopEntity → WorkflowExecution` |
| Workflow → Agent | Workflow node runs an agent loop | `WorkflowExecution → AgentLoopEntity` |

#### 2.7.2 CrossBoundaryConverter

```typescript
// packages/sdk/shared/messaging/cross-boundary-converter.ts

/**
 * Cross-boundary protocol converter.
 *
 * Converts conversation history and tool call format between different
 * execution contexts with potentially different protocols.
 */
export class CrossBoundaryConverter {
  /**
   * Convert messages from source protocol to target protocol.
   * This is the universal conversion function used by all boundary types.
   *
   * @param messages Messages to convert
   * @param sourceFormat Source protocol format
   * @param targetFormat Target protocol format
   * @param options Conversion options (custom markers, XML tags, etc.)
   * @returns Converted messages
   */
  static convert(
    messages: LLMMessage[],
    sourceFormat: ToolCallFormatConfig,
    targetFormat: ToolCallFormatConfig,
    options?: Partial<HistoryConversionOptions>,
  ): LLMMessage[] {
    // If formats are the same, no conversion needed
    if (sourceFormat.format === targetFormat.format) {
      return messages;
    }

    // Step 1: Convert from source format to canonical internal format
    // (always go through "native" as the canonical intermediate)
    const toCanonical = HistoryConverter.toNative(messages, sourceFormat);

    // Step 2: Convert from canonical to target format
    const fromCanonical = HistoryConverter.fromNative(toCanonical, targetFormat);

    return fromCanonical;
  }

  /**
   * Convert a single LLM result (tool calls) from source to target format.
   * Used when passing execution results across boundaries.
   */
  static convertToolCalls(
    toolCalls: LLMToolCall[],
    sourceFormat: ToolCallFormatConfig,
    targetFormat: ToolCallFormatConfig,
  ): LLMToolCall[] {
    if (sourceFormat.format === targetFormat.format) {
      return toolCalls;
    }

    // Convert tool calls to canonical format, then to target format
    // Implementation depends on the specific formats involved
    return toolCalls; // placeholder
  }
}
```

#### 2.7.3 Integration points

**Sub-agent spawn** (`AgentLoopEntity.spawnSubAgent()`):

```typescript
spawnSubAgent(options: SubAgentSpawnOptions): AgentLoopEntity {
  const subAgent = new AgentLoopEntity(/* ... */);

  // Resolve protocol for sub-agent
  const parentProtocol = this.getLockedToolCallFormat();
  const subAgentProtocol = resolveToolCallFormatConfig({
    toolCallFormat: subAgent.config.toolCallFormat ?? subAgentProfile.toolCallFormat,
  });

  if (enableCrossBoundaryConversion) {
    // Convert conversation history if protocols differ
    if (parentProtocol.format !== subAgentProtocol.format) {
      const convertedMessages = CrossBoundaryConverter.convert(
        this.getConversationMessages(),
        parentProtocol,
        subAgentProtocol,
      );
      subAgent.setInitialMessages(convertedMessages);

      recordProtocolConversionMetric(
        parentProtocol.format,
        subAgentProtocol.format,
        "sub_agent",
      );
    }
  }

  // Lock protocol on sub-agent
  subAgent.lockToolCallFormat(subAgentProtocol);

  return subAgent;
}
```

**Workflow fork** (in `WorkflowExecutor`):

```typescript
function createForkExecution(
  parentExecution: WorkflowExecution,
  forkNode: ForkNode,
): WorkflowExecution {
  const childExecution = new WorkflowExecution(/* ... */);

  // Each fork branch may have a different profile
  const childProtocol = resolveChildProtocol(childExecution, forkNode);

  if (parentProtocol.format !== childProtocol.format) {
    logger.info("Fork branch uses different protocol", {
      parentProtocol: parentProtocol.format,
      childProtocol: childProtocol.format,
      forkNodeId: forkNode.id,
    });

    // The child execution's initial input is converted
    childExecution.setInput(
      CrossBoundaryConverter.convertInput(
        parentExecution.getCurrentOutput(),
        parentProtocol,
        childProtocol,
      ),
    );
  }

  childExecution.lockToolCallFormat(childProtocol);
  return childExecution;
}
```

#### 2.7.4 Configuration option: strict mode

```typescript
export interface CrossBoundaryConfig {
  /**
   * How to handle protocol mismatches across execution boundaries.
   *
   * - "convert": Automatically convert messages between protocols (default)
   * - "inherit": Force child to inherit parent's protocol (ignores child's config)
   * - "strict": Reject if protocols differ (throw error)
   * - "warn_and_continue": Log warning, then accept the mismatch
   *     (only safe when both protocols are text-based and compatible)
   */
  mismatchStrategy: "convert" | "inherit" | "strict" | "warn_and_continue";
}
```

### 2.8 HistoryConverter Enhancements

#### 2.8.1 Add `toNative()` and `fromNative()` methods

The current `HistoryConverter` only converts `native → text`. For robust cross-boundary conversion, we need the reverse path:

```typescript
// packages/sdk/shared/messaging/history-converter.ts

export class HistoryConverter {
  /**
   * Convert text-mode messages back to native format.
   * Reverses convertToTextMode() — parses tool calls from text content
   * back into structured toolCalls fields.
   */
  static toNative(
    messages: LLMMessage[],
    sourceFormat: ToolCallFormatConfig,
  ): LLMMessage[] {
    if (sourceFormat.format === "native") {
      return messages;
    }

    return messages.map(msg => {
      if (msg.role === "assistant" && msg.content) {
        // Parse tool calls from text content
        const toolCalls = ToolCallParser.parseFromText(
          msg.content,
          getToolCallParserOptions(sourceFormat.format, sourceFormat.markers),
        );
        if (toolCalls.length > 0) {
          // Remove the tool call text from content and add structured toolCalls
          return {
            ...msg,
            content: msg.content, // Keep original content
            toolCalls: toolCalls.map(tc => ({
              id: tc.id,
              type: "function" as const,
              function: {
                name: tc.function.name,
                arguments: tc.function.arguments,
              },
            })),
          };
        }
      }
      if (msg.role === "user" && msg.content) {
        // Detect tool result text and convert back to tool role
        // (requires heuristic detection of tool result markers)
        // ... implementation depends on marker conventions
      }
      return msg;
    });
  }

  /**
   * Convert native messages to text-mode format.
   * Currently named convertToTextMode(); fromNative is an alias for clarity.
   */
  static fromNative(
    messages: LLMMessage[],
    targetFormat: ToolCallFormatConfig,
  ): LLMMessage[] {
    return HistoryConverter.convertToTextMode(messages, targetFormat.format, {
      xmlTags: targetFormat.xmlTags,
      markers: targetFormat.markers,
    });
  }
}
```

> **Note on `toNative()`**: The reverse conversion (text → native) is inherently lossy because tool call IDs, structured arguments, and role distinctions are embedded in free-form text. The heuristic parser must be robust enough to handle edge cases. A future improvement could store the original structured data in message metadata.

---

## 3. Implementation Plan

### Phase 1: Type Definitions and Schema Updates

| Task | Files | Effort |
|------|-------|--------|
| Add `ToolCallFormatConfig` to `AgentLoopDefinition` | `packages/types/src/agent/definition.ts`, `schemas.ts` | Small |
| Add `ToolCallFormatConfig` to `AgentLoopRuntimeConfig` | `packages/types/src/agent/runtime-config.ts` | Small |
| Add `ToolCallProtocolViolationPolicy` and `ToolCallProtocolConfig` to SDK config | `packages/types/src/sdk-config.ts` (or new file) | Small |
| Add `lockedToolCallFormat` to `LLMRequest` | `packages/types/src/llm/request.ts` | Small |
| Add `toolCallFormat` to workflow node types | `packages/types/src/workflow/node.ts` | Medium |

### Phase 2: Protocol Locking

| Task | Files | Effort |
|------|-------|--------|
| Add `lockToolCallFormat()` to `AgentLoopEntity` | `packages/sdk/agent/entities/agent-loop-entity.ts` | Small |
| Resolve and lock protocol in `AgentLoopExecutor.prepareExecution()` | `packages/sdk/agent/execution/executors/agent-loop-executor.ts` | Small |
| Enforce locked protocol in `LLMClientImpl.getFormatterConfig()` | `packages/sdk/services/llm/client.ts` | Small |
| Pass `lockedToolCallFormat` through `LLMExecutor.executeLLMCall()` | `packages/sdk/services/executors/llm-executor.ts` | Small |
| Add `handleProtocolViolation()` function | `packages/sdk/services/llm/formatters/tool-format-selector.ts` | Small |

### Phase 3: Static Pre-Check

| Task | Files | Effort |
|------|-------|--------|
| Implement `validateAgentToolCallProtocol()` in `AgentLoopValidator` | `packages/sdk/agent/validation/agent-loop-validator.ts` | Small |
| Implement `validateWorkflowToolCallProtocolConsistency()` in workflow validator | `packages/sdk/workflow/validation/` | Medium |
| Add pre-check call chain to agent creation / workflow definition loading | Various | Medium |

### Phase 4: Cross-Boundary Conversion

| Task | Files | Effort |
|------|-------|--------|
| Implement `CrossBoundaryConverter` class | `packages/sdk/shared/messaging/cross-boundary-converter.ts` | Medium |
| Implement `HistoryConverter.toNative()` | `packages/sdk/shared/messaging/history-converter.ts` | Medium |
| Integrate converter into `AgentLoopEntity.spawnSubAgent()` | `packages/sdk/agent/entities/agent-loop-entity.ts` | Small |
| Integrate converter into workflow fork creation | `packages/sdk/workflow/execution/` | Medium |
| Add `CrossBoundaryConfig` and `mismatchStrategy` | Types + integration points | Small |

### Phase 5: Logging and Metrics

| Task | Files | Effort |
|------|-------|--------|
| Add protocol lock logging to `AgentLoopExecutor` | `packages/sdk/agent/execution/executors/agent-loop-executor.ts` | Small |
| Add protocol violation logging to `handleProtocolViolation()` | `packages/sdk/services/llm/formatters/tool-format-selector.ts` | Small |
| Add cross-boundary conversion logging to converter | `packages/sdk/shared/messaging/cross-boundary-converter.ts` | Small |
| Add static pre-check logging to validators | Various | Small |
| Add `recordProtocolLocked/recordProtocolViolation/recordProtocolConversion` to `AgentMetricsCollector` | `packages/sdk/metrics/agent-collector.ts` | Small |
| Add `protocol_violation_policy` label to existing agent metrics | `packages/sdk/metrics/agent-collector.ts` | Small |

### Phase 6: Tests

| Task | Effort |
|------|--------|
| Unit tests for `handleProtocolViolation()` with all policies | Medium |
| Unit tests for `HistoryConverter.toNative()` | Medium |
| Unit tests for `CrossBoundaryConverter` | Medium |
| Unit tests for protocol locking in `AgentLoopExecutor` | Medium |
| Integration tests for protocol consistency across sub-agent spawn | Medium |
| Integration tests for workflow fork with different protocols | Medium |
| Tests for static pre-check validators | Medium |

---

## 4. Migration / Backward Compatibility

| Change | Backward Compatible? | Notes |
|--------|---------------------|-------|
| New fields on `AgentLoopDefinition` | Yes | Optional fields, default to undefined |
| Protocol locking (default: on) | Yes | Existing profiles with a single, stable `toolCallFormat` will lock correctly |
| `handleProtocolViolation` with default "warn" policy | Yes | Degrades gracefully; no existing behavior changes |
| `CrossBoundaryConverter` | Yes | Only invoked when protocols differ; no-op when same |
| New metrics | Yes | No schema changes, additive only |

**Important**: Since the project explicitly states "No backward compatibility needed" (per AGENTS.md), we can be more aggressive with breaking changes if needed. The above table is informational.

---

## 5. Open Questions

1. **Should `HistoryConverter.toNative()` be lossy or should we store original structured data in message metadata?**
   - Recommendation: Start with lossy parsing (heuristic), add metadata storage in a follow-up.

2. **Should the "inherit" strategy be the default for cross-boundary?**
   - Recommendation: "convert" should be the default. "inherit" could lead to silent protocol changes when the child's profile differs from the parent.

3. **Should `auto_convert` policy on protocol violation also convert the in-memory conversation history?**
   - Recommendation: Yes, but only for the current request's snapshot. The persistent history stays in the locked format. The `auto_convert` policy is essentially "ignore the lock once, but don't update it."

4. **Should protocol lock be persisted in checkpoints?**
   - Recommendation: Yes. The locked `ToolCallFormatConfig` should be part of the checkpoint snapshot so that on resume, the same protocol is used regardless of profile changes.

---

## 6. Appendix: Data Flow Diagrams

### 6.1 Protocol Locking Flow

```
AgentLoopExecutor.execute(entity, stateCoordinator)
    │
    ├─ 1. Resolve protocol:
    │      entity.config.toolCallFormat ?? profile.toolCallFormat ?? DEFAULT
    │
    ├─ 2. Lock protocol:
    │      entity.lockToolCallFormat(resolvedProtocol)
    │
    ├─ 3. Log: "Protocol locked" { format, source, profileId }
    │
    ├─ 4. Record metric: agent.protocol.locked
    │
    └─ 5. Execute iteration loop:
           │
           └─ Per iteration:
                  LLMClientImpl.generate(request)
                      │
                      ├─ getFormatterConfig()
                      │   ├─ If lockedToolCallFormat exists:
                      │   │   ├─ Validate compatibility
                      │   │   ├─ On mismatch → handleProtocolViolation()
                      │   │   └─ Use locked format
                      │   └─ Else: use normal resolution
                      │
                      └─ buildModeAwareRequest()
                          └─ Uses resolved format from config
```

### 6.2 Cross-Boundary Conversion Flow

```
Parent Agent (format: "native")
    │
    ├─ spawnSubAgent({ agentProfile: "code-reviewer" })
    │   │
    │   ├─ Resolve child protocol: "xml" (from code-reviewer profile)
    │   │
    │   ├─ Protocols differ → invoke CrossBoundaryConverter
    │   │   ├─ HistoryConverter.toNative(parentMessages, "native") → no-op
    │   │   └─ HistoryConverter.fromNative(messages, "xml") → convert
    │   │
    │   ├─ Log: "Cross-boundary protocol conversion"
    │   │
    │   ├─ Record metric: agent.protocol.conversion
    │   │
    │   └─ Set converted messages as sub-agent initial state
    │
    └─ Sub-agent executes with locked "xml" protocol
```

---

## 7. Summary of Recommendations

| # | Recommendation | Priority | Phase |
|---|---------------|----------|-------|
| R1 | Fix `LLMRequest.toolCallFormat` override path | P0 | 2 |
| R2 | Implement protocol locking at execution start | P0 | 2 |
| R3 | Add `handleProtocolViolation()` with configurable policy | P0 | 2 |
| R4 | Add protocol-related logging and metrics | P1 | 5 |
| R5 | Add static pre-check for agent definitions | P1 | 3 |
| R6 | Add workflow-level protocol consistency validation | P1 | 3 |
| R7 | Implement `CrossBoundaryConverter` for sub-agent / fork | P2 | 4 |
| R8 | Implement `HistoryConverter.toNative()` for reverse conversion | P2 | 4 |
| R9 | Persist protocol lock in checkpoints | P2 | Follow-up |
| R10 | Add metadata-based structured data for lossless cross-boundary conversion | P3 | Follow-up |