# Type Check Report

## Type Issues Summary

- **Total**: 163
- **❌** error: 163
- **Categories**: 15
- **Files Affected**: 40
- **Packages Affected**: 4

## Breakdown by Category

- **[TS2339]**: 36 occurrence(s)
- **[TS2305]**: 27 occurrence(s)
- **[TS2304]**: 25 occurrence(s)
- **[TS2307]**: 20 occurrence(s)
- **[TS2353]**: 16 occurrence(s)
- **[TS2345]**: 10 occurrence(s)
- **[TS2551]**: 9 occurrence(s)
- **[TS7006]**: 7 occurrence(s)
- **[TS2724]**: 4 occurrence(s)
- **[TS18046]**: 2 occurrence(s)
- **[TS4111]**: 2 occurrence(s)
- **[TS2300]**: 2 occurrence(s)
- **[TS4112]**: 1 occurrence(s)
- **[TS2554]**: 1 occurrence(s)
- **[TS2739]**: 1 occurrence(s)

## Details by Package

### Package: `  Property 'threadEntity' is missing in type '{ workflowExecution` (113 issue(s))

#### `workflow/execution/utils/event/index.ts` (13 item(s))

- ❌ **error** `[[TS2724]]` at line 45:3: '"../../../../core/utils/event/builders/index.js"' has no exported member named 'buildThreadStartedEvent'. Did you mean 'buildNodeStartedEvent'?
- ❌ **error** `[[TS2724]]` at line 46:3: '"../../../../core/utils/event/builders/index.js"' has no exported member named 'buildThreadCompletedEvent'. Did you mean 'buildNodeCompletedEvent'?
- ❌ **error** `[[TS2724]]` at line 47:3: '"../../../../core/utils/event/builders/index.js"' has no exported member named 'buildThreadFailedEvent'. Did you mean 'buildNodeFailedEvent'?
- ❌ **error** `[[TS2305]]` at line 48:3: Module '"../../../../core/utils/event/builders/index.js"' has no exported member 'buildThreadPausedEvent'.
- ❌ **error** `[[TS2305]]` at line 49:3: Module '"../../../../core/utils/event/builders/index.js"' has no exported member 'buildThreadResumedEvent'.
- ... and 8 more

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

#### `workflow/execution/factories/llm-context-factory.ts` (9 item(s))

- ❌ **error** `[[TS2305]]` at line 12:15: Module '"../../stores/workflow-execution-registry.js"' has no exported member 'ThreadRegistry'.
- ❌ **error** `[[TS2305]]` at line 14:15: Module '"../../stores/workflow-graph-registry.js"' has no exported member 'GraphRegistry'.
- ❌ **error** `[[TS2304]]` at line 27:19: Cannot find name 'WorkflowExecutionRegistry'.
- ❌ **error** `[[TS2304]]` at line 30:19: Cannot find name 'WorkflowGraphRegistry'.
- ❌ **error** `[[TS2304]]` at line 37:20: Cannot find name 'WorkflowExecutionRegistry'.
- ... and 4 more

#### `workflow/execution/coordinators/node-execution-coordinator.ts` (9 item(s))

- ❌ **error** `[[TS2551]]` at line 362:56: Property 'getWorkflowExecution' does not exist on type 'WorkflowExecutionEntity'. Did you mean 'getWorkflowExecutionData'?
- ❌ **error** `[[TS2345]]` at line 387:11: Argument of type '{ workflowExecution: any; workflowExecutionEntity: WorkflowExecutionEntity; node: Node; result: NodeExecutionResult; checkpointDependencies: CheckpointDependencies | undefined; }' is not assignable to parameter of type 'HookExecutionContext'.
- ❌ **error** `[[TS2551]]` at line 388:56: Property 'getWorkflowExecution' does not exist on type 'WorkflowExecutionEntity'. Did you mean 'getWorkflowExecutionData'?
- ❌ **error** `[[TS2353]]` at line 413:17: Object literal may only specify known properties, and 'executionId' does not exist in type 'CreateCheckpointOptions'.
- ❌ **error** `[[TS2339]]` at line 433:60: Property 'buildEvent' does not exist on type 'WorkflowExecutionEntity'.
- ... and 4 more

*... and 20 more files in this package*

### Package: `    Type '{ threadRegistry` (21 issue(s))

#### `core/di/container-config.ts` (4 item(s))

