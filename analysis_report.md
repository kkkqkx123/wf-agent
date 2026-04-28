# Type Check Report

## Type Issues Summary

- **Total**: 173
- **❌** error: 173
- **Categories**: 16
- **Files Affected**: 42
- **Packages Affected**: 2

## Breakdown by Category

- **[TS2339]**: 39 occurrence(s)
- **[TS2304]**: 35 occurrence(s)
- **[TS2307]**: 23 occurrence(s)
- **[TS2305]**: 19 occurrence(s)
- **[TS2353]**: 17 occurrence(s)
- **[TS2345]**: 10 occurrence(s)
- **[TS2551]**: 10 occurrence(s)
- **[TS7006]**: 6 occurrence(s)
- **[TS2724]**: 4 occurrence(s)
- **[TS2300]**: 2 occurrence(s)
- **[TS18046]**: 2 occurrence(s)
- **[TS4111]**: 2 occurrence(s)
- **[TS2739]**: 1 occurrence(s)
- **run failed: command exited (2)**: 1 occurrence(s)
- **[TS4112]**: 1 occurrence(s)
- **[TS2554]**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/sdk` (172 issue(s))

#### `workflow/execution/coordinators/node-execution-coordinator.ts` (20 item(s))

- ❌ **error** `[[TS2307]]` at line 41:8: Cannot find module '../handlers/subworkflow-handler.js' or its corresponding type declarations.
- ❌ **error** `[[TS2724]]` at line 48:10: '"@wf-agent/types"' has no exported member named 'SUBWORKFLOW_METADATA_KEYS'. Did you mean 'isWorkflowMetadata'?
- ❌ **error** `[[TS2305]]` at line 48:37: Module '"@wf-agent/types"' has no exported member 'SubworkflowBoundaryType'.
- ❌ **error** `[[TS2724]]` at line 61:3: '"../utils/event/index.js"' has no exported member named 'buildSubworkflowStartedEvent'. Did you mean 'buildSubgraphStartedEvent'?
- ❌ **error** `[[TS2724]]` at line 62:3: '"../utils/event/index.js"' has no exported member named 'buildSubworkflowCompletedEvent'. Did you mean 'buildSubgraphCompletedEvent'?
- ... and 15 more

#### `workflow/execution/factories/workflow-execution-builder.ts` (12 item(s))

- ❌ **error** `[[TS2353]]` at line 193:7: Object literal may only specify known properties, and 'executionType' does not exist in type 'WorkflowExecution'.
- ❌ **error** `[[TS2345]]` at line 208:7: Argument of type 'WorkflowExecutionState' is not assignable to parameter of type 'ExecutionState'.
- ❌ **error** `[[TS2551]]` at line 259:67: Property 'getWorkflowExecution' does not exist on type 'WorkflowExecutionEntity'. Did you mean 'getWorkflowExecutionData'?
- ❌ **error** `[[TS7006]]` at line 275:56: Parameter 'v' implicitly has an 'any' type.
- ❌ **error** `[[TS7006]]` at line 285:60: Parameter 'h' implicitly has an 'any' type.
- ... and 7 more

#### `workflow/message/workflow-conversation-session.ts` (11 item(s))

- ❌ **error** `[[TS2307]]` at line 21:8: Cannot find module '../../../core/messaging/conversation-session.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 22:35: Cannot find module '../../../core/registry/tool-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 24:40: Cannot find module '../../../utils/contextual-logger.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 25:48: Cannot find module '../../../resources/dynamic/prompts/fragments/available-tools.js' or its corresponding type declarations.
- ❌ **error** `[[TS2339]]` at line 74:30: Property 'getAllMessages' does not exist on type 'WorkflowConversationSession'.
- ... and 6 more

#### `core/di/container-config.ts` (9 item(s))

- ❌ **error** `[[TS2345]]` at line 292:9: Argument of type '(options: CreateCheckpointOptions, dependencies: CheckpointDependencies) => Promise<string>' is not assignable to parameter of type 'CheckpointCreator'.
- ❌ **error** `[[TS2353]]` at line 314:73: Object literal may only specify known properties, and 'threadId' does not exist in type 'WorkflowConversationSessionConfig'.
- ❌ **error** `[[TS2554]]` at line 337:13: Expected 3 arguments, but got 4.
- ❌ **error** `[[TS2353]]` at line 386:9: Object literal may only specify known properties, and 'graphRegistry' does not exist in type 'WorkflowExecutorDependencies'.
- ❌ **error** `[[TS2345]]` at line 573:49: Argument of type '{ eventManager: EventRegistry; llmCoordinator: LLMExecutionCoordinator; conversationManager: ConversationSession; interruptionManager: InterruptionState; ... 4 more ...; agentLoopExecutorFactory: AgentLoopExecutor; }' is not assignable to parameter of type 'NodeExecutionCoordinatorConfig'.
- ... and 4 more

#### `workflow/execution/factories/trigger-handler-context-factory.ts` (9 item(s))

- ❌ **error** `[[TS2305]]` at line 14:15: Module '"../../stores/workflow-execution-registry.js"' has no exported member 'ThreadRegistry'.
- ❌ **error** `[[TS2305]]` at line 16:15: Module '"../../stores/workflow-graph-registry.js"' has no exported member 'GraphRegistry'.
- ❌ **error** `[[TS2307]]` at line 22:43: Cannot find module '../coordinators/thread-state-transitor.js' or its corresponding type declarations.
- ❌ **error** `[[TS2304]]` at line 38:19: Cannot find name 'WorkflowExecutionRegistry'.
- ❌ **error** `[[TS2304]]` at line 47:19: Cannot find name 'WorkflowExecutionRegistry'.
- ... and 4 more

*... and 36 more files in this package*

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (2)

