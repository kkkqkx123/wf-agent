# Apply-Diff Tool (SEARCH/REPLACE Format)

## Overview

The `apply_diff` tool applies changes to files using an intuitive SEARCH/REPLACE format with intelligent fuzzy matching capabilities.

## Format

```
<<<<<<< SEARCH
[code to find]
=======
[new code to replace with]
>>>>>>> REPLACE
```

## Features

- ✅ **Fuzzy Matching**: Handles minor differences (whitespace, quotes, etc.)
- ✅ **Multiple Blocks**: Apply multiple changes in one call
- ✅ **Indentation Preservation**: Automatically maintains correct indentation
- ✅ **Line Hints**: Use `:start_line:N` for precise location
- ✅ **Smart Error Recovery**: Detailed feedback helps AI correct mistakes

## Usage Examples

### Simple Replacement

```
<<<<<<< SEARCH
console.log("hello")
=======
console.log("world")
>>>>>>> REPLACE
```

### With Line Hint

```
<<<<<<< SEARCH
:start_line:10
-------
function oldName() {
    return value;
}
=======
function newName() {
    return newValue;
}
>>>>>>> REPLACE
```

### Multiple Changes

```
<<<<<<< SEARCH
const a = 1;
=======
const a = 10;
>>>>>>> REPLACE
<<<<<<< SEARCH
const b = 2;
=======
const b = 20;
>>>>>>> REPLACE
```

### Delete Content

```
<<<<<<< SEARCH
// Comment to remove
console.log("debug");
=======
>>>>>>> REPLACE
```

## Configuration

```typescript
interface ApplyDiffConfig {
  fuzzyThreshold?: number;   // Default: 0.9 (90% similarity)
  bufferLines?: number;      // Default: 40 (search context)
  workspaceDir?: string;     // Workspace directory
  enableProtect?: boolean;   // Enable file protection
}
```

## Best Practices

1. **Include Context**: Add 2-3 lines around your change for unique identification
2. **Use Line Hints**: When code appears multiple times, use `:start_line:N`
3. **Batch Related Changes**: Group related modifications in one call
4. **Check First**: Use `read_file` before editing to get current content

## Error Handling

The tool provides detailed error messages:

- **No Match Found**: Shows similarity score and best match
- **Invalid Format**: Explains what's wrong with markers
- **File Not Found**: Suggests using `read_file` first
- **Consecutive Failures**: After 2 failures, suggests re-reading the file

## Testing

Run tests:
```bash
cd sdk
pnpm test resources/predefined/tools/stateless/filesystem/apply-diff/__tests__/apply-diff-search-replace.test.ts
```

All 15 tests pass ✅

## Migration from Unified Diff

If you were using the old unified diff format:

**Old:**
```diff
--- a/file.ts
+++ b/file.ts
@@ -1,3 +1,3 @@
-old line
+new line
```

**New:**
```
<<<<<<< SEARCH
old line
=======
new line
>>>>>>> REPLACE
```

The new format is simpler and more reliable!

## Technical Details

- **Matching Algorithm**: Levenshtein distance with configurable threshold
- **Search Strategy**: Middle-out from specified line or file center
- **Indentation**: Relative level calculation preserves tabs/spaces
- **Performance**: Optimized for typical code edit sizes (< 1000 lines)
