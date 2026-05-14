# Type Check Report

## Type Issues Summary

- **Total**: 157
- **❌** error: 1
- **⚠️** warning: 156
- **Categories**: 60
- **Files Affected**: 63

## Breakdown by Category

- **Unexpected any. Specify a different**: 82 occurrence(s)
- **'WorkflowToolExecutionContext' is defined but never**: 3 occurrence(s)
- **'error' is defined but never**: 3 occurrence(s)
- **'now' is defined but never**: 3 occurrence(s)
- **'generateId' is defined but never**: 2 occurrence(s)
- **'CheckpointMetadata' is defined but never**: 2 occurrence(s)
- **'NamedMessageContext' is defined but never**: 2 occurrence(s)
- **'logger' is assigned a value**: 2 occurrence(s)
- **'errorMessage' is defined but never**: 2 occurrence(s)
- **Unexpected console statement**: 2 occurrence(s)
- **'subgraphConfig' is assigned a value**: 2 occurrence(s)
- **'IgnoreMode' is defined but never**: 2 occurrence(s)
- **'AgentStreamEventType' is defined but never**: 2 occurrence(s)
- **'CleanupPolicy' is defined but never**: 2 occurrence(s)
- **'toolCallArgs' is assigned a value**: 1 occurrence(s)
- **'ExecutionError' is defined but never**: 1 occurrence(s)
- **'LLMMessage' is defined but never**: 1 occurrence(s)
- **'oldController' is assigned a value**: 1 occurrence(s)
- **'SUBGRAPH_METADATA_KEYS' is defined but never**: 1 occurrence(s)
- **'MetricQueryResult' is defined but never**: 1 occurrence(s)
- **'timeout' is assigned a value**: 1 occurrence(s)
- **'DeltaRestoreResult' is defined but never**: 1 occurrence(s)
- **'VariableManager' is defined but never**: 1 occurrence(s)
- **'ExecuteWorkflowParams' is defined but never**: 1 occurrence(s)
- **'compressionPrompt' is defined but never**: 1 occurrence(s)
- **'WorkflowExecution' is defined but never**: 1 occurrence(s)
- **'failureCount' is assigned a value**: 1 occurrence(s)
- **'StaticNodeType' is defined but never**: 1 occurrence(s)
- **'WorkflowExecutionInterruptedException' is defined but never**: 1 occurrence(s)
- **'searchChunk' is assigned a value**: 1 occurrence(s)
- **The value assigned to 'targetMap'**: 1 occurrence(s)
- **'DEFAULT_DELTA_STORAGE_CONFIG' is defined but never**: 1 occurrence(s)
- **'CancelWorkflowParams' is defined but never**: 1 occurrence(s)
- **'sourceContext' is assigned a value**: 1 occurrence(s)
- **'ID' is defined but never**: 1 occurrence(s)
- **'StaticNode' is defined but never**: 1 occurrence(s)
- **'nodeId' is defined but never**: 1 occurrence(s)
- **'toolCallName' is assigned a value**: 1 occurrence(s)
- **'CleanupResult' is defined but never**: 1 occurrence(s)
- **'searchEndIndex' is assigned a value**: 1 occurrence(s)
- **'iteration' is defined but never**: 1 occurrence(s)
- **'MetricFilter' is defined but never**: 1 occurrence(s)
- **'executionTime' is assigned a value**: 1 occurrence(s)
- **'ALL_EVENT_TYPES' is assigned a value**: 1 occurrence(s)
- **'targetContext' is assigned a value**: 1 occurrence(s)
- **'recordHistory' is assigned a value**: 1 occurrence(s)
- **'CheckpointOptions' is defined but never**: 1 occurrence(s)
- **'EventEmitterOptions' is defined but never**: 1 occurrence(s)
- **'formatLabels' is assigned a value**: 1 occurrence(s)
- **'options' is defined but never**: 1 occurrence(s)
- **'buildCheckpointDeletedEvent' is defined but never**: 1 occurrence(s)
- **'workflowExecution' is assigned a value**: 1 occurrence(s)
- **'nodes' is defined but never**: 1 occurrence(s)
- **'edges' is defined but never**: 1 occurrence(s)
- **'WorkflowNode' is defined but never**: 1 occurrence(s)
- **'resolvePath' is defined but never**: 1 occurrence(s)
- **'TCheckpoint' is defined but never**: 1 occurrence(s)
- **'QueryWorkflowStatusParams' is defined but never**: 1 occurrence(s)
- **'getErrorOrNew' is defined but never**: 1 occurrence(s)
- **'currentMessages' is assigned a value**: 1 occurrence(s)

