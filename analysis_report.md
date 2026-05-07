# Type Check Report

## Type Issues Summary

- **Total**: 285
- **❌** error: 8
- **⚠️** warning: 277
- **Categories**: 40
- **Files Affected**: 54
- **Packages Affected**: 7

## Breakdown by Category

- **Unexpected any. Specify a different**: 201 occurrence(s)
- **Unexpected console statement**: 24 occurrence(s)
- **'error' is defined but never**: 9 occurrence(s)
- **Unexpected lexical declaration in case**: 7 occurrence(s)
- **'filter' is defined but never**: 5 occurrence(s)
- **'db' is assigned a value**: 4 occurrence(s)
- **'e' is defined but never**: 2 occurrence(s)
- **'McpRequest' is defined but never**: 1 occurrence(s)
- **'currentVersion' is assigned a value**: 1 occurrence(s)
- **'now' is assigned a value**: 1 occurrence(s)
- **'WorkflowExecutionOptions' is defined but never**: 1 occurrence(s)
- **'parseAndValidateAgentLoopConfig' is defined but never**: 1 occurrence(s)
- **'formatSkill' is defined but never**: 1 occurrence(s)
- **'inputData' is assigned a value**: 1 occurrence(s)
- **'ResolutionEngine' is defined but never**: 1 occurrence(s)
- **'formatWorkflowList' is defined but never**: 1 occurrence(s)
- **'WorkflowExecutionResult' is defined but never**: 1 occurrence(s)
- **'JsonNoteStorage' is defined but never**: 1 occurrence(s)
- **'setAllLoggersLevel' is defined but never**: 1 occurrence(s)
- **'FilePermissionLevel' is defined but never**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/cli-app` (236 issue(s))

#### `D:\项目\agent\wf-agent\apps\cli-app\src\handlers\cli-tool-approval-handler.ts` (26 item(s))

- ⚠️ **warning** at line 30:5: Unexpected console statement
- ⚠️ **warning** at line 31:5: Unexpected console statement
- ⚠️ **warning** at line 32:5: Unexpected console statement
- ⚠️ **warning** at line 36:7: Unexpected console statement
- ⚠️ **warning** at line 38:9: Unexpected console statement
- ... and 21 more

#### `D:\项目\agent\wf-agent\apps\cli-app\src\utils\cli-formatters.ts` (26 item(s))

- ⚠️ **warning** at line 17:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 26:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 52:52: Unexpected any. Specify a different type
- ⚠️ **warning** at line 61:15: Unexpected any. Specify a different type
- ⚠️ **warning** at line 87:46: Unexpected any. Specify a different type
- ... and 21 more

#### `D:\项目\agent\wf-agent\apps\cli-app\src\adapters\template-adapter.ts` (22 item(s))

- ⚠️ **warning** at line 24:65: Unexpected any. Specify a different type
- ⚠️ **warning** at line 50:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 78:22: Unexpected any. Specify a different type
- ⚠️ **warning** at line 109:68: Unexpected any. Specify a different type
- ⚠️ **warning** at line 135:14: Unexpected any. Specify a different type
- ... and 17 more

#### `D:\项目\agent\wf-agent\apps\cli-app\src\adapters\workflow-execution-checkpoint-adapter.ts` (15 item(s))

- ⚠️ **warning** at line 26:71: Unexpected any. Specify a different type
- ⚠️ **warning** at line 48:37: Unexpected any. Specify a different type
- ⚠️ **warning** at line 68:25: 'filter' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 68:34: Unexpected any. Specify a different type
- ⚠️ **warning** at line 68:48: Unexpected any. Specify a different type
- ... and 10 more

#### `D:\项目\agent\wf-agent\apps\cli-app\src\adapters\workflow-execution-adapter.ts` (13 item(s))

- ⚠️ **warning** at line 16:87: Unexpected any. Specify a different type
- ⚠️ **warning** at line 19:41: Unexpected any. Specify a different type
- ⚠️ **warning** at line 32:26: Unexpected any. Specify a different type
- ⚠️ **warning** at line 43:26: Unexpected any. Specify a different type
- ⚠️ **warning** at line 54:26: Unexpected any. Specify a different type
- ... and 8 more

*... and 32 more files in this package*

### Package: `@wf-agent/storage` (27 issue(s))

#### `D:\项目\agent\wf-agent\packages\storage\src\sqlite\base-sqlite-storage.ts` (10 item(s))

- ⚠️ **warning** at line 172:11: 'currentVersion' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 433:11: 'db' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 457:11: 'db' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 472:11: 'db' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 487:11: 'db' is assigned a value but never used. Allowed unused vars must match /^_/u
- ... and 5 more

#### `D:\项目\agent\wf-agent\packages\storage\src\memory\memory-workflow-execution-storage.ts` (6 item(s))

- ❌ **error** at line 117:15: Unexpected lexical declaration in case block
- ❌ **error** at line 118:15: Unexpected lexical declaration in case block
- ❌ **error** at line 123:15: Unexpected lexical declaration in case block
- ❌ **error** at line 124:15: Unexpected lexical declaration in case block
- ⚠️ **warning** at line 206:36: Unexpected any. Specify a different type
- ... and 1 more

#### `D:\项目\agent\wf-agent\packages\storage\src\json\base-json-storage.ts` (4 item(s))

- ⚠️ **warning** at line 177:31: Unexpected any. Specify a different type
- ⚠️ **warning** at line 180:30: Unexpected any. Specify a different type
- ⚠️ **warning** at line 486:29: Unexpected any. Specify a different type
- ⚠️ **warning** at line 489:28: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\packages\storage\src\compression\compressor.ts` (2 item(s))

