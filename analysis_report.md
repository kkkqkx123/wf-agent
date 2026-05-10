# Type Check Report

## Type Issues Summary

- **Total**: 8
- **❌** error: 3
- **⚠️** warning: 5
- **Categories**: 6
- **Files Affected**: 6
- **Packages Affected**: 3

## Breakdown by Category

- **Unexpected lexical declaration in case**: 2 occurrence(s)
- **Unexpected any. Specify a different**: 2 occurrence(s)
- **'tags' is defined but never**: 1 occurrence(s)
- **'options' is defined but never**: 1 occurrence(s)
- **run failed: command exited (1)**: 1 occurrence(s)
- **'T' is defined but never**: 1 occurrence(s)

## Details by Package

### Package: `@wf-agent/storage` (6 issue(s))

#### `D:\项目\agent\wf-agent\packages\storage\src\memory\memory-workflow-execution-storage.ts` (2 item(s))

- ❌ **error** at line 117:15: Unexpected lexical declaration in case block
- ❌ **error** at line 118:15: Unexpected lexical declaration in case block

#### `D:\项目\agent\wf-agent\packages\storage\src\json\base-json-storage.ts` (2 item(s))

- ⚠️ **warning** at line 177:31: Unexpected any. Specify a different type
- ⚠️ **warning** at line 180:30: Unexpected any. Specify a different type

#### `D:\项目\agent\wf-agent\packages\storage\src\memory\base-memory-storage.ts` (1 item(s))

- ⚠️ **warning** at line 126:14: 'options' is defined but never used. Allowed unused args must match /^_/u

#### `D:\项目\agent\wf-agent\packages\storage\src\sqlite\base-sqlite-storage.ts` (1 item(s))

- ⚠️ **warning** at line 757:44: 'T' is defined but never used. Allowed unused vars must match /^_/u

### Package: `@wf-agent/prompt-templates` (1 issue(s))

#### `D:\项目\agent\wf-agent\packages\prompt-templates\src\formatters\tool-declaration-formatter.ts` (1 item(s))

- ⚠️ **warning** at line 87:5: 'tags' is defined but never used. Allowed unused args must match /^_/u

### Package: `D` (1 issue(s))

#### `unknown` (1 item(s))

- ❌ **error** at line -: run failed: command  exited (1)

