# Tool Call Protocol — Test Coverage Analysis & Plan

## 1. System Overview

The Tool Call Protocol subsystem spans **6 layers** across the SDK:

```
┌──────────────────────────────────────────────────────────────┐
│ Layer 1: Config Resolution & Validation                      │
│  AgentLoopValidator.validateAgentToolCallProtocol()           │
│  ProtocolConsistencyValidator.validateWorkflowToolCallProtocol│
├──────────────────────────────────────────────────────────────┤
│ Layer 2: Protocol Locking                                     │
│  AgentLoopExecutor.prepareExecution() → resolveToolCallFormat │
│  AgentLoopEntity.lockToolCallFormat() / getLockedToolCall()   │
│  AgentLoopState.lockedToolCallFormat (checkpoint persistence) │
├──────────────────────────────────────────────────────────────┤
│ Layer 3: Protocol Enforcement (LLMClient)                     │
│  LLMClientImpl.getFormatterConfig() → handleProtocolViolation │
│  FormatterConfig.toolCallFormat / protocolAutoConverted       │
├──────────────────────────────────────────────────────────────┤
│ Layer 4: History Conversion (HistoryConverter)                │
│  convertToTextMode() / toNative() / fromNative()              │
│  stripToolCallText() / removeRawJsonToolCalls()               │
│  parseToolResultFromText() / parseRawJsonToolResult()         │
├──────────────────────────────────────────────────────────────┤
│ Layer 5: Cross-Boundary Conversion (CrossBoundaryConverter)   │
│  convert() / convertToolCalls()                               │
│  Integrated in workflow-execution-builder.ts                  │
├──────────────────────────────────────────────────────────────┤
│ Layer 6: LLM Formatter Output (OpenAIChatFormatter)           │
│  buildTextModeRequest() / buildNativeRequest()                │
│  injectToolDeclarations() / parseTextModeResponse()           │
│  ToolCallParser.parseFromText() / ToolDeclarationFormatter    │
└──────────────────────────────────────────────────────────────┘
```

---

## 2. Current Test Coverage

### 2.1 Unit Tests — Existing

| Test File | Tests | Coverage |
|-----------|-------|----------|
| `tool-format-selector.test.ts` | 20 | `getToolFormatTemplates`, `getToolFormatDisplayName`, `getToolFormatDescription`, `getAvailableToolFormats`, `handleProtocolViolation` (4 policies + edge cases) |
| `agent-loop-validator.protocol.test.ts` | 7 | `validateAgentToolCallProtocol` — 5 scenarios (no config, compatible, incompatible, missing profile, edge cases) |
| `protocol-consistency-validator.test.ts` | 11 | `validateWorkflowToolCallProtocolConsistency` — 5 scenarios (no relevant nodes, consistent, inconsistent, compatibility, edge cases) |
| `history-converter.test.ts` | 17 | `convertToTextMode` (5), `convertAssistantMessage` (4), `convertToolResultMessage` (2), `needsConversion` (6) — basic XML/JSON only |
| `cross-boundary-converter.test.ts` | 12 | `convert` — native↔xml/`json_wrapped`, same format, empty messages, `convertToolCalls` |
| `llm-execution-coordinator.test.ts` | 20 | Core execution, tool calls, token tracking, interruption, events — **no protocol coverage** |
| `agent-iteration-coordinator.test.ts` | 11 | Iteration, tool calls, interruption, events — basic protocol passing only |
| `agent-loop-executor.test.ts` | 19 | Constructor, execute, stream, coordinator creation — basic protocol passing only |
| `llm-executor.test.ts` | 12 | Basic execution, streaming, abort, interruption — **no protocol coverage** |
| `tool-call-parser.test.ts` | — | XML parsing |
| `tool-call-executor.test.ts` | — | Tool execution |
| `tool-declaration-formatter.test.ts` | — | Tool declaration formatting |

### 2.2 Integration & E2E Tests — Existing