## Details by File

### `D:\项目\agent\wf-agent\sdk\core\checkpoint\base-diff-calculator.ts` (12 item(s))

- ⚠️ **warning** at line 25:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 28:29: Unexpected any. Specify a different type
- ⚠️ **warning** at line 28:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 29:41: Unexpected any. Specify a different type
- ⚠️ **warning** at line 29:50: Unexpected any. Specify a different type
- ⚠️ **warning** at line 68:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 70:35: Unexpected any. Specify a different type
- ⚠️ **warning** at line 70:44: Unexpected any. Specify a different type
- ⚠️ **warning** at line 77:27: Unexpected any. Specify a different type
- ⚠️ **warning** at line 80:20: Unexpected any. Specify a different type
- ⚠️ **warning** at line 93:24: Unexpected any. Specify a different type
- ⚠️ **warning** at line 93:32: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\subgraph-handler.ts` (11 item(s))

- ⚠️ **warning** at line 135:49: Unexpected any. Specify a different type
- ⚠️ **warning** at line 144:9: 'subgraphConfig' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 164:82: Unexpected any. Specify a different type
- ⚠️ **warning** at line 174:37: Unexpected any. Specify a different type
- ⚠️ **warning** at line 195:36: Unexpected any. Specify a different type
- ⚠️ **warning** at line 198:34: Unexpected any. Specify a different type
- ⚠️ **warning** at line 223:12: Unexpected any. Specify a different type
- ⚠️ **warning** at line 247:49: Unexpected any. Specify a different type
- ⚠️ **warning** at line 256:9: 'subgraphConfig' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 283:82: Unexpected any. Specify a different type
- ⚠️ **warning** at line 322:14: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\checkpoint\base-checkpoint-state-manager.ts` (8 item(s))

- ⚠️ **warning** at line 8:31: 'CheckpointMetadata' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 26:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 26:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 66:54: Unexpected any. Specify a different type
- ⚠️ **warning** at line 90:53: Unexpected any. Specify a different type
- ⚠️ **warning** at line 135:54: Unexpected any. Specify a different type
- ⚠️ **warning** at line 177:17: Unexpected any. Specify a different type
- ⚠️ **warning** at line 200:30: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\checkpoint\types.ts` (6 item(s))

- ⚠️ **warning** at line 5:31: 'CheckpointMetadata' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:43: 'TCheckpoint' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:78: Unexpected any. Specify a different type
- ⚠️ **warning** at line 10:83: Unexpected any. Specify a different type
- ⚠️ **warning** at line 31:76: Unexpected any. Specify a different type
- ⚠️ **warning** at line 31:81: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\builder\workflow-graph-builder.ts` (6 item(s))

- ⚠️ **warning** at line 27:10: 'SUBGRAPH_METADATA_KEYS' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 383:44: Unexpected any. Specify a different type
- ⚠️ **warning** at line 384:36: Unexpected any. Specify a different type
- ⚠️ **warning** at line 401:70: Unexpected any. Specify a different type
- ⚠️ **warning** at line 417:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 418:34: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\context-processor-handler.ts` (6 item(s))

- ⚠️ **warning** at line 102:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 156:9: 'sourceContext' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 159:9: 'targetContext' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 188:9: 'currentMessages' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 222:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 224:59: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\checkpoint\checkpoint-state-manager.ts` (5 item(s))

- ⚠️ **warning** at line 9:3: 'CleanupPolicy' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:3: 'CleanupResult' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 18:3: 'buildCheckpointDeletedEvent' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 39:25: Unexpected any. Specify a different type
- ⚠️ **warning** at line 139:58: 'nodeId' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\registry\event-registry.ts` (5 item(s))

- ⚠️ **warning** at line 17:10: 'ExecutionError' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 18:10: 'generateId' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 19:10: 'now' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 19:15: 'getErrorOrNew' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 22:38: 'EventEmitterOptions' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\core\sdk-instance.ts` (5 item(s))

