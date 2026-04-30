# Character Limit Implementation - Summary

## Overview

Replaced unused `MAX_LINE_LENGTH` (per-line character limit) with `max_chars` (total character limit) to better handle files with extremely long lines (e.g., minified JSON, single-line code).

## Problem

The original `MAX_LINE_LENGTH = 2000` constant in the read_file schema was **completely unused** and would have caused issues:
- Would truncate valid long lines in JSON/minified files
- Line-by-line truncation breaks structured data
- Doesn't align with token/character-based context window management

## Solution

Implemented **total character limit** that:
1. Protects against excessive output from any source
2. Preserves complete lines (truncates at line boundaries when possible)
3. Works naturally with token estimation
4. Allows reading files with very long single lines

## Changes Made

### 1. Schema Update (`sdk/resources/predefined/tools/stateless filesystem/read-file/schema.ts`)

**Removed:**
```typescript
const MAX_LINE_LENGTH = 2000; // Unused per-line limit
```

**Added:**
```typescript
const DEFAULT_CHAR_LIMIT = 50000; // Total character limit

// New parameter in schema:
max_chars: {
  type: "integer",
  description: `Maximum total characters to return across all lines (default: ${DEFAULT_CHAR_LIMIT}). Prevents excessive output from files with extremely long lines (e.g., minified JSON). Takes precedence over line limit when exceeded.`,
  minimum: 1,
}
```

### 2. Handler Update (`sdk/resources/predefined/tools/stateless/filesystem/read-file/handler.ts`)

**Key Changes:**
- Accept `max_chars` parameter from tool invocation
- Apply character limit after formatting with line numbers
- Truncate at line boundaries when possible (preserves readability)
- Enhanced truncation messages distinguish between line limit vs character limit

**Logic Flow:**
```typescript
1. Read file → split into lines
2. Apply offset/limit → select line range
3. Format with line numbers
4. Check total character count
5. If exceeds max_chars:
   - Find last newline before limit
   - Truncate at that boundary
   - Recalculate included lines
6. Generate appropriate truncation message
```

### 3. Utility Functions Update (`sdk/utils/misc/file-reader.ts`)

**Enhanced `readWithSlice()`:**
- Added `SliceReadOptions` interface with `maxChars` support
- Maintains backward compatibility (accepts old `offset, limit` signature)
- Returns `wasCharTruncated` flag in result

**Enhanced `readWithIndentation()`:**
- Added `maxChars` to `IndentationOptions`
- Applies character limit after semantic block extraction
- Returns `wasCharTruncated` flag

**New Interface:**
```typescript
export interface SliceReadOptions {
  offset?: number;        // Starting line (0-based)
  limit?: number;         // Maximum lines to read
  maxChars?: number;      // Maximum total characters to return
}
```

### 4. Exports Update (`sdk/utils/misc/index.ts`)

Added export for new `SliceReadOptions` type.

## Usage Examples

### Basic File Reading (uses default 50KB char limit)
```typescript
// Reads up to 2000 lines OR 50000 chars, whichever comes first
await readFile({ path: "large.json" });
```

### Custom Character Limit
```typescript
// Limit to 10000 characters total
await readFile({ 
  path: "minified.json",
  max_chars: 10000 
});
```

### Combined Line + Character Limits
```typescript
// Limit to 100 lines AND 5000 chars
await readFile({ 
  path: "file.txt",
  limit: 100,
  max_chars: 5000 
});
```

### Using Utility Functions
```typescript
import { readWithSlice } from '@wf-agent/sdk/utils/misc';

// New API with options object
const result = readWithSlice(content, {
  offset: 0,
  limit: 100,
  maxChars: 5000
});

if (result.wasCharTruncated) {
  console.log("Content truncated due to character limit");
}

// Old API still works (backward compatible)
const result2 = readWithSlice(content, 0, 100);
```

## Benefits

1. **Handles Long Lines**: Can read minified JSON, single-line configs, etc.
2. **Token-Friendly**: Character count correlates better with token usage
3. **Preserves Structure**: Doesn't break JSON/XML by truncating mid-line
4. **Flexible**: Users can tune both line count and character count
5. **Backward Compatible**: Existing code continues to work unchanged
6. **Clear Feedback**: Truncation messages indicate which limit was hit

## Default Values

- **Line Limit**: 2000 lines (schema), 100 lines (utility functions)
- **Character Limit**: 50,000 characters (~12,500 tokens estimated)

These defaults balance:
- Enough content for meaningful analysis
- Protection against context window overflow
- Reasonable memory usage

## Testing Recommendations

Test with these scenarios:
1. ✅ Normal text files (< 50KB)
2. ✅ Files with very long lines (> 2000 chars per line)
3. ✅ Minified JSON/JavaScript
4. ✅ Large files requiring chunked reading
5. ✅ Empty files
6. ✅ Files exactly at character limit boundary

## Migration Notes

- **No breaking changes** - existing code works as-is
- `MAX_LINE_LENGTH` constant removed (was unused anyway)
- New `max_chars` parameter is optional with sensible default
- Utility functions support both old and new API signatures