**No protocol-related tests exist** in `packages/sdk/__tests__/integration/` or `packages/sdk/__tests__/e2e/`.

---

## 3. Test Coverage Gaps

### 3.1 Layer 1: Config Resolution & Validation — ✅ Adequate

| Component | Status | Notes |
|-----------|--------|-------|
| `validateAgentToolCallProtocol` | ✅ Covered | All 5 scenarios tested |
| `validateWorkflowToolCallProtocolConsistency` | ✅ Covered | All 5 scenarios tested |

### 3.2 Layer 2: Protocol Locking — ⚠️ Major Gap

| Component | Status | Notes |
|-----------|--------|-------|
| `AgentLoopEntity.lockToolCallFormat()` | ❌ Not tested | Immutability guard (double-lock), freeze behavior |
| `AgentLoopEntity.getLockedToolCallFormat()` | ❌ Not tested | Return value, undefined state |
| `AgentLoopState.lockedToolCallFormat` (getter/setter) | ❌ Not tested | State persistence |
| `AgentLoopState serialization (snapshot)` | ❌ Not tested | `lockedToolCallFormat` in checkpoint snapshots |
| `AgentLoopState restoreFromSnapshot()` | ❌ Not tested | `lockedToolCallFormat` restoration |
| `AgentLoopExecutor.resolveToolCallFormat()` | ❌ Not tested | Priority resolution logic (definition > profile > default) |
| `AgentLoopExecutor.prepareExecution()` protocol locking | ❌ Not tested | End-to-end flow from config → lock → log/metrics |
| Checkpoint restore (entity) | ❌ Not tested | `AgentLoopEntity.fromSnapshot()` `lockedToolCallFormat` bypass |

### 3.3 Layer 3: Protocol Enforcement (LLMClient) — ⚠️ Major Gap

| Component | Status | Notes |
|-----------|--------|-------|
| `LLMClientImpl.getFormatterConfig()` locked format | ❌ Not tested | Locked format enforcement vs. profile format |
| `LLMClientImpl.getFormatterConfig()` violation detection | ❌ Not tested | Format mismatch detection logic |
| `LLMClientImpl.getFormatterConfig()` policy resolution | ❌ Not tested | `request-level > global default` priority |
| `FormatterConfig.protocolAutoConverted` flag | ❌ Not tested | Auto-convert flag propagation |
| `LLMClientImpl.generate()` with locked format | ❌ Not tested | End-to-end enforcement |
| `LLMClientImpl.generateStream()` with locked format | ❌ Not tested | Streaming enforcement |

### 3.4 Layer 4: History Conversion — ⚠️ Partial Coverage

| Component | Status | Notes |
|-----------|--------|-------|
| `HistoryConverter.convertToTextMode()` | ✅ Basic | XML/JSON wrapped tested, **json_raw missing** |
| `HistoryConverter.convertAssistantMessage()` | ✅ Basic | XML/JSON wrapped tested, **json_raw covered minimally** |
| `HistoryConverter.convertToolResultMessage()` | ✅ Basic | XML/JSON tested, **json_raw missing** |
| `HistoryConverter.toNative()` | ❌ Not tested | No direct tests (only indirect via CrossBoundaryConverter) |
| `HistoryConverter.fromNative()` | ❌ Not tested | **Completely untested** |
| `HistoryConverter.stripToolCallText()` (updated) | ❌ Not tested | **Custom XML tags, json_raw, removeRawJsonToolCalls** |
| `HistoryConverter.removeRawJsonToolCalls()` | ❌ Not tested | **New method, completely untested** |
| `HistoryConverter.parseToolResultFromText()` | ❌ Not tested | **XML, json_wrapped, json_raw parsing** |
| `HistoryConverter.parseRawJsonToolResult()` | ❌ Not tested | **New method, completely untested** |
| Conversion with custom XML tags (`xmlTags`) | ❌ Not tested | Non-default tag names |
| Conversion with custom JSON markers (`markers`) | ❌ Not tested | Non-default marker strings |
| Round-trip: `convertToTextMode` → `toNative` | ❌ Not tested | `originalToolCalls` metadata preservation |
| Round-trip: `fromNative` → `convertToTextMode` | ❌ Not tested | Bidirectional conversion fidelity |

