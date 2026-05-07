# Type Check Report

## Type Issues Summary

- **Total**: 82
- **⚠️** warning: 82
- **Categories**: 26
- **Files Affected**: 59

## Breakdown by Category

- **Unexpected any. Specify a different**: 35 occurrence(s)
- **Unexpected console statement**: 14 occurrence(s)
- **'error' is defined but never**: 5 occurrence(s)
- **'config' is defined but never**: 4 occurrence(s)
- **'node' is defined but never**: 2 occurrence(s)
- **'filter' is defined but never**: 2 occurrence(s)
- **'sessionId' is assigned a value**: 1 occurrence(s)
- **'now' is defined but never**: 1 occurrence(s)
- **'GraphLLMExecutionConfig' is defined but never**: 1 occurrence(s)
- **'executionId' is defined but never**: 1 occurrence(s)
- **'prefix' is assigned a value**: 1 occurrence(s)
- **'e' is defined but never**: 1 occurrence(s)
- **'getErrorMessage' is defined but never**: 1 occurrence(s)
- **'event' is defined but never**: 1 occurrence(s)
- **'failed' is assigned a value**: 1 occurrence(s)
- **'nodeResult' is assigned a value**: 1 occurrence(s)
- **'timeout' is defined but never**: 1 occurrence(s)
- **'McpServerState' is defined but never**: 1 occurrence(s)
- **'suffix' is assigned a value**: 1 occurrence(s)
- **'getCurrentTimestamp' is defined but never**: 1 occurrence(s)
- **'format' is assigned a value**: 1 occurrence(s)
- **'maxFileSize' is assigned a value**: 1 occurrence(s)
- **'rename' is defined but never**: 1 occurrence(s)
- **'AgentLoopStateSnapshot' is defined but never**: 1 occurrence(s)
- **'err' is assigned a value**: 1 occurrence(s)
- **'options' is defined but never**: 1 occurrence(s)

## Details by File

### `D:\项目\agent\wf-agent\sdk\agent\execution\handlers\agent-error-handler.ts` (4 item(s))

- ⚠️ **warning** at line 216:35: Unexpected any. Specify a different type
- ⚠️ **warning** at line 224:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 246:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 253:14: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\entities\workflow-execution-entity.ts` (3 item(s))

- ⚠️ **warning** at line 690:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 704:33: Unexpected any. Specify a different type
- ⚠️ **warning** at line 704:41: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\component-message\message-bus.ts` (3 item(s))

- ⚠️ **warning** at line 154:15: Unexpected console statement
- ⚠️ **warning** at line 389:15: Unexpected console statement
- ⚠️ **warning** at line 393:11: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\core\coordinators\tool-approval-coordinator.ts` (3 item(s))

- ⚠️ **warning** at line 306:9: Unexpected console statement
- ⚠️ **warning** at line 308:9: Unexpected console statement
- ⚠️ **warning** at line 310:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\workflow\execution\utils\checkpoint-restoration.ts` (3 item(s))

- ⚠️ **warning** at line 59:30: Unexpected any. Specify a different type
- ⚠️ **warning** at line 103:30: Unexpected any. Specify a different type
- ⚠️ **warning** at line 107:35: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\di\container-config.ts` (2 item(s))

- ⚠️ **warning** at line 175:17: Unexpected any. Specify a different type
- ⚠️ **warning** at line 853:52: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\core\sdk.ts` (2 item(s))

- ⚠️ **warning** at line 376:54: Unexpected any. Specify a different type
- ⚠️ **warning** at line 404:48: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\ignore\IgnoreController.ts` (2 item(s))

- ⚠️ **warning** at line 75:47: Unexpected any. Specify a different type
- ⚠️ **warning** at line 80:44: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\route-handler.ts` (2 item(s))

- ⚠️ **warning** at line 11:10: 'now' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 11:15: 'getErrorMessage' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\checkpoint\checkpoint-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 445:7: Unexpected console statement
- ⚠️ **warning** at line 471:11: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\apply-patch\handler.ts` (2 item(s))

- ⚠️ **warning** at line 5:46: 'rename' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 218:23: 'err' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\prompts\system\system-prompt-builder.ts` (2 item(s))

- ⚠️ **warning** at line 87:5: 'prefix' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 88:5: 'suffix' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\anthropic.ts` (2 item(s))

- ⚠️ **warning** at line 157:48: 'config' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 200:11: 'format' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\node-execution-coordinator.ts` (2 item(s))

- ⚠️ **warning** at line 461:92: Unexpected any. Specify a different type
- ⚠️ **warning** at line 496:90: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\mcp\transport\sse.ts` (2 item(s))

- ⚠️ **warning** at line 75:32: 'event' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 80:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\core\registry\execution-hierarchy-registry.ts` (2 item(s))

- ⚠️ **warning** at line 238:11: Unexpected console statement
- ⚠️ **warning** at line 248:11: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\core\utils\error-handler.ts` (2 item(s))

- ⚠️ **warning** at line 347:59: Unexpected any. Specify a different type
- ⚠️ **warning** at line 347:69: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\validators\agent-loop-validator.ts` (1 item(s))

