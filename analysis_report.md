# Type Check Report

## Type Issues Summary

- **Total**: 137
- **❌** error: 137
- **Categories**: 7
- **Files Affected**: 47
- **Packages Affected**: 2

## Breakdown by Category

- **[TS2307]**: 96 occurrence(s)
- **[TS2305]**: 22 occurrence(s)
- **[TS7006]**: 11 occurrence(s)
- **[TS2724]**: 4 occurrence(s)
- **[TS2694]**: 2 occurrence(s)
- **run failed: command exited (2)**: 1 occurrence(s)
- **[TS2532]**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/sdk` (136 issue(s))

#### `core/di/container-config.ts` (29 item(s))

- ❌ **error** `[[TS2307]]` at line 36:31: Cannot find module '../../graph/stores/graph-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 37:32: Cannot find module '../../graph/stores/thread-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 46:30: Cannot find module '../../graph/stores/task/task-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 47:27: Cannot find module '../../graph/stores/task/task-queue.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 48:34: Cannot find module '../../graph/stores/workflow-registry.js' or its corresponding type declarations.
- ... and 24 more

#### `api/graph/resources/checkpoints/checkpoint-resource-api.ts` (9 item(s))

- ❌ **error** `[[TS2307]]` at line 7:33: Cannot find module '../../../../graph/checkpoint/checkpoint-state-manager.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 9:39: Cannot find module '../../../../graph/checkpoint/checkpoint-coordinator.js' or its corresponding type declarations.
- ❌ **error** `[[TS2724]]` at line 15:10: '"@wf-agent/types"' has no exported member named 'ThreadStatus'. Did you mean 'ThreadStats'?
- ❌ **error** `[[TS2307]]` at line 197:17: Cannot find module '../../../../graph/stores/thread-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 200:17: Cannot find module '../../../../graph/stores/workflow-registry.js' or its corresponding type declarations.
- ... and 4 more

#### `graph/entities/thread-entity.ts` (8 item(s))

- ❌ **error** `[[TS2305]]` at line 14:15: Module '"@wf-agent/types"' has no exported member 'Thread'.
- ❌ **error** `[[TS2724]]` at line 14:23: '"@wf-agent/types"' has no exported member named 'ThreadStatus'. Did you mean 'ThreadStats'?
- ❌ **error** `[[TS2305]]` at line 14:37: Module '"@wf-agent/types"' has no exported member 'ThreadType'.
- ❌ **error** `[[TS2305]]` at line 15:15: Module '"@wf-agent/types"' has no exported member 'PreprocessedGraph'.
- ❌ **error** `[[TS2307]]` at line 16:38: Cannot find module '../state-managers/execution-state.js' or its corresponding type declarations.
- ... and 3 more

#### `graph/graph-builder/index.ts` (8 item(s))

- ❌ **error** `[[TS2307]]` at line 6:30: Cannot find module './graph-builder.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 7:32: Cannot find module './graph-navigator.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 12:52: Cannot find module './utils/graph-analyzer.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 14:30: Cannot find module './utils/graph-cycle-detector.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 16:37: Cannot find module './utils/graph-reachability-analyzer.js' or its corresponding type declarations.
- ... and 3 more

#### `graph/graph-builder/workflow-processor.ts` (7 item(s))

- ❌ **error** `[[TS2305]]` at line 10:3: Module '"@wf-agent/types"' has no exported member 'WorkflowDefinition'.
- ❌ **error** `[[TS2305]]` at line 15:3: Module '"@wf-agent/types"' has no exported member 'PreprocessedGraph'.
- ❌ **error** `[[TS2307]]` at line 21:30: Cannot find module './graph-builder.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 25:39: Cannot find module '../entities/preprocessed-graph-data.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 34:36: Cannot find module '../stores/graph-registry.js' or its corresponding type declarations.
- ... and 2 more

*... and 41 more files in this package*

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (2)