### 3.5 Layer 5: Cross-Boundary Conversion — ⚠️ Partial Coverage

| Component | Status | Notes |
|-----------|--------|-------|
| `CrossBoundaryConverter.convert()` native↔xml | ✅ Covered | Basic conversion tested |
| `CrossBoundaryConverter.convert()` native↔json_wrapped | ✅ Covered | Basic conversion tested |
| `CrossBoundaryConverter.convert()` with json_raw | ❌ Not tested | **json_raw format missing** |
| `CrossBoundaryConverter.convert()` with conversion options | ✅ Covered | stripToolCallText, restoreToolRole |
| `CrossBoundaryConverter.convertToolCalls()` | ✅ Covered | Basic passthrough tested |
| Integration in `workflow-execution-builder.ts` | ❌ Not tested | Cross-boundary at fork/agent boundary |
| Metric/event recording during conversion | ❌ Not tested | |

### 3.6 Layer 6: LLM Formatter — ⚠️ Major Gap

| Component | Status | Notes |
|-----------|--------|-------|
| `OpenAIChatFormatter.buildTextModeRequest()` | ❌ Not tested | XML/json_wrapped/json_raw text mode |
| `OpenAIChatFormatter.buildNativeRequest()` | ❌ Not tested | Native function call mode |
| `OpenAIChatFormatter.injectToolDeclarations()` | ❌ Not tested | System prompt injection |
| `OpenAIChatFormatter.parseTextModeResponse()` | ❌ Not tested | Response parsing |
| `ToolCallParser.parseFromText()` | ✅ Covered | XML parsing |
| `ToolCallParser.parseFromJson()` | ❌ Not tested | JSON parsing |
| `ToolDeclarationFormatter.formatTools()` | ✅ Covered | Basic formatting |
| `ToolDeclarationFormatter.formatTools()` descriptionStyle | ❌ Not tested | detailed/compact/minimal (not yet implemented) |

### 3.7 Integration & E2E — ⚠️ Completely Missing

| Scenario | Status | Notes |
|----------|--------|-------|
| Agent loop execution with XML format | ❌ Not tested | End-to-end protocol flow |
| Agent loop execution with json_wrapped format | ❌ Not tested | End-to-end protocol flow |
| Agent loop execution with json_raw format | ❌ Not tested | End-to-end protocol flow |
| Protocol violation → `fail` policy | ❌ Not tested | Error propagation |
| Protocol violation → `auto_convert` policy | ❌ Not tested | Auto-conversion flow |
| Cross-boundary sub-agent (different protocol) | ❌ Not tested | Parent→child protocol conversion |
| Workflow fork with different protocol | ❌ Not tested | Fork→branch protocol conversion |
| Checkpoint → resume with locked protocol | ❌ Not tested | Protocol survives checkpoint/restore |
| Streaming with text-mode protocol | ❌ Not tested | Stream path with protocol |

---

## 4. Test Plan & Priority

### Phase 1: Unit Tests (P0 — Protect existing functionality)

