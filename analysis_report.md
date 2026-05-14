# Type Check Report

## Type Issues Summary

- **Total**: 53
- **❌** error: 7
- **⚠️** warning: 46
- **Categories**: 18
- **Files Affected**: 21
- **Packages Affected**: 2

## Breakdown by Category

- **'_error' is defined but never**: 13 occurrence(s)
- **Unexpected console statement**: 12 occurrence(s)
- **Unexpected any. Specify a different**: 7 occurrence(s)
- **Unexpected control character(s) in regular**: 6 occurrence(s)
- **'KeyId' is defined but never**: 2 occurrence(s)
- **'_e' is defined but never**: 1 occurrence(s)
- **'cleanupAndExit' is assigned a value**: 1 occurrence(s)
- **'CODEPOINTS' is defined but never**: 1 occurrence(s)
- **run failed: command exited (1)**: 1 occurrence(s)
- **'Box' is defined but never**: 1 occurrence(s)
- **'LEGACY_KEY_SEQUENCES' is defined but never**: 1 occurrence(s)
- **'ToolApprovalRequest' is defined but never**: 1 occurrence(s)
- **'eventAPI' is assigned a value**: 1 occurrence(s)
- **'LEGACY_SEQUENCE_KEY_IDS' is defined but never**: 1 occurrence(s)
- **'ARROW_CODEPOINTS' is defined but never**: 1 occurrence(s)
- **'formatKeyNameWithModifiers' is defined but never**: 1 occurrence(s)
- **'ChildProcess' is defined but never**: 1 occurrence(s)
- **'shutdown' is assigned a value**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/cli-app` (52 issue(s))

#### `D:\项目\agent\wf-agent\apps\cli-app\src\handlers\user-interaction\followup-question.ts` (11 item(s))

- ⚠️ **warning** at line 26:5: Unexpected console statement
- ⚠️ **warning** at line 27:5: Unexpected console statement
- ⚠️ **warning** at line 28:5: Unexpected console statement
- ⚠️ **warning** at line 33:7: Unexpected console statement
- ⚠️ **warning** at line 35:9: Unexpected console statement
- ⚠️ **warning** at line 37:7: Unexpected console statement
- ⚠️ **warning** at line 38:7: Unexpected console statement
- ⚠️ **warning** at line 65:11: Unexpected console statement
- ⚠️ **warning** at line 72:7: Unexpected console statement
- ⚠️ **warning** at line 73:7: Unexpected console statement
- ⚠️ **warning** at line 77:7: Unexpected console statement

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\core\keys\kitty-protocol.ts` (6 item(s))

- ⚠️ **warning** at line 7:3: 'CODEPOINTS' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 8:3: 'ARROW_CODEPOINTS' is defined but never used. Allowed unused vars must match /^_/u
- ❌ **error** at line 78:32: Unexpected control character(s) in regular expression: \x1b
- ❌ **error** at line 90:33: Unexpected control character(s) in regular expression: \x1b
- ❌ **error** at line 100:32: Unexpected control character(s) in regular expression: \x1b
- ❌ **error** at line 121:35: Unexpected control character(s) in regular expression: \x1b

#### `D:\项目\agent\wf-agent\apps\cli-app\src\index.ts` (5 item(s))

- ⚠️ **warning** at line 65:14: '_error' is defined but never used
- ⚠️ **warning** at line 162:11: 'eventAPI' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 168:16: Unexpected any. Specify a different type
- ⚠️ **warning** at line 274:11: 'cleanupAndExit' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 330:7: 'shutdown' is assigned a value but never used. Allowed unused vars must match /^_/u

#### `D:\项目\agent\wf-agent\apps\cli-app\src\handlers\user-interaction\index.ts` (5 item(s))

- ⚠️ **warning** at line 10:15: 'ToolApprovalRequest' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 23:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 25:7: Unexpected console statement
- ⚠️ **warning** at line 50:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 51:36: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\core\keys\legacy-sequences.ts` (4 item(s))

- ⚠️ **warning** at line 17:3: 'LEGACY_KEY_SEQUENCES' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 20:3: 'LEGACY_SEQUENCE_KEY_IDS' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 23:15: 'KeyId' is defined but never used. Allowed unused vars must match /^_/u
- ❌ **error** at line 56:28: Unexpected control character(s) in regular expression: \x1b

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\core\keys\parsing.ts` (2 item(s))

- ⚠️ **warning** at line 19:15: 'KeyId' is defined but never used. Allowed unused vars must match /^_/u
- ❌ **error** at line 158:27: Unexpected control character(s) in regular expression: \x1b

#### `D:\项目\agent\wf-agent\apps\cli-app\src\commands\metrics\index.ts` (2 item(s))

- ⚠️ **warning** at line 81:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 194:29: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\commands\agent\index.ts` (2 item(s))

- ⚠️ **warning** at line 94:22: '_error' is defined but never used
- ⚠️ **warning** at line 191:22: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\services\io\human-relay-service.ts` (2 item(s))

- ⚠️ **warning** at line 104:14: '_error' is defined but never used
- ⚠️ **warning** at line 154:16: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\services\io\display-output-service.ts` (2 item(s))

- ⚠️ **warning** at line 100:16: '_error' is defined but never used
- ⚠️ **warning** at line 307:14: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\commands\tool\index.ts` (2 item(s))

- ⚠️ **warning** at line 244:22: '_error' is defined but never used
- ⚠️ **warning** at line 300:20: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\handlers\user-interaction\tool-approval.ts` (1 item(s))

- ⚠️ **warning** at line 179:14: '_e' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\services\terminal\terminal-manager.ts` (1 item(s))

- ⚠️ **warning** at line 7:22: 'ChildProcess' is defined but never used. Allowed unused vars must match /^_/u

#### `D:\项目\agent\wf-agent\apps\cli-app\src\commands\variable\index.ts` (1 item(s))

- ⚠️ **warning** at line 74:20: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\adapters\metrics-adapter.ts` (1 item(s))

- ⚠️ **warning** at line 118:24: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\commands\script\index.ts` (1 item(s))

- ⚠️ **warning** at line 244:22: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\components\tool-call-indicator.ts` (1 item(s))

- ⚠️ **warning** at line 144:14: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\commands\workflow-execution\index.ts` (1 item(s))

- ⚠️ **warning** at line 74:22: '_error' is defined but never used

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\components\iteration-panel.ts` (1 item(s))

- ⚠️ **warning** at line 8:10: 'Box' is defined but never used. Allowed unused vars must match /^_/u

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\core\keys\matching.ts` (1 item(s))

- ⚠️ **warning** at line 43:10: 'formatKeyNameWithModifiers' is defined but never used. Allowed unused vars must match /^_/u

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (1)

