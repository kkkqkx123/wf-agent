# Type Check Report

## Type Issues Summary

- **Total**: 37
- **⚠️** warning: 37
- **Categories**: 3
- **Files Affected**: 25

## Breakdown by Category

- **Unexpected any. Specify a different**: 34 occurrence(s)
- **'filter' is defined but never**: 2 occurrence(s)
- **'sessionId' is assigned a value**: 1 occurrence(s)

## Details by File

### `D:\项目\agent\wf-agent\sdk\agent\execution\handlers\agent-error-handler.ts` (4 item(s))

- ⚠️ **warning** at line 216:35: Unexpected any. Specify a different type
- ⚠️ **warning** at line 224:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 246:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 253:14: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\utils\checkpoint-restoration.ts` (3 item(s))

- ⚠️ **warning** at line 59:30: Unexpected any. Specify a different type
- ⚠️ **warning** at line 103:30: Unexpected any. Specify a different type
- ⚠️ **warning** at line 107:35: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\entities\workflow-execution-entity.ts` (3 item(s))

- ⚠️ **warning** at line 690:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 704:33: Unexpected any. Specify a different type
- ⚠️ **warning** at line 704:41: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\node-execution-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 461:92: Unexpected any. Specify a different type
- ⚠️ **warning** at line 496:90: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\core\sdk.ts` (2 item(s))

- ⚠️ **warning** at line 376:54: Unexpected any. Specify a different type
- ⚠️ **warning** at line 404:48: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\di\container-config.ts` (2 item(s))

- ⚠️ **warning** at line 175:17: Unexpected any. Specify a different type
- ⚠️ **warning** at line 853:52: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\ignore\IgnoreController.ts` (2 item(s))

- ⚠️ **warning** at line 75:47: Unexpected any. Specify a different type
- ⚠️ **warning** at line 80:44: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\utils\error-handler.ts` (2 item(s))

- ⚠️ **warning** at line 347:59: Unexpected any. Specify a different type
- ⚠️ **warning** at line 347:69: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\resources\generic-resource-api.ts` (1 item(s))

- ⚠️ **warning** at line 188:41: 'filter' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\serialization\entities\agent-loop-entity-serializer.ts` (1 item(s))

- ⚠️ **warning** at line 136:35: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\llm-profile.ts` (1 item(s))

- ⚠️ **warning** at line 31:48: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\validators\agent-loop-validator.ts` (1 item(s))

- ⚠️ **warning** at line 23:52: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\utils\pause-timeout-manager.ts` (1 item(s))

- ⚠️ **warning** at line 165:12: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\mcp\transport\index.ts` (1 item(s))

- ⚠️ **warning** at line 34:61: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\workflow\resources\executions\workflow-execution-registry-api.ts` (1 item(s))

- ⚠️ **warning** at line 164:5: 'filter' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\agent-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 613:77: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\node-template.ts` (1 item(s))

- ⚠️ **warning** at line 126:65: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\terminal\terminal-service.ts` (1 item(s))

- ⚠️ **warning** at line 309:17: 'sessionId' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\tool-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 464:39: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\workflow.ts` (1 item(s))

- ⚠️ **warning** at line 35:54: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\prompt-template.ts` (1 item(s))

- ⚠️ **warning** at line 32:48: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\agent-loop-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 51:20: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\executors\tool-call-executor.ts` (1 item(s))

- ⚠️ **warning** at line 306:40: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\workflow-lifecycle-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 65:23: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\variable-handler.ts` (1 item(s))

- ⚠️ **warning** at line 201:57: Unexpected any. Specify a different type