- ❌ **error** at line 100:9: Unexpected lexical declaration in case block
- ❌ **error** at line 215:9: Unexpected lexical declaration in case block

#### `D:\项目\agent\wf-agent\packages\storage\src\memory\base-memory-storage.ts` (2 item(s))

- ⚠️ **warning** at line 34:67: Unexpected any. Specify a different type
- ⚠️ **warning** at line 126:14: 'options' is defined but never used. Allowed unused args must match /^_/u

*... and 3 more files in this package*

### Package: `@wf-agent/types` (13 issue(s))

#### `D:\项目\agent\wf-agent\packages\types\src\workflow-execution\workflow-execution.ts` (7 item(s))

- ⚠️ **warning** at line 9:15: 'WorkflowExecution' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:34: 'WorkflowExecutionOptions' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:60: 'WorkflowExecutionResult' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:15: 'WorkflowTemplate' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 11:15: 'ID' is defined but never used. Allowed unused vars must match /^_/u
- ... and 2 more

#### `D:\项目\agent\wf-agent\packages\types\src\tool\tool-schema.ts` (3 item(s))

- ⚠️ **warning** at line 8:15: 'AutoApprovalCategory' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 9:15: 'FilePermissionLevel' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 10:15: 'McpDefaultBehavior' is defined but never used. Allowed unused vars must match /^_/u

#### `D:\项目\agent\wf-agent\packages\types\src\llm\profile.ts` (1 item(s))

- ⚠️ **warning** at line 7:15: 'ToolCallFormat' is defined but never used. Allowed unused vars must match /^_/u

#### `D:\项目\agent\wf-agent\packages\types\src\tool\approval.ts` (1 item(s))

- ⚠️ **warning** at line 9:36: 'McpRequest' is defined but never used. Allowed unused vars must match /^_/u

#### `D:\项目\agent\wf-agent\packages\types\src\serialization\base.ts` (1 item(s))

- ⚠️ **warning** at line 9:15: 'SerializedError' is defined but never used. Allowed unused vars must match /^_/u

### Package: `@wf-agent/prompt-templates` (6 issue(s))

#### `D:\项目\agent\wf-agent\packages\prompt-templates\src\formatters\tool-declaration-formatter.ts` (6 item(s))

- ⚠️ **warning** at line 86:46: Unexpected any. Specify a different type
- ⚠️ **warning** at line 87:5: 'tags' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 112:22: Unexpected any. Specify a different type
- ⚠️ **warning** at line 134:32: Unexpected any. Specify a different type
- ⚠️ **warning** at line 136:22: Unexpected any. Specify a different type
- ... and 1 more

### Package: `@wf-agent/vscode-app` (1 issue(s))

#### `D:\项目\agent\wf-agent\apps\vscode-app\backend\resources\dynamic\diagnostics.ts` (1 item(s))

- ⚠️ **warning** at line 53:60: Unexpected any. Specify a different type

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (1)

### Package: `@wf-agent/common-utils` (1 issue(s))

#### `D:\项目\agent\wf-agent\packages\common-utils\src\di\types.ts` (1 item(s))

- ⚠️ **warning** at line 6:15: 'ResolutionEngine' is defined but never used. Allowed unused vars must match /^_/u

