# Type Check Report

## Type Issues Summary

- **Total**: 38
- **❌** error: 38
- **Categories**: 5
- **Files Affected**: 12
- **Packages Affected**: 2

## Breakdown by Category

- **[TS2305]**: 31 occurrence(s)
- **[TS2724]**: 4 occurrence(s)
- **[TS2353]**: 1 occurrence(s)
- **[TS2345]**: 1 occurrence(s)
- **run failed: command exited (2)**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/cli-app` (37 issue(s))

#### `src/adapters/agent-loop-adapter.ts` (14 item(s))

- ❌ **error** `[[TS2305]]` at line 8:3: Module '"@wf-agent/sdk"' has no exported member 'AgentLoopFactory'.
- ❌ **error** `[[TS2305]]` at line 9:3: Module '"@wf-agent/sdk"' has no exported member 'AgentLoopCoordinator'.
- ❌ **error** `[[TS2724]]` at line 10:3: '"@wf-agent/sdk"' has no exported member named 'AgentLoopRegistry'. Did you mean 'AgentLoopRegistryAPI'?
- ❌ **error** `[[TS2305]]` at line 11:3: Module '"@wf-agent/sdk"' has no exported member 'AgentLoopExecutor'.
- ❌ **error** `[[TS2305]]` at line 12:3: Module '"@wf-agent/sdk"' has no exported member 'createAgentLoopCheckpoint'.
- ... and 9 more

#### `src/adapters/agent-loop-checkpoint-adapter.ts` (5 item(s))

- ❌ **error** `[[TS2305]]` at line 8:3: Module '"@wf-agent/sdk"' has no exported member 'createCheckpoint'.
- ❌ **error** `[[TS2724]]` at line 9:3: '"@wf-agent/sdk"' has no exported member named 'restoreFromCheckpoint'. Did you mean 'RestoreFromCheckpointParams'?
- ❌ **error** `[[TS2305]]` at line 10:8: Module '"@wf-agent/sdk"' has no exported member 'CheckpointDependencies'.
- ❌ **error** `[[TS2724]]` at line 11:8: '"@wf-agent/sdk"' has no exported member named 'CreateCheckpointOptions'. Did you mean 'CreateCheckpointParams'?
- ❌ **error** `[[TS2305]]` at line 14:15: Module '"@wf-agent/sdk"' has no exported member 'AgentLoopEntity'.

#### `src/adapters/tool-adapter.ts` (3 item(s))

- ❌ **error** `[[TS2305]]` at line 9:30: Module '"@wf-agent/sdk"' has no exported member 'loadConfigContent'.
- ❌ **error** `[[TS2305]]` at line 9:49: Module '"@wf-agent/sdk"' has no exported member 'parseToml'.
- ❌ **error** `[[TS2305]]` at line 9:60: Module '"@wf-agent/sdk"' has no exported member 'parseJson'.

#### `src/adapters/template-adapter.ts` (3 item(s))

- ❌ **error** `[[TS2305]]` at line 9:10: Module '"@wf-agent/sdk"' has no exported member 'loadConfigContent'.
- ❌ **error** `[[TS2305]]` at line 9:29: Module '"@wf-agent/sdk"' has no exported member 'parseNodeTemplate'.
- ❌ **error** `[[TS2305]]` at line 9:48: Module '"@wf-agent/sdk"' has no exported member 'parseTriggerTemplate'.

#### `src/adapters/workflow-adapter.ts` (2 item(s))

- ❌ **error** `[[TS2305]]` at line 9:40: Module '"@wf-agent/sdk"' has no exported member 'loadConfigContent'.
- ❌ **error** `[[TS2305]]` at line 9:59: Module '"@wf-agent/sdk"' has no exported member 'parseWorkflow'.

*... and 6 more files in this package*

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (2)

