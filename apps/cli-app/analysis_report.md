# Type Check Report

## Type Issues Summary

- **Total**: 249
- **❌** error: 12
- **⚠️** warning: 237
- **Categories**: 39
- **Files Affected**: 54
- **Packages Affected**: 5

## Breakdown by Category

- **Unexpected any. Specify a different**: 155 occurrence(s)
- **Unexpected console statement**: 33 occurrence(s)
- **'error' is defined but never**: 14 occurrence(s)
- **'filter' is defined but never**: 5 occurrence(s)
- **'data' is defined but never**: 4 occurrence(s)
- **Unexpected lexical declaration in case**: 3 occurrence(s)
- **'e' is defined but never**: 2 occurrence(s)
- **'agentLoopId' is defined but never**: 2 occurrence(s)
- **'matchesKey' is defined but never**: 1 occurrence(s)
- **'formatWorkflowList' is defined but never**: 1 occurrence(s)
- **Lifecycle script `lint` failed with**: 1 occurrence(s)
- **'width' is defined but never**: 1 occurrence(s)
- **'rgiEmojiRegex' is assigned a value**: 1 occurrence(s)
- **'CLINotFoundError' is defined but never**: 1 occurrence(s)
- **'baseKey' is never reassigned. Use**: 1 occurrence(s)
- **'parseAndValidateAgentLoopConfig' is defined but never**: 1 occurrence(s)
- **'TerminalSession' is defined but never**: 1 occurrence(s)
- **workspace @wf-agent/cli-app@1.0.0**: 1 occurrence(s)
- **location D:\项目\agent\wf-agent\apps\cli-app**: 1 occurrence(s)
- **'readFileSync' is defined but never**: 1 occurrence(s)
- **command failed**: 1 occurrence(s)
- **The value assigned to 'result'**: 1 occurrence(s)
- **'formatSkill' is defined but never**: 1 occurrence(s)
- **NPM error code: 1**: 1 occurrence(s)
- **command C:\WINDOWS\system32\cmd.exe /d /s /c**: 1 occurrence(s)
- **'handleCLIError' is defined but never**: 1 occurrence(s)
- **'explicitTab' is assigned a value**: 1 occurrence(s)
- **'inputData' is assigned a value**: 1 occurrence(s)
- **'JsonNoteStorage' is defined but never**: 1 occurrence(s)
- **'dir' is assigned a value**: 1 occurrence(s)
- **'ATTACHMENT_AUTOCOMPLETE_DEBOUNCE_MS' is assigned a value**: 1 occurrence(s)
- **'config' is defined but never**: 1 occurrence(s)
- **'stat' is defined but never**: 1 occurrence(s)
- **'sessionId' is defined but never**: 1 occurrence(s)
- **path D:\项目\agent\wf-agent\apps\cli-app**: 1 occurrence(s)
- **'status' is assigned a value**: 1 occurrence(s)
- **'num' is assigned a value**: 1 occurrence(s)
- **'Box' is defined but never**: 1 occurrence(s)
- **'getCommunicationBridge' is defined but never**: 1 occurrence(s)

## Details by Package

### Package: `  205` (49 issue(s))

#### `D:\项目\agent\wf-agent\apps\cli-app\src\utils\cli-formatters.ts` (26 item(s))

- ⚠️ **warning** at line 17:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 26:14: Unexpected any. Specify a different type
- ⚠️ **warning** at line 52:52: Unexpected any. Specify a different type
- ⚠️ **warning** at line 61:15: Unexpected any. Specify a different type
- ⚠️ **warning** at line 87:46: Unexpected any. Specify a different type
- ⚠️ **warning** at line 95:51: Unexpected any. Specify a different type
- ⚠️ **warning** at line 118:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 131:48: Unexpected any. Specify a different type
- ⚠️ **warning** at line 156:38: Unexpected any. Specify a different type
- ⚠️ **warning** at line 168:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 193:34: Unexpected any. Specify a different type
- ⚠️ **warning** at line 203:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 227:40: Unexpected any. Specify a different type
- ⚠️ **warning** at line 239:45: Unexpected any. Specify a different type
- ⚠️ **warning** at line 263:40: Unexpected any. Specify a different type
- ⚠️ **warning** at line 276:45: Unexpected any. Specify a different type
- ⚠️ **warning** at line 299:53: Unexpected any. Specify a different type
- ⚠️ **warning** at line 310:29: Unexpected any. Specify a different type
- ⚠️ **warning** at line 337:36: Unexpected any. Specify a different type
- ⚠️ **warning** at line 350:41: Unexpected any. Specify a different type
- ⚠️ **warning** at line 374:42: Unexpected any. Specify a different type
- ⚠️ **warning** at line 384:47: Unexpected any. Specify a different type
- ⚠️ **warning** at line 408:44: Unexpected any. Specify a different type
- ⚠️ **warning** at line 430:49: Unexpected any. Specify a different type
- ⚠️ **warning** at line 454:36: Unexpected any. Specify a different type
- ⚠️ **warning** at line 467:41: Unexpected any. Specify a different type

