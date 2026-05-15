# Type Check Report

## Type Issues Summary

- **Total**: 66
- **⚠️** warning: 66
- **Categories**: 44
- **Files Affected**: 38

## Breakdown by Category

- **Unexpected any. Specify a different**: 13 occurrence(s)
- **'WorkflowToolExecutionContext' is defined but never**: 3 occurrence(s)
- **'error' is defined but never**: 3 occurrence(s)
- **'errorMessage' is defined but never**: 2 occurrence(s)
- **Unexpected console statement**: 2 occurrence(s)
- **'now' is defined but never**: 2 occurrence(s)
- **'logger' is assigned a value**: 2 occurrence(s)
- **'IgnoreMode' is defined but never**: 2 occurrence(s)
- **'subgraphConfig' is assigned a value**: 2 occurrence(s)
- **'ALL_EVENT_TYPES' is assigned a value**: 1 occurrence(s)
- **'executionId' is assigned a value**: 1 occurrence(s)
- **'QueryWorkflowStatusParams' is defined but never**: 1 occurrence(s)
- **'AgentStreamEventType' is defined but never**: 1 occurrence(s)
- **'searchChunk' is assigned a value**: 1 occurrence(s)
- **'EventEmitterOptions' is defined but never**: 1 occurrence(s)
- **'toolCallName' is assigned a value**: 1 occurrence(s)
- **'executionTime' is assigned a value**: 1 occurrence(s)
- **'CancelWorkflowParams' is defined but never**: 1 occurrence(s)
- **'options' is defined but never**: 1 occurrence(s)
- **'failureCount' is assigned a value**: 1 occurrence(s)
- **'ID' is defined but never**: 1 occurrence(s)
- **'currentMessages' is assigned a value**: 1 occurrence(s)
- **'timeout' is assigned a value**: 1 occurrence(s)
- **'NamedMessageContext' is defined but never**: 1 occurrence(s)
- **'nodeId' is defined but never**: 1 occurrence(s)
- **'ExecutionError' is defined but never**: 1 occurrence(s)
- **'MetricFilter' is defined but never**: 1 occurrence(s)
- **'compressionPrompt' is defined but never**: 1 occurrence(s)
- **'searchEndIndex' is assigned a value**: 1 occurrence(s)
- **'MetricQueryResult' is defined but never**: 1 occurrence(s)
- **'TCheckpoint' is defined but never**: 1 occurrence(s)
- **'VariableManager' is defined but never**: 1 occurrence(s)
- **'workflowExecution' is assigned a value**: 1 occurrence(s)
- **'ExecuteWorkflowParams' is defined but never**: 1 occurrence(s)
- **'WorkflowNode' is defined but never**: 1 occurrence(s)
- **'recordHistory' is assigned a value**: 1 occurrence(s)
- **'SUBGRAPH_METADATA_KEYS' is defined but never**: 1 occurrence(s)
- **'resolvePath' is defined but never**: 1 occurrence(s)
- **'WorkflowExecution' is defined but never**: 1 occurrence(s)
- **'generateId' is defined but never**: 1 occurrence(s)
- **'toolCallArgs' is assigned a value**: 1 occurrence(s)
- **'sourceContext' is assigned a value**: 1 occurrence(s)
- **'getErrorOrNew' is defined but never**: 1 occurrence(s)
- **'targetContext' is assigned a value**: 1 occurrence(s)

## Details by File

### `D:\项目\agent\wf-agent\sdk\core\registry\event-registry.ts` (5 item(s))

- ⚠️ **warning** at line 17:10: 'ExecutionError' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 18:10: 'generateId' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 19:10: 'now' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 19:15: 'getErrorOrNew' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 22:38: 'EventEmitterOptions' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\workflow\builders\workflow-builder.ts` (4 item(s))

- ⚠️ **warning** at line 443:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 459:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 464:63: Unexpected any. Specify a different type
- ⚠️ **warning** at line 473:39: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\checkpoint\types.ts` (3 item(s))

- ⚠️ **warning** at line 10:43: 'TCheckpoint' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:78: Unexpected any. Specify a different type
- ⚠️ **warning** at line 10:83: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\context-processor-handler.ts` (3 item(s))

- ⚠️ **warning** at line 156:9: 'sourceContext' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 159:9: 'targetContext' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 188:9: 'currentMessages' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\workflow-execution-coordinator.ts` (3 item(s))

- ⚠️ **warning** at line 7:40: 'WorkflowNode' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 20:7: 'logger' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 52:11: 'executionId' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\workflow-collector.ts` (2 item(s))

- ⚠️ **warning** at line 145:11: 'failureCount' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 248:18: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\core\metrics\resource-collector.ts` (2 item(s))