- ❌ **error** `[[TS2739]]` at line 613:13: Type 'WorkflowExecutionBuilder' is missing the following properties from type 'ThreadBuilder': threadTemplates, getGraphRegistry, buildFromPreprocessedGraph
- ❌ **error** `[[TS2345]]` at line 640:9: Argument of type 'WorkflowExecutionBuilder' is not assignable to parameter of type '{ build: (subgraphId: string, options: { input: Record<string, unknown>; }) => Promise<ThreadBuildResultSimple>; }'.
- ❌ **error** `[[TS2353]]` at line 754:15: Object literal may only specify known properties, and 'threadRegistry' does not exist in type 'CheckpointDependencies'.
- ❌ **error** `[[TS2353]]` at line 764:13: Object literal may only specify known properties, and 'threadRegistry' does not exist in type 'CheckpointDependencies'.

#### `workflow/entities/index.ts` (3 item(s))

- ❌ **error** `[[TS2307]]` at line 12:27: Cannot find module '../../graph/entities/graph-data.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 13:39: Cannot find module '../../graph/entities/preprocessed-graph-data.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 14:30: Cannot find module '../../graph/entities/thread-entity.js' or its corresponding type declarations.

#### `workflow/checkpoint/checkpoint-coordinator.ts` (3 item(s))

- ❌ **error** `[[TS2339]]` at line 421:31: Property 'setParentThreadId' does not exist on type 'WorkflowExecutionEntity'.
- ❌ **error** `[[TS2339]]` at line 456:47: Property 'setParentThreadId' does not exist on type 'WorkflowExecutionEntity'.
- ❌ **error** `[[TS2339]]` at line 460:35: Property 'registerChildThread' does not exist on type 'WorkflowExecutionEntity'.

#### `resources/predefined/tools/builtin/workflow/execute-workflow/handler.ts` (3 item(s))

- ❌ **error** `[[TS2307]]` at line 10:8: Cannot find module '../../../../../../graph/execution/types/workflow-tool.types.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 15:8: Cannot find module '../../../../../../graph/execution/types/triggered-subworkflow.types.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 19:50: Cannot find module '../../../../../../graph/execution/handlers/triggered-subworkflow-handler.js' or its corresponding type declarations.

#### `resources/predefined/tools/builtin/workflow/cancel-workflow/handler.ts` (2 item(s))

- ❌ **error** `[[TS2307]]` at line 9:8: Cannot find module '../../../../../../graph/execution/types/workflow-tool.types.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 13:50: Cannot find module '../../../../../../graph/execution/handlers/triggered-subworkflow-handler.js' or its corresponding type declarations.

*... and 4 more files in this package*

### Package: `  Type '{ threadRegistry` (5 issue(s))

#### `workflow/execution/coordinators/node-execution-coordinator.ts` (5 item(s))

- ❌ **error** `[[TS2353]]` at line 165:7: Object literal may only specify known properties, and 'workflowExecutionRegistry' does not exist in type 'NodeHandlerContextFactoryConfig'.
- ❌ **error** `[[TS2353]]` at line 218:13: Object literal may only specify known properties, and 'executionId' does not exist in type 'CreateCheckpointOptions'.
- ❌ **error** `[[TS2339]]` at line 316:56: Property 'buildEvent' does not exist on type 'WorkflowExecutionEntity'.
- ❌ **error** `[[TS2353]]` at line 335:17: Object literal may only specify known properties, and 'executionId' does not exist in type 'CreateCheckpointOptions'.
- ❌ **error** `[[TS2345]]` at line 361:11: Argument of type '{ workflowExecution: any; workflowExecutionEntity: WorkflowExecutionEntity; node: Node; checkpointDependencies: CheckpointDependencies | undefined; }' is not assignable to parameter of type 'HookExecutionContext'.

### Package: `    Property 'workflowExecutionId' is missing in type '{ threadId` (3 issue(s))

#### `core/di/container-config.ts` (3 item(s))

- ❌ **error** `[[TS2353]]` at line 314:73: Object literal may only specify known properties, and 'threadId' does not exist in type 'WorkflowConversationSessionConfig'.
- ❌ **error** `[[TS2554]]` at line 337:13: Expected 3 arguments, but got 4.
- ❌ **error** `[[TS2345]]` at line 573:49: Argument of type '{ eventManager: EventRegistry; llmCoordinator: LLMExecutionCoordinator; conversationManager: ConversationSession; interruptionManager: InterruptionState; ... 4 more ...; agentLoopExecutorFactory: AgentLoopExecutor; }' is not assignable to parameter of type 'NodeExecutionCoordinatorConfig'.