- ⚠️ **warning** at line 23:52: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\openai-response.ts` (1 item(s))

- ⚠️ **warning** at line 64:48: 'config' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\execution\factories\workflow-execution-builder.ts` (1 item(s))

- ⚠️ **warning** at line 22:29: 'getCurrentTimestamp' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\llm-profile.ts` (1 item(s))

- ⚠️ **warning** at line 31:48: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\loop-end-handler.ts` (1 item(s))

- ⚠️ **warning** at line 29:71: 'node' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\openai-chat.ts` (1 item(s))

- ⚠️ **warning** at line 29:48: 'config' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\tool-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 464:39: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\subgraph-handler.ts` (1 item(s))

- ⚠️ **warning** at line 99:13: 'nodeResult' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\builtin\agent\call-agent\handler.ts` (1 item(s))

- ⚠️ **warning** at line 119:20: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\prompt-template.ts` (1 item(s))

- ⚠️ **warning** at line 32:48: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\mcp\transport\index.ts` (1 item(s))

- ⚠️ **warning** at line 34:61: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\agent\entities\agent-loop-entity.ts` (1 item(s))

- ⚠️ **warning** at line 663:14: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\utils\pause-timeout-manager.ts` (1 item(s))

- ⚠️ **warning** at line 165:12: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\mcp\transport\streamable-http.ts` (1 item(s))

- ⚠️ **warning** at line 51:14: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\agent-execution-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 613:77: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\workflow.ts` (1 item(s))

- ⚠️ **warning** at line 35:54: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\gemini-native.ts` (1 item(s))

- ⚠️ **warning** at line 138:42: 'config' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\services\executors\BaseExecutor.ts` (1 item(s))

- ⚠️ **warning** at line 150:14: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\services\terminal\terminal-service.ts` (1 item(s))

- ⚠️ **warning** at line 309:17: 'sessionId' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\shared\config\processors\node-template.ts` (1 item(s))

- ⚠️ **warning** at line 126:65: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\registry\trigger-template-registry.ts` (1 item(s))

- ⚠️ **warning** at line 152:28: 'options' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\workflow\resources\templates\trigger-template-registry-api.ts` (1 item(s))

- ⚠️ **warning** at line 148:9: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\read-file\handler.ts` (1 item(s))

- ⚠️ **warning** at line 62:9: 'maxFileSize' is assigned a value but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\services\mcp\mcp-client.ts` (1 item(s))

- ⚠️ **warning** at line 154:5: 'timeout' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\registry\event-registry.ts` (1 item(s))

- ⚠️ **warning** at line 305:71: 'failed' is assigned a value but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\agent\execution\coordinators\agent-loop-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 51:20: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\llm-handler.ts` (1 item(s))

- ⚠️ **warning** at line 14:15: 'GraphLLMExecutionConfig' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\serialization\entities\agent-loop-checkpoint-serializer.ts` (1 item(s))

- ⚠️ **warning** at line 14:3: 'AgentLoopStateSnapshot' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\api\workflow\resources\executions\workflow-execution-registry-api.ts` (1 item(s))

- ⚠️ **warning** at line 164:5: 'filter' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\services\mcp\config\loader.ts` (1 item(s))

- ⚠️ **warning** at line 60:7: Unexpected console statement

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\loop-start-handler.ts` (1 item(s))

- ⚠️ **warning** at line 27:63: 'node' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\core\llm\formatters\tool-call-parser.ts` (1 item(s))

- ⚠️ **warning** at line 226:14: 'e' is defined but never used

### `D:\项目\agent\wf-agent\sdk\core\serialization\entities\agent-loop-entity-serializer.ts` (1 item(s))

- ⚠️ **warning** at line 136:35: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\api\shared\resources\generic-resource-api.ts` (1 item(s))

- ⚠️ **warning** at line 188:41: 'filter' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\workflow\state-managers\variable-state.ts` (1 item(s))

- ⚠️ **warning** at line 58:15: 'executionId' is defined but never used. Allowed unused args must match /^_/u

### `D:\项目\agent\wf-agent\sdk\resources\predefined\tools\stateless\filesystem\grep\handler.ts` (1 item(s))

- ⚠️ **warning** at line 70:16: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\services\mcp\server-registry.ts` (1 item(s))

- ⚠️ **warning** at line 6:34: 'McpServerState' is defined but never used. Allowed unused vars must match /^_/u

### `D:\项目\agent\wf-agent\sdk\utils\misc\token-aware-reader.ts` (1 item(s))

- ⚠️ **warning** at line 81:16: 'error' is defined but never used

### `D:\项目\agent\wf-agent\sdk\workflow\execution\coordinators\workflow-lifecycle-coordinator.ts` (1 item(s))

- ⚠️ **warning** at line 65:23: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\workflow\execution\handlers\node-handlers\variable-handler.ts` (1 item(s))

- ⚠️ **warning** at line 201:57: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\core\executors\tool-call-executor.ts` (1 item(s))

- ⚠️ **warning** at line 306:40: Unexpected any. Specify a different type

### `D:\项目\agent\wf-agent\sdk\services\protect\ProtectController.ts` (1 item(s))

- ⚠️ **warning** at line 105:7: Unexpected console statement

