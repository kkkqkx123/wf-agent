# Tool Utility Organization Architecture

## Overview

This document describes the architecture for organizing utility functions in the Modular Agent Framework, specifically distinguishing between **general-purpose utilities** and **tool-specific utilities**.

## Architecture Decision

### Two-Layer Utility Structure

```
sdk/
├── utils/misc/                    # General-purpose utilities (cross-tool reuse)
│   ├── file-reader.ts            # ✅ File reading with slicing/indentation
│   ├── binary-detector.ts        # ✅ Binary file detection
│   ├── text-extractor.ts         # ✅ PDF/DOCX/XLSX text extraction
│   ├── image-processor.ts        # ✅ Image validation and processing
│   ├── line-number-utils.ts      # ✅ Line number formatting
│   ├── stream-reader.ts          # ✅ Streaming file reads
│   ├── terminal-output-utils.ts  # ✅ Terminal output processing
│   └── token-aware-reader.ts     # ✅ Token-budget-aware reading
│
└── resources/predefined/tools/
    └── stateless/filesystem/
        ├── apply-diff/
        │   ├── handler.ts
        │   └── utils/             # 🔧 Tool-specific utilities
        │       ├── diff-stats.ts
        │       ├── text-normalization.ts
        │       └── index.ts
        │
        └── apply-patch/
            ├── handler.ts
            └── utils/             # 🔧 Tool-specific utilities (already exists)
                ├── parser.ts
                ├── apply.ts
                ├── matcher.ts
                └── types.ts
```

## Classification Criteria

### ✅ Keep in `sdk/utils/misc` (General-Purpose)

A utility should remain in `sdk/utils/misc` if it meets **ANY** of these criteria:

1. **Used by multiple tools** - e.g., `binary-detector.ts` used by `read-file`, potentially by `grep`
2. **Domain-general functionality** - e.g., file I/O, text processing, image handling
3. **Reusable across different contexts** - e.g., line number formatting useful for any code display
4. **Part of core SDK capabilities** - e.g., token estimation, streaming reads

**Examples:**
- `file-reader.ts` - Any tool that reads files can use this
- `binary-detector.ts` - Search, grep, and other tools need binary detection
- `text-extractor.ts` - Generic PDF/DOCX extraction usable anywhere
- `image-processor.ts` - Image handling is a general capability
- `line-number-utils.ts` - Useful for displaying any code/text with line numbers
- `stream-reader.ts` - Generic large file handling
- `terminal-output-utils.ts` - Shell tools and terminal emulators need this
- `token-aware-reader.ts` - Any LLM-integrated tool needs token management

### 📦 Move to `tools/{tool-name}/utils/` (Tool-Specific)

A utility should be in a tool's local `utils/` directory if it meets **ALL** of these criteria:

1. **Only used by one tool** - No other tool needs this functionality
2. **Tightly coupled to tool's format/logic** - e.g., unified diff parsing
3. **Implementation details** - Not part of the public API contract
4. **Format-specific processing** - e.g., patch format, diff format

**Examples:**
- `apply-diff/utils/diff-stats.ts` - Only applies to unified diff format
- `apply-diff/utils/text-normalization.ts` - Specific to diff content from LLMs
- `apply-patch/utils/parser.ts` - Custom Codex patch format parser
- `apply-patch/utils/apply.ts` - Patch-specific chunk application logic

## Benefits of This Architecture

### 1. **Clear Separation of Concerns**
- General utilities are easy to find and reuse
- Tool-specific logic stays close to where it's used
- Reduces cognitive load when navigating codebase

### 2. **Better Encapsulation**
- Tool-specific utilities are implementation details
- Changes to tool internals don't affect other parts of the system
- Easier to refactor or replace individual tools

### 3. **Improved Discoverability**
- Developers know where to look:
  - Need general file handling? → `sdk/utils/misc/`
  - Need to understand apply-diff? → `apply-diff/utils/`

### 4. **Reduced Coupling**
- Tools don't depend on unrelated utilities
- Easier to extract tools into separate packages if needed
- Cleaner dependency graph

### 5. **Easier Testing**
- Tool-specific utilities can be tested alongside the tool
- General utilities have their own comprehensive test suites
- Clear test organization matches code organization

## Migration Guidelines

### When to Move a Utility to Tool-Specific

**Move if:**
```typescript
// ❌ Before: In sdk/utils/misc but only used by one tool
import { computeDiffStats } from "@wf-agent/sdk";

// ✅ After: In tool's utils directory
import { computeDiffStats } from "./utils/diff-stats.js";
```

**Criteria checklist:**
- [ ] Used by only ONE tool
- [ ] Tied to specific format (diff, patch, etc.)
- [ ] Not part of general SDK capabilities
- [ ] Moving won't break other tools

### When to Keep in General Utilities

**Keep if:**
```typescript
// ✅ Already in right place: used by multiple tools
import { detectBinaryFile } from "@wf-agent/sdk"; // read-file, grep, etc.
```

**Criteria checklist:**
- [ ] Used by 2+ tools OR likely to be reused
- [ ] Solves a general problem (file I/O, text processing)
- [ ] Part of SDK's public API surface
- [ ] Independent of specific tool formats

## Current Status

### ✅ Correctly Placed (sdk/utils/misc)
- `file-reader.ts` - Multi-tool usage
- `binary-detector.ts` - Multi-tool usage
- `text-extractor.ts` - General capability
- `image-processor.ts` - General capability
- `line-number-utils.ts` - General formatting
- `stream-reader.ts` - General I/O pattern
- `terminal-output-utils.ts` - Shell/terminal tools
- `token-aware-reader.ts` - LLM integration pattern

### ✅ Correctly Placed (tool-specific)
- `apply-diff/utils/diff-stats.ts` - Unified diff specific
- `apply-diff/utils/text-normalization.ts` - Diff content specific
- `apply-patch/utils/*` - Custom patch format specific

## Future Considerations

### If a Tool-Specific Utility Becomes General

**Scenario:** Another tool starts needing diff statistics

**Action:**
1. Move from `apply-diff/utils/diff-stats.ts` to `sdk/utils/misc/diff-stats.ts`
2. Update imports in both tools
3. Add comprehensive tests in `sdk/utils/misc/__tests__/`
4. Update documentation

### If a General Utility Becomes Obsolete

**Scenario:** No tools use `image-processor.ts` anymore

**Action:**
1. Verify no external dependencies
2. Deprecate with warning in next minor version
3. Remove in next major version
4. Update migration guide

## Best Practices

### For New Utilities

1. **Ask: "Who else might need this?"**
   - If yes → `sdk/utils/misc/`
   - If no → `{tool}/utils/`

2. **Consider the abstraction level**
   - Low-level I/O/formatting → `sdk/utils/misc/`
   - High-level domain logic → `{tool}/utils/`

3. **Think about testing**
   - General utilities need broader test coverage
   - Tool-specific utilities tested with the tool

### For Refactoring

1. **Search before moving**
   ```bash
   grep -r "computeDiffStats" sdk/
   ```

2. **Update all imports atomically**
   - Use IDE refactoring tools
   - Verify build succeeds
   - Run tests

3. **Document the change**
   - Update this architecture doc
   - Add migration notes if breaking

## Conclusion

This two-layer architecture provides:
- **Clarity**: Easy to understand where utilities belong
- **Flexibility**: Easy to move utilities as usage patterns evolve
- **Maintainability**: Reduced coupling and better encapsulation
- **Scalability**: Supports adding new tools without cluttering global utilities

The key principle: **General capabilities stay general, specific implementations stay specific.**
