# Type Check Report

## Type Issues Summary

- **Total**: 166
- **❌** error: 166
- **Categories**: 19
- **Files Affected**: 46
- **Packages Affected**: 2

## Breakdown by Category

- **[TS2339]**: 54 occurrence(s)
- **[TS2307]**: 25 occurrence(s)
- **[TS2304]**: 19 occurrence(s)
- **[TS2305]**: 18 occurrence(s)
- **[TS2353]**: 14 occurrence(s)
- **[TS18004]**: 7 occurrence(s)
- **[TS2345]**: 7 occurrence(s)
- **[TS2724]**: 4 occurrence(s)
- **[TS2551]**: 3 occurrence(s)
- **[TS7006]**: 2 occurrence(s)
- **[TS4111]**: 2 occurrence(s)
- **[TS2300]**: 2 occurrence(s)
- **[TS18046]**: 2 occurrence(s)
- **[TS2561]**: 2 occurrence(s)
- **[TS2564]**: 1 occurrence(s)
- **[TS4112]**: 1 occurrence(s)
- **[TS2367]**: 1 occurrence(s)
- **[TS2554]**: 1 occurrence(s)
- **[TS2552]**: 1 occurrence(s)

## Details by Package

### Package: `  Property 'threadEntity' is missing in type '{ workflowExecution` (124 issue(s))

#### `workflow/execution/handlers/triggered-subworkflow-handler.ts` (13 item(s))

- ❌ **error** `[[TS2564]]` at line 77:11: Property 'workflowExecutionRegistry' has no initializer and is not definitely assigned in the constructor.
- ❌ **error** `[[TS2339]]` at line 129:10: Property 'threadRegistry' does not exist on type 'TriggeredSubworkflowHandler'.
- ❌ **error** `[[TS2304]]` at line 129:27: Cannot find name 'threadRegistry'.
- ❌ **error** `[[TS2339]]` at line 172:10: Property 'threadRegistry' does not exist on type 'TriggeredSubworkflowHandler'.
- ❌ **error** `[[TS2339]]` at line 177:27: Property 'registerChildThread' does not exist on type 'WorkflowExecutionEntity'.
- ... and 8 more

#### `workflow/execution/utils/event/index.ts` (13 item(s))

- ❌ **error** `[[TS2724]]` at line 45:3: '"../../../../core/utils/event/builders/index.js"' has no exported member named 'buildThreadStartedEvent'. Did you mean 'buildNodeStartedEvent'?
- ❌ **error** `[[TS2724]]` at line 46:3: '"../../../../core/utils/event/builders/index.js"' has no exported member named 'buildThreadCompletedEvent'. Did you mean 'buildNodeCompletedEvent'?
- ❌ **error** `[[TS2724]]` at line 47:3: '"../../../../core/utils/event/builders/index.js"' has no exported member named 'buildThreadFailedEvent'. Did you mean 'buildNodeFailedEvent'?
- ❌ **error** `[[TS2305]]` at line 48:3: Module '"../../../../core/utils/event/builders/index.js"' has no exported member 'buildThreadPausedEvent'.
- ❌ **error** `[[TS2305]]` at line 49:3: Module '"../../../../core/utils/event/builders/index.js"' has no exported member 'buildThreadResumedEvent'.
- ... and 8 more

#### `workflow/message/workflow-conversation-session.ts` (11 item(s))

- ❌ **error** `[[TS2307]]` at line 21:8: Cannot find module '../../../core/messaging/conversation-session.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 22:35: Cannot find module '../../../core/registry/tool-registry.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 24:40: Cannot find module '../../../utils/contextual-logger.js' or its corresponding type declarations.
- ❌ **error** `[[TS2307]]` at line 25:48: Cannot find module '../../../resources/dynamic/prompts/fragments/available-tools.js' or its corresponding type declarations.
- ❌ **error** `[[TS2339]]` at line 74:30: Property 'getAllMessages' does not exist on type 'WorkflowConversationSession'.
- ... and 6 more

#### `workflow/execution/factories/workflow-execution-builder.ts` (10 item(s))