| # | Test | File | Priority | Rationale |
|---|------|------|----------|-----------|
| 1 | `AgentLoopEntity.lockToolCallFormat` — first lock succeeds, second lock throws, freeze behavior | `agent-loop-entity.test.ts` | P0 | Core locking contract |
| 2 | `AgentLoopEntity.getLockedToolCallFormat` — returns correct value, undefined before lock | `agent-loop-entity.test.ts` | P0 | Core locking contract |
| 3 | `AgentLoopExecutor.resolveToolCallFormat` — definition > profile > default, priority chain | `agent-loop-executor.test.ts` | P0 | Resolution logic |
| 4 | `LLMClientImpl.getFormatterConfig` — locked format enforcement, mismatch detection, policy resolution | `llm-client.test.ts` | P0 | Core enforcement |
| 5 | `HistoryConverter.toNative` — direct tests for all formats (xml, json_wrapped, json_raw) | `history-converter.test.ts` | P0 | Conversion fidelity |
| 6 | `HistoryConverter.fromNative` — direct tests for all formats (xml, json_wrapped, json_raw) | `history-converter.test.ts` | P0 | Conversion fidelity |
| 7 | `HistoryConverter.stripToolCallText` — XML tags (default + custom), json_wrapped, json_raw, nested JSON | `history-converter.test.ts` | P0 | Text stripping |

### Phase 2: Unit Tests (P1 — Fill conversion gaps)

| # | Test | File | Priority | Rationale |
|---|------|------|----------|-----------|
| 8 | `HistoryConverter.parseToolResultFromText` — XML, json_wrapped, json_raw | `history-converter.test.ts` | P1 | Result parsing |
| 9 | `HistoryConverter.parseRawJsonToolResult` — valid JSON, nested JSON, invalid JSON, no match | `history-converter.test.ts` | P1 | New method coverage |
| 10 | `HistoryConverter.removeRawJsonToolCalls` — basic, nested, non-tool JSON preserved | `history-converter.test.ts` | P1 | New method coverage |
| 11 | `HistoryConverter` round-trip — `convertToTextMode` → `toNative`, `originalToolCalls` metadata | `history-converter.test.ts` | P1 | Bidirectional fidelity |
| 12 | `HistoryConverter` with custom `xmlTags` / `markers` | `history-converter.test.ts` | P1 | Configuration support |
| 13 | `CrossBoundaryConverter.convert` with json_raw format | `cross-boundary-converter.test.ts` | P1 | Missing format |

### Phase 3: Unit Tests (P2 — Edge cases & state)

| # | Test | File | Priority | Rationale |
|---|------|------|----------|-----------|
| 14 | `AgentLoopState.lockedToolCallFormat` — getter/setter, snapshot, restore | `agent-loop-state.test.ts` | P2 | State persistence |
| 15 | `AgentLoopEntity.fromSnapshot` — lockedToolCallFormat restore | `agent-loop-entity.test.ts` | P2 | Checkpoint restore |
| 16 | `LLMExecutionCoordinator.executeLLMCallWithMessages` — lockedToolCallFormat/violationPolicy passing | `llm-execution-coordinator.test.ts` | P2 | Protocol passing |
| 17 | `LLMExecutionCoordinator.executeSingleLLMCall` — lockedToolCallFormat/violationPolicy passing | `llm-execution-coordinator.test.ts` | P2 | Protocol passing |
| 18 | `OpenAIChatFormatter.buildTextModeRequest` — XML, json_wrapped, json_raw | `openai-chat.test.ts` | P2 | Formatter output |
| 19 | `OpenAIChatFormatter.buildNativeRequest` — native function call | `openai-chat.test.ts` | P2 | Formatter output |
| 20 | `ToolCallParser.parseFromJson` — json_raw nested JSON | `tool-call-parser.test.ts` | P2 | Parser coverage |

### Phase 4: Integration Tests (P1 — End-to-end flows)

| # | Test | Priority | Rationale |
|---|------|----------|-----------|
| 21 | Agent loop execution with text-mode protocol (XML) — full iteration cycle | P1 | End-to-end protocol |
| 22 | Agent loop execution with text-mode protocol (json_wrapped) — full iteration cycle | P1 | End-to-end protocol |
| 23 | Agent loop execution with text-mode protocol (json_raw) — full iteration cycle | P1 | End-to-end protocol |
| 24 | Protocol violation detection — `fail` policy triggers error | P1 | Error path |
| 25 | Protocol violation detection — `auto_convert` policy with `protocolAutoConverted` flag | P1 | Auto-convert |
| 26 | Cross-boundary sub-agent with different protocol — parent xml → child native | P1 | Boundary conversion |
| 27 | Workflow fork with different protocol — fork branch conversion | P1 | Boundary conversion |
| 28 | Checkpoint → resume preserves locked protocol format | P1 | State persistence |

