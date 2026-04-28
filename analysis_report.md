# Type Check Report

## Type Issues Summary

- **Total**: 227
- **❌** error: 227
- **Categories**: 14
- **Files Affected**: 51
- **Packages Affected**: 2

## Breakdown by Category

- **[TS2304]**: 67 occurrence(s)
- **[TS2307]**: 66 occurrence(s)
- **[TS2339]**: 31 occurrence(s)
- **[TS2305]**: 19 occurrence(s)
- **[TS7006]**: 13 occurrence(s)
- **[TS2353]**: 13 occurrence(s)
- **[TS2345]**: 4 occurrence(s)
- **[TS2551]**: 4 occurrence(s)
- **[TS2724]**: 3 occurrence(s)
- **[TS18046]**: 2 occurrence(s)
- **[TS4111]**: 2 occurrence(s)
- **[TS4112]**: 1 occurrence(s)
- **run failed: command exited (2)**: 1 occurrence(s)
- **[TS7053]**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/sdk` (226 issue(s))

#### `core/di/container-config.ts` (34 item(s))

- ❌ **error** `[[TS2305]]` at line 36:10: Module '"../../workflow/stores/workflow-graph-registry.js"' has no exported member 'GraphRegistry'.
- ❌ **error** `[[TS2307]]` at line 49:28: Cannot find module '../../workflow/execution/thread-pool.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 64:38: Cannot find module '../../workflow/execution/coordinators/thread-state-transitor.js' or its corresponding type declarations.
- ❌ **error** `[[TS2305]]` at line 67:10: Module '"../../workflow/message/workflow-conversation-session.js"' has no exported member 'GraphConversationSession'.
- ❌ **error** `[[TS2307]]` at line 71:32: Cannot find module '../../workflow/execution/executors/thread-executor.js' or its corresponding type declarations.
- ... and 29 more

#### `workflow/execution/coordinators/node-execution-coordinator.ts` (22 item(s))

- ❌ **error** `[[TS2307]]` at line 41:8: Cannot find module '../handlers/subworkflow-handler.js' or its corresponding type declarations.
- ❌ **error** `[[TS2724]]` at line 48:10: '"@wf-agent/types"' has no exported member named 'SUBWORKFLOW_METADATA_KEYS'. Did you mean 'isWorkflowMetadata'?
- ❌ **error** `[[TS2305]]` at line 48:37: Module '"@wf-agent/types"' has no exported member 'SubworkflowBoundaryType'.
- ❌ **error** `[[TS2305]]` at line 56:3: Module '"../utils/event/index.js"' has no exported member 'buildWorkflowExecutionPausedEvent'.
- ❌ **error** `[[TS2305]]` at line 57:3: Module '"../utils/event/index.js"' has no exported member 'buildWorkflowExecutionCancelledEvent'.
- ... and 17 more

#### `workflow/message/workflow-conversation-session.ts` (11 item(s))

- ❌ **error** `[[TS2307]]` at line 21:8: Cannot find module '../../../core/messaging/conversation-session.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 22:35: Cannot find module '../../../core/registry/tool-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 24:40: Cannot find module '../../../utils/contextual-logger.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 25:48: Cannot find module '../../../resources/dynamic/prompts/fragments/available-tools.js' or its corresponding type declarations.
- ❌ **error** `[[TS2339]]` at line 74:30: Property 'getAllMessages' does not exist on type 'WorkflowConversationSession'.
- ... and 6 more

#### `api/shared/resources/index.ts` (10 item(s))

- ❌ **error** `[[TS2307]]` at line 17:39: Cannot find module '../../workflow/resources/checkpoints/checkpoint-resource-api.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 24:8: Cannot find module '../../workflow/resources/messages/message-resource-api.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 31:8: Cannot find module '../../workflow/resources/variables/variable-resource-api.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 34:36: Cannot find module '../../workflow/resources/triggers/trigger-resource-api.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 40:37: Cannot find module '../../workflow/resources/workflows/workflow-registry-api.js' or its corresponding type declarations.
- ... and 5 more

#### `workflow/execution/factories/llm-context-factory.ts` (9 item(s))

- ❌ **error** `[[TS2305]]` at line 12:15: Module '"../../stores/workflow-execution-registry.js"' has no exported member 'ThreadRegistry'.
- ❌ **error** `[[TS2305]]` at line 14:15: Module '"../../stores/workflow-graph-registry.js"' has no exported member 'GraphRegistry'.
- ❌ **error** `[[TS2304]]` at line 27:19: Cannot find name 'WorkflowExecutionRegistry'.
- ❌ **error** `[[TS2304]]` at line 30:19: Cannot find name 'WorkflowGraphRegistry'.
- ❌ **error** `[[TS2304]]` at line 37:20: Cannot find name 'WorkflowExecutionRegistry'.
- ... and 4 more

*... and 45 more files in this package*

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (2)