- ❌ **error** `[[TS2353]]` at line 193:7: Object literal may only specify known properties, and 'executionType' does not exist in type 'WorkflowExecution'.
- ❌ **error** `[[TS2345]]` at line 208:7: Argument of type 'WorkflowExecutionState' is not assignable to parameter of type 'ExecutionState'.
- ❌ **error** `[[TS2353]]` at line 289:9: Object literal may only specify known properties, and 'parentExecutionId' does not exist in type 'TriggeredSubworkflowContext'.
- ❌ **error** `[[TS2345]]` at line 301:7: Argument of type 'WorkflowExecutionState' is not assignable to parameter of type 'ExecutionState'.
- ❌ **error** `[[TS2353]]` at line 307:7: Object literal may only specify known properties, and 'workflowExecutionId' does not exist in type 'ConversationSessionConfig'.
- ... and 5 more

#### `workflow/execution/handlers/human-relay-handler.ts` (7 item(s))

- ❌ **error** `[[TS2339]]` at line 78:22: Property 'workflowExecutionEntity' does not exist on type 'HumanRelayTask'.
- ❌ **error** `[[TS2339]]` at line 98:20: Property 'workflowExecutionEntity' does not exist on type 'HumanRelayTask'.
- ❌ **error** `[[TS2339]]` at line 129:22: Property 'workflowExecutionEntity' does not exist on type 'HumanRelayTask'.
- ❌ **error** `[[TS2339]]` at line 152:22: Property 'workflowExecutionEntity' does not exist on type 'HumanRelayTask'.
- ❌ **error** `[[TS2339]]` at line 175:22: Property 'workflowExecutionEntity' does not exist on type 'HumanRelayTask'.
- ... and 2 more

*... and 28 more files in this package*

### Package: `    Property 'workflowExecutionId' is missing in type '{ threadId` (22 issue(s))

#### `workflow/execution/coordinators/node-execution-coordinator.ts` (12 item(s))

- ❌ **error** `[[TS2353]]` at line 165:7: Object literal may only specify known properties, and 'workflowExecutionRegistry' does not exist in type 'NodeHandlerContextFactoryConfig'.
- ❌ **error** `[[TS18004]]` at line 200:44: No value exists in scope for the shorthand property 'executionId'. Either declare one or provide an initializer.
- ❌ **error** `[[TS18004]]` at line 203:97: No value exists in scope for the shorthand property 'executionId'. Either declare one or provide an initializer.
- ❌ **error** `[[TS2304]]` at line 207:73: Cannot find name 'executionId'.
- ❌ **error** `[[TS18004]]` at line 209:76: No value exists in scope for the shorthand property 'executionId'. Either declare one or provide an initializer.
- ... and 7 more

#### `core/di/container-config.ts` (4 item(s))

- ❌ **error** `[[TS2353]]` at line 314:73: Object literal may only specify known properties, and 'threadId' does not exist in type 'WorkflowConversationSessionConfig'.
- ❌ **error** `[[TS2554]]` at line 337:13: Expected 3 arguments, but got 4.
- ❌ **error** `[[TS2561]]` at line 611:13: Object literal may only specify known properties, but 'workflowGraphRegistry' does not exist in type 'TriggerHandlerContextFactoryConfig'. Did you mean to write 'workflowRegistry'?
- ❌ **error** `[[TS2345]]` at line 640:9: Argument of type 'WorkflowExecutionBuilder' is not assignable to parameter of type '{ build: (subgraphId: string, options: { input: Record<string, unknown>; }) => Promise<ThreadBuildResultSimple>; }'.

#### `workflow/checkpoint/checkpoint-coordinator.ts` (3 item(s))

- ❌ **error** `[[TS2339]]` at line 421:31: Property 'setParentThreadId' does not exist on type 'WorkflowExecutionEntity'.
- ❌ **error** `[[TS2339]]` at line 456:47: Property 'setParentThreadId' does not exist on type 'WorkflowExecutionEntity'.
- ❌ **error** `[[TS2339]]` at line 460:35: Property 'registerChildThread' does not exist on type 'WorkflowExecutionEntity'.

#### `core/serialization/entities/task-serializer.ts` (2 item(s))

- ❌ **error** `[[TS2339]]` at line 114:18: Property 'id' does not exist on type 'WorkflowExecutionResult'.
- ❌ **error** `[[TS2353]]` at line 134:7: Object literal may only specify known properties, and 'id' does not exist in type 'WorkflowExecutionResult'.

#### `workflow/execution/coordinators/llm-execution-coordinator.ts` (1 item(s))

- ❌ **error** `[[TS2339]]` at line 507:56: Property 'threadRegistry' does not exist on type 'ToolApprovalContext'.

