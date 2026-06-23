# Type Check Report

## Type Issues Summary

- **Total**: 2
- **❌** error: 2
- **Categories**: 2
- **Files Affected**: 2
- **Packages Affected**: 2

## Breakdown by Category

- **run failed: command exited (2)**: 1 occurrence(s)
- **[TS2739]**: 1 occurrence(s)

## Details by Package

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (2)

### Package: `@wf-agent/sdk` (1 issue(s))

#### `api/agent/resources/agent-loop-registry-api.ts` (1 item(s))

- ❌ **error** `[[TS2739]]` at line 265:11: Type '{ CREATED: number; RUNNING: number; PAUSED: number; COMPLETED: number; FAILED: number; CANCELLED: number; }' is missing the following properties from type 'Record<AgentLoopStatus, number>': STOPPED, TIMEOUT