- ⚠️ **warning** at line 480:40: Unexpected any. Specify a different type
- ⚠️ **warning** at line 482:69: Unexpected any. Specify a different type
- ⚠️ **warning** at line 483:56: Unexpected any. Specify a different type
- ⚠️ **warning** at line 484:51: Unexpected any. Specify a different type
- ⚠️ **warning** at line 485:61: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\node-execution-coordinator.ts` (5 item(s))

- ⚠️ **warning** at line 363:87: Unexpected any. Specify a different type
- ⚠️ **warning** at line 400:27: Unexpected any. Specify a different type
- ⚠️ **warning** at line 440:27: Unexpected any. Specify a different type
- ⚠️ **warning** at line 455:87: Unexpected any. Specify a different type
- ⚠️ **warning** at line 558:22: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\agent\checkpoint\checkpoint-state-manager.ts` (4 item(s))

- ⚠️ **warning** at line 8:15: 'CleanupPolicy' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 15:7: 'logger' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 43:58: Unexpected any. Specify a different type
- ⚠️ **warning** at line 80:33: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\workflow\builders\workflow-builder.ts` (4 item(s))

- ⚠️ **warning** at line 456:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 472:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 477:63: Unexpected any. Specify a different type
- ⚠️ **warning** at line 486:39: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\llm-handler.ts` (4 item(s))

- ⚠️ **warning** at line 13:43: 'NamedMessageContext' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 26:7: 'logger' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 66:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 155:46: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\checkpoint\base-delta-restorer.ts` (3 item(s))

- ⚠️ **warning** at line 27:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 27:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 104:43: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\checkpoint\base-checkpoint-coordinator.ts` (3 item(s))

- ⚠️ **warning** at line 16:61: 'DeltaRestoreResult' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 40:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 40:43: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\agent\checkpoint\checkpoint-coordinator.ts` (3 item(s))

- ⚠️ **warning** at line 19:10: 'DEFAULT_DELTA_STORAGE_CONFIG' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 20:10: 'generateId' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 21:10: 'now' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\node-metrics-collector.ts` (2 item(s))

- ⚠️ **warning** at line 186:18: 'error' is defined but never used
- ⚠️ **warning** at line 209:18: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\api\shared\resources\events\event-resource-api.ts` (2 item(s))

- ⚠️ **warning** at line 26:7: 'ALL_EVENT_TYPES' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 274:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\core\coordinators\followup-question-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 97:50: Unexpected any. Specify a different type
- ⚠️ **warning** at line 116:50: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\continue-from-trigger-handler.ts` (2 item(s))

- ⚠️ **warning** at line 73:44: Unexpected any. Specify a different type
- ⚠️ **warning** at line 74:54: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\workflow\query-workflow-status\handler.ts` (2 item(s))

- ⚠️ **warning** at line 7:3: 'QueryWorkflowStatusParams' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:3: 'WorkflowToolExecutionContext' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\config\config-transformer.ts` (2 item(s))

- ⚠️ **warning** at line 216:36: 'nodes' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 216:57: 'edges' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\resource-collector.ts` (2 item(s))

- ⚠️ **warning** at line 12:38: 'MetricFilter' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 12:52: 'MetricQueryResult' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\registry\event-emitter.ts` (2 item(s))

- ⚠️ **warning** at line 74:50: Unexpected any. Specify a different type
- ⚠️ **warning** at line 347:27: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\apply-diff\utils\apply.ts` (2 item(s))

- ⚠️ **warning** at line 74:9: 'searchChunk' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 78:9: 'searchEndIndex' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\coordinators\llm-execution-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 140:19: Unexpected any. Specify a different type
- ⚠️ **warning** at line 249:39: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\interaction\ask-followup-question\handler.ts` (2 item(s))

- ⚠️ **warning** at line 164:13: 'executionTime' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 226:75: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\workflow\cancel-workflow\handler.ts` (2 item(s))

- ⚠️ **warning** at line 7:3: 'CancelWorkflowParams' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:3: 'WorkflowToolExecutionContext' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\variable-handler.ts` (2 item(s))