- ⚠️ **warning** at line 12:38: 'MetricFilter' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 12:52: 'MetricQueryResult' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\workflow\query-workflow-status\handler.ts` (2 item(s))

- ⚠️ **warning** at line 7:3: 'QueryWorkflowStatusParams' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:3: 'WorkflowToolExecutionContext' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\interaction\ask-followup-question\handler.ts` (2 item(s))

- ⚠️ **warning** at line 164:13: 'executionTime' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 226:75: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\trigger-handlers\execute-triggered-subgraph-handler.ts` (2 item(s))

- ⚠️ **warning** at line 123:11: 'timeout' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 124:11: 'recordHistory' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\resources\events\event-resource-api.ts` (2 item(s))

- ⚠️ **warning** at line 26:7: 'ALL_EVENT_TYPES' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 274:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\workflow\execute-workflow\handler.ts` (2 item(s))

- ⚠️ **warning** at line 7:3: 'WorkflowToolExecutionContext' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 8:3: 'ExecuteWorkflowParams' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\apply-diff\utils\apply.ts` (2 item(s))

- ⚠️ **warning** at line 74:9: 'searchChunk' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 78:9: 'searchEndIndex' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\llm-handler.ts` (2 item(s))

- ⚠️ **warning** at line 13:43: 'NamedMessageContext' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 26:7: 'logger' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\workflow\cancel-workflow\handler.ts` (2 item(s))

- ⚠️ **warning** at line 7:3: 'CancelWorkflowParams' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:3: 'WorkflowToolExecutionContext' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\variable-handler.ts` (2 item(s))

- ⚠️ **warning** at line 6:48: 'WorkflowExecution' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:10: 'resolvePath' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\agent\checkpoint\checkpoint-state-manager.ts` (2 item(s))

- ⚠️ **warning** at line 41:58: Unexpected any. Specify a different type
- ⚠️ **warning** at line 78:33: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\subgraph-handler.ts` (2 item(s))

- ⚠️ **warning** at line 175:9: 'subgraphConfig' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 288:9: 'subgraphConfig' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\node-collector.ts` (2 item(s))

- ⚠️ **warning** at line 188:18: 'error' is defined but never used
- ⚠️ **warning** at line 211:18: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\llm-execution-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 599:11: 'toolCallName' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 600:11: 'toolCallArgs' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\list-files\handler.ts` (1 item(s))

- ⚠️ **warning** at line 10:28: 'IgnoreMode' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\checkpoint\checkpoint-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 33:10: 'VariableManager' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\loop-end-handler.ts` (1 item(s))

- ⚠️ **warning** at line 39:9: 'workflowExecution' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\template-collector.ts` (1 item(s))

- ⚠️ **warning** at line 308:43: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\metrics\error-collector.ts` (1 item(s))

- ⚠️ **warning** at line 39:5: 'errorMessage' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\hook-handlers\hook-handler.ts` (1 item(s))

- ⚠️ **warning** at line 211:27: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\triggered-subworkflow-handler.ts` (1 item(s))

- ⚠️ **warning** at line 231:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\core\utils\event\builders\custom-events.ts` (1 item(s))

- ⚠️ **warning** at line 7:82: 'AgentStreamEventType' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\types.ts` (1 item(s))

- ⚠️ **warning** at line 7:10: 'now' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\state-managers\tool-failure-protection-types.ts` (1 item(s))

- ⚠️ **warning** at line 7:15: 'ID' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\event-collector.ts` (1 item(s))

- ⚠️ **warning** at line 363:42: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\metrics\tool-collector.ts` (1 item(s))

- ⚠️ **warning** at line 102:5: 'errorMessage' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\metrics-registry.ts` (1 item(s))

- ⚠️ **warning** at line 155:24: 'options' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\utils\interruption\interruption-handler.ts` (1 item(s))

- ⚠️ **warning** at line 92:33: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\checkpoint\checkpoint-state-manager.ts` (1 item(s))

- ⚠️ **warning** at line 136:58: 'nodeId' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\workflow\context-compression.ts` (1 item(s))

- ⚠️ **warning** at line 49:50: 'compressionPrompt' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\read-file\handler.ts` (1 item(s))

- ⚠️ **warning** at line 10:28: 'IgnoreMode' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\builder\workflow-graph-builder.ts` (1 item(s))

- ⚠️ **warning** at line 27:10: 'SUBGRAPH_METADATA_KEYS' is defined but never used. Allowed unused vars must match /^_/u

