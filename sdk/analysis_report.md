# Type Check Report

## Type Issues Summary

- **Total**: 81
- **⚠️** warning: 81
- **Categories**: 41
- **Files Affected**: 74

## Breakdown by Category

- **Unexpected any. Specify a different**: 36 occurrence(s)
- **'NotFoundError' is defined but never**: 2 occurrence(s)
- **'loopId' is defined but never**: 2 occurrence(s)
- **'ConfigType' is defined but never**: 2 occurrence(s)
- **'stream' is assigned a value**: 2 occurrence(s)
- **'node' is defined but never**: 2 occurrence(s)
- **'createErrorBuilder' is defined but never**: 1 occurrence(s)
- **'CheckpointError' is defined but never**: 1 occurrence(s)
- **'event' is defined but never**: 1 occurrence(s)
- **'options' is assigned a value**: 1 occurrence(s)
- **'ThreadIsolatedManagerFactory' is defined but never**: 1 occurrence(s)
- **'AgentLoopState' is defined but never**: 1 occurrence(s)
- **'logger' is assigned a value**: 1 occurrence(s)
- **'currentKey' is assigned a value**: 1 occurrence(s)
- **'container' is assigned a value**: 1 occurrence(s)
- **'toolCalls' is assigned a value**: 1 occurrence(s)
- **'affectedVisibleIndices' is assigned a value**: 1 occurrence(s)
- **'ok' is defined but never**: 1 occurrence(s)
- **'role' is assigned a value**: 1 occurrence(s)
- **'resource' is defined but never**: 1 occurrence(s)

## Details by File

### `D:\项目\agent\wf-agent\sdk\api\shared\core\api-factory.ts` (2 item(s))

- ⚠️ **warning** at line 110:29: Unexpected any. Specify a different type
- ⚠️ **warning** at line 112:34: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\resources\generic-resource-api.ts` (2 item(s))

- ⚠️ **warning** at line 259:5: 'resource' is defined but never used. Allowed unused args must match /^\_/u
- ⚠️ **warning** at line 273:5: 'updates' is defined but never used. Allowed unused args must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\core\utils\event\builders\common.ts` (2 item(s))

- ⚠️ **warning** at line 29:56: Unexpected any. Specify a different type
- ⚠️ **warning** at line 63:35: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\validation\message-validator.ts` (2 item(s))

- ⚠️ **warning** at line 166:23: Unexpected any. Specify a different type
- ⚠️ **warning** at line 168:22: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\workflow.ts` (2 item(s))

- ⚠️ **warning** at line 8:10: 'ConfigType' is defined but never used. Allowed unused vars must match /^\_/u
- ⚠️ **warning** at line 41:31: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\graph\execution\handlers\node-handlers\loop-start-handler.ts` (2 item(s))

- ⚠️ **warning** at line 167:39: 'loopId' is defined but never used. Allowed unused args must match /^\_/u
- ⚠️ **warning** at line 188:41: 'loopId' is defined but never used. Allowed unused args must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\gemini-native.ts` (2 item(s))

- ⚠️ **warning** at line 135:14: '\_e' is defined but never used
- ⚠️ **warning** at line 148:11: 'toolCalls' is assigned a value but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\core\utils\checkpoint\cleanup-policy.ts` (1 item(s))

- ⚠️ **warning** at line 178:66: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\agent\execution\executors\agent-stream-executor.ts` (1 item(s))

- ⚠️ **warning** at line 265:11: 'donePromise' is assigned a value but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\agent\execution\factories\agent-loop-factory.ts` (1 item(s))

- ⚠️ **warning** at line 19:10: 'AgentLoopState' is defined but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\core\managers\execution-queue-manager.ts` (1 item(s))

- ⚠️ **warning** at line 333:5: 'executionTime' is defined but never used. Allowed unused args must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\api\graph\resources\variables\variable-resource-api.ts` (1 item(s))

- ⚠️ **warning** at line 181:11: 'thread' is assigned a value but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\graph\execution\coordinators\llm-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 500:11: 'toolApprovalData' is assigned a value but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\operations\events\dispatch-event-command.ts` (1 item(s))

- ⚠️ **warning** at line 55:51: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\gemini-openai.ts` (1 item(s))

- ⚠️ **warning** at line 208:13: 'stream' is assigned a value but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\graph\preprocessing\id-mapping-builder.ts` (1 item(s))

- ⚠️ **warning** at line 158:5: 'workflowRegistry' is defined but never used. Allowed unused args must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\graph\execution\handlers\node-handlers\continue-from-trigger-handler.ts` (1 item(s))

- ⚠️ **warning** at line 8:46: 'LLMMessage' is defined but never used. Allowed unused vars must match /^\_/u

### `D:\项目\agent\wf-agent\sdk\core\llm\message-stream.ts` (1 item(s))

- ⚠️ **warning** at line 327:18: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\graph\builders\node-builder.ts` (1 item(s))

- ⚠️ **warning** at line 90:85: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\agent\entities\agent-loop-entity.ts` (1 item(s))

- ⚠️ **warning** at line 224:36: Unexpected any. Specify a different type

_... and 54 more files (use --verbose to see all)_