- ⚠️ **warning** at line 6:48: 'WorkflowExecution' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:10: 'resolvePath' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\workflow-metrics-collector.ts` (2 item(s))

- ⚠️ **warning** at line 143:11: 'failureCount' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 246:18: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\agent-loop-handler.ts` (2 item(s))

- ⚠️ **warning** at line 9:80: 'NamedMessageContext' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 77:42: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\utils\checkpoint-restoration.ts` (2 item(s))

- ⚠️ **warning** at line 116:83: Unexpected any. Specify a different type
- ⚠️ **warning** at line 122:37: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\checkpoint\checkpoint-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 33:10: 'VariableManager' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 419:35: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\llm-execution-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 613:11: 'toolCallName' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 614:11: 'toolCallArgs' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\trigger-handlers\execute-triggered-subgraph-handler.ts` (2 item(s))

- ⚠️ **warning** at line 123:11: 'timeout' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 124:11: 'recordHistory' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\workflow\execute-workflow\handler.ts` (2 item(s))

- ⚠️ **warning** at line 7:3: 'WorkflowToolExecutionContext' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 8:3: 'ExecuteWorkflowParams' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\entities\workflow-graph.ts` (1 item(s))

- ⚠️ **warning** at line 18:3: 'StaticNodeType' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\read-file\handler.ts` (1 item(s))

- ⚠️ **warning** at line 10:28: 'IgnoreMode' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\factories\workflow-execution-builder.ts` (1 item(s))

- ⚠️ **warning** at line 270:27: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\metrics\error-collector.ts` (1 item(s))

- ⚠️ **warning** at line 38:5: 'errorMessage' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\tool-collector.ts` (1 item(s))

- ⚠️ **warning** at line 101:5: 'errorMessage' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\list-files\handler.ts` (1 item(s))

- ⚠️ **warning** at line 10:28: 'IgnoreMode' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\workflow-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 7:40: 'WorkflowNode' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\base-collector.ts` (1 item(s))

- ⚠️ **warning** at line 396:27: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\messaging\message-context-utils.ts` (1 item(s))

- ⚠️ **warning** at line 7:60: 'LLMMessage' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\state-managers\variable-manager.ts` (1 item(s))

- ❌ **error** at line 323:7: The value assigned to 'targetMap' is not used in subsequent statements

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\tool-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 26:10: 'AgentStreamEventType' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\di\service-identifiers.ts` (1 item(s))

- ⚠️ **warning** at line 276:49: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\resources\metrics\metrics-resource-api.ts` (1 item(s))

- ⚠️ **warning** at line 234:11: 'formatLabels' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\utils\event\builders\custom-events.ts` (1 item(s))

- ⚠️ **warning** at line 7:82: 'AgentStreamEventType' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\metrics\agent-metrics-collector.ts` (1 item(s))

- ⚠️ **warning** at line 81:38: 'iteration' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\triggered-subworkflow-handler.ts` (1 item(s))

- ⚠️ **warning** at line 231:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\core\executors\tool-call-executor.ts` (1 item(s))

- ⚠️ **warning** at line 32:10: 'WorkflowExecutionInterruptedException' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\agent\resources\checkpoint-resource-api.ts` (1 item(s))

- ⚠️ **warning** at line 18:8: 'CheckpointOptions' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\executors\script-executor.ts` (1 item(s))

- ⚠️ **warning** at line 23:33: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\metrics\metrics-registry.ts` (1 item(s))

- ⚠️ **warning** at line 77:24: 'options' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\loop-end-handler.ts` (1 item(s))

- ⚠️ **warning** at line 39:9: 'workflowExecution' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\start-from-trigger-handler.ts` (1 item(s))

- ⚠️ **warning** at line 133:44: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\metrics\types.ts` (1 item(s))

- ⚠️ **warning** at line 7:10: 'now' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\state-managers\tool-failure-protection-types.ts` (1 item(s))

- ⚠️ **warning** at line 7:15: 'ID' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\config\types.ts` (1 item(s))

- ⚠️ **warning** at line 18:15: 'StaticNode' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\types\interruption-state.ts` (1 item(s))

- ⚠️ **warning** at line 130:11: 'oldController' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\workflow\context-compression.ts` (1 item(s))

- ⚠️ **warning** at line 49:50: 'compressionPrompt' is defined but never used. Allowed unused args must match /^_/u