#### `package.json` (7 item(s))

- ❌ **error** at line -: Lifecycle script `lint` failed with error:
- ❌ **error** at line -: NPM error code: 1
- ❌ **error** at line -: path D:\项目\agent\wf-agent\apps\cli-app
- ❌ **error** at line -: workspace @wf-agent/cli-app@1.0.0
- ❌ **error** at line -: location D:\项目\agent\wf-agent\apps\cli-app
- ❌ **error** at line -: command failed
- ❌ **error** at line -: command C:\WINDOWS\system32\cmd.exe /d /s /c eslint . --ext .ts

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\screens\agent-screen.ts` (5 item(s))

- ❌ **error** at line 163:9: Unexpected lexical declaration in case block
- ❌ **error** at line 171:9: Unexpected lexical declaration in case block
- ⚠️ **warning** at line 265:50: Unexpected any. Specify a different type
- ⚠️ **warning** at line 270:48: Unexpected any. Specify a different type
- ⚠️ **warning** at line 365:27: 'config' is defined but never used. Allowed unused args must match /^_/u

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\screens\workflow-screen.ts` (5 item(s))

- ⚠️ **warning** at line 155:39: Unexpected any. Specify a different type
- ⚠️ **warning** at line 168:7: Unexpected console statement
- ⚠️ **warning** at line 188:43: Unexpected any. Specify a different type
- ⚠️ **warning** at line 224:7: Unexpected console statement
- ⚠️ **warning** at line 230:7: Unexpected console statement

#### `D:\项目\agent\wf-agent\apps\cli-app\src\utils\formatter.ts` (2 item(s))

- ⚠️ **warning** at line 218:11: 'status' is assigned a value but never used. Allowed unused vars must match /^_/u
- ❌ **error** at line 220:9: The value assigned to 'result' is not used in subsequent statements

#### `D:\项目\agent\wf-agent\apps\cli-app\src\utils\output.ts` (2 item(s))

- ⚠️ **warning** at line 363:32: Unexpected any. Specify a different type
- ⚠️ **warning** at line 376:32: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\handlers\tui-human-relay-handler.ts` (1 item(s))

- ⚠️ **warning** at line 37:53: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\utils\error-handler.ts` (1 item(s))

- ⚠️ **warning** at line 43:35: Unexpected any. Specify a different type

### Package: `   336` (12 issue(s))

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\components\file-selection.ts` (3 item(s))

- ⚠️ **warning** at line 6:19: 'stat' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 128:7: Unexpected console statement
- ⚠️ **warning** at line 136:40: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\components\tool-call-indicator.ts` (3 item(s))

- ⚠️ **warning** at line 8:10: 'Box' is defined but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 145:14: 'error' is defined but never used
- ⚠️ **warning** at line 150:16: 'data' is defined but never used. Allowed unused args must match /^_/u

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\components\iteration-panel.ts` (3 item(s))

- ⚠️ **warning** at line 99:10: 'width' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 113:17: 'num' is assigned a value but never used. Allowed unused vars must match /^_/u
- ⚠️ **warning** at line 149:16: 'data' is defined but never used. Allowed unused args must match /^_/u

#### `\x1b, \x1b                                               no-control-regex` (2 item(s))

- ⚠️ **warning** at line 1511:33: 'explicitTab' is assigned a value but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 1589:53: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\core\autocomplete.ts` (1 item(s))

- ⚠️ **warning** at line 49:9: Unexpected console statement

### Package: `  139` (3 issue(s))

#### `\x1b             no-control-regex` (3 item(s))

- ❌ **error** at line 181:9: 'baseKey' is never reassigned. Use 'const' instead
- ⚠️ **warning** at line 371:30: 'data' is defined but never used. Allowed unused args must match /^_/u
- ⚠️ **warning** at line 382:29: 'data' is defined but never used. Allowed unused args must match /^_/u

### Package: `  104` (1 issue(s))

#### `D:\项目\agent\wf-agent\apps\cli-app\src\tui\core\tui.ts` (1 item(s))

- ⚠️ **warning** at line 5:24: 'matchesKey' is defined but never used. Allowed unused vars must match /^_/u

### Package: `   31` (1 issue(s))

#### `\x00, \x1f                         no-control-regex` (1 item(s))

- ⚠️ **warning** at line 33:7: 'rgiEmojiRegex' is assigned a value but never used. Allowed unused vars must match /^_/u