### Phase 5: E2E Tests (P3 — Full system validation)

| # | Test | Priority | Rationale |
|---|------|----------|-----------|
| 29 | Agent loop with XML tool call format across multiple iterations | P3 | Full system |
| 30 | Sub-agent with different protocol, verify message conversion | P3 | Full system |
| 31 | Workflow with mixed protocol nodes, verify cross-boundary conversion | P3 | Full system |

---

## 5. Testing Strategy & Best Practices

### 5.1 Unit Test Pattern

Per the project's test conventions (`AGENTS.md`):
- Unit tests must be placed in `__tests__` directories **at the same directory level** as the source file
- Example: `agent/entities/__tests__/agent-loop-entity.test.ts`
- Mock external dependencies, test one component at a time

### 5.2 Integration Test Pattern

- Place in `__tests__/integration/<domain>/` under the package root
- Example: `packages/sdk/__tests__/integration/agent/agent-protocol.int.test.ts`
- Focus on cross-module functional collaboration

### 5.3 Key Scenarios to Test

**Protocol Locking:**
```typescript
// Arrange
entity = new AgentLoopEntity(id, definitionConfig);
expect(entity.getLockedToolCallFormat()).toBeUndefined();

// Act
entity.lockToolCallFormat({ format: "xml" });

// Assert
expect(entity.getLockedToolCallFormat()).toEqual({ format: "xml" });
expect(() => entity.lockToolCallFormat({ format: "native" })).toThrow();
```

**Protocol Enforcement:**
```typescript
// Arrange
request = { lockedToolCallFormat: { format: "xml" }, toolCallFormat: { format: "native" } };

// Act
const config = client.getFormatterConfig(request);

// Assert
expect(config.toolCallFormat?.format).toBe("xml"); // locked format wins
expect(config.protocolAutoConverted).toBe(true);
```

**History Conversion Round-trip:**
```typescript
// Arrange
const nativeMessages = [
  { role: "assistant", content: "Checking", toolCalls: [{ id: "c1", type: "function", function: { name: "get_weather", arguments: '{"city":"Beijing"}' } }] },
  { role: "tool", toolCallId: "c1", content: "Sunny" },
];

// Act: native → xml → native
const xmlMessages = HistoryConverter.convertToTextMode(nativeMessages, { format: "xml" });
const restored = HistoryConverter.toNative(xmlMessages, { format: "xml" });

// Assert: originalToolCalls metadata preserved for lossless round-trip
expect(restored[0].toolCalls).toBeDefined();
expect(restored[0].toolCalls[0].function.name).toBe("get_weather");
expect(restored[1].role).toBe("tool");
```

---

## 6. Summary

| Priority | Tests to Add | Files |
|----------|-------------|-------|
| P0 (Phase 1) | 7 | `agent-loop-entity`, `agent-loop-executor`, `llm-client`, `history-converter` |
| P1 (Phase 2) | 6 | `history-converter`, `cross-boundary-converter` |
| P2 (Phase 3) | 6 | `agent-loop-state`, `llm-execution-coordinator`, `openai-chat`, `tool-call-parser` |
| P1 (Phase 4) | 8 | Integration tests in `__tests__/integration/agent/` |
| P3 (Phase 5) | 3 | E2E tests in `__tests__/e2e/` |
| **Total** | **30** | |

**Untested areas by severity:**
- 🔴 Layer 2 (Protocol Locking): 7/7 components untested
- 🔴 Layer 3 (Enforcement): 5/5 components untested  
- 🟡 Layer 4 (History Conversion): 10/17 components untested or partially tested
- 🟡 Layer 6 (Formatter): 6/8 components untested
- 🔴 Integration/E2E: 0% coverage