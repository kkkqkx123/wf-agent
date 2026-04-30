# SQLite Storage Module Issues

## Issue Summary

The SQLite storage module in `packages/storage` is experiencing native module binding failures when running tests on Windows with Node.js v22.14.0. This prevents all SQLite-based storage tests from executing, although the code compiles successfully.

---

## Problem Description

### Error Message

```
Error: Could not locate the bindings file. Tried:
 → D:\项目\agent\wf-agent\node_modules\.pnpm\better-sqlite3@12.6.2\node_modules\better-sqlite3\build\better_sqlite3.node
 → D:\项目\agent\wf-agent\node_modules\.pnpm\better-sqlite3@12.6.2\node_modules\better-sqlite3\build\Debug\better_sqlite3.node
 → D:\项目\agent\wf-agent\node_modules\.pnpm\better-sqlite3@12.6.2\node_modules\better-sqlite3\build\Release\better_sqlite3.node
 → ... (multiple paths)
 → D:\项目\agent\wf-agent\node_modules\.pnpm\better-sqlite3@12.6.2\node_modules\better-sqlite3\lib\binding\node-v127-win32-x64\better_sqlite3.node
```

### Affected Components

- **Package**: `@wf-agent/storage`
- **Dependency**: `better-sqlite3@^12.6.2`
- **Node.js Version**: v22.14.0 (ABI version 127)
- **Platform**: Windows (win32-x64)
- **Failed Tests**: All SQLite storage tests (89 test cases)
  - `sqlite-checkpoint-storage.test.ts`
  - `sqlite-task-storage.test.ts`
  - `sqlite-workflow-storage.test.ts`
  - `base-sqlite-storage.test.ts`

### Impact

- ❌ **All SQLite storage tests fail** during initialization
- ✅ JSON storage tests pass successfully (149 tests)
- ✅ Compression module tests pass successfully (32 tests)
- ✅ TypeScript compilation succeeds without errors
- ⚠️ Production impact unknown (depends on deployment environment)

---

## Root Cause Analysis

### 1. Native Module Binding Issue

**Primary Cause**: The `better-sqlite3` package requires platform-specific native bindings (`.node` files) that are compiled for specific Node.js ABI versions.

**Current Situation**:
- Node.js v22.14.0 uses **ABI version 127**
- The installed `better-sqlite3@12.6.2` may not have pre-built binaries for ABI 127
- The native module needs to be rebuilt for the current Node.js version

### 2. pnpm Monorepo Complications

**Secondary Cause**: In a pnpm monorepo with hoisted dependencies, native modules can face issues with:
- Incorrect symlinks to native bindings
- Missing rebuild triggers after Node.js version changes
- Platform-specific binary resolution failures

### 3. Windows-Specific Considerations

**Tertiary Factors**:
- Windows requires Visual Studio Build Tools for native module compilation
- Path length limitations on Windows (260 character limit)
- Different path separators and encoding issues

---

## Current Workarounds

### What Works ✅

1. **JSON Storage**: All JSON-based storage implementations work correctly
2. **Compression Module**: Independent compression utilities function properly
3. **TypeScript Compilation**: No type errors, builds successfully
4. **Code Structure**: Architecture and logic are sound

### What Doesn't Work ❌

1. **SQLite Tests**: Cannot run any SQLite-related tests
2. **Runtime Initialization**: SQLite databases cannot be initialized in test environment
3. **Integration Testing**: End-to-end tests requiring SQLite are blocked

---

## Proposed Solutions

### Solution 1: Rebuild Native Modules (Recommended)

**Steps**:
```bash
# Clean and reinstall dependencies
cd d:\项目\agent\wf-agent
pnpm clean
rm -rf node_modules
pnpm install

# Rebuild native modules specifically
cd packages/storage
pnpm rebuild better-sqlite3

# Or rebuild all native modules
pnpm rebuild
```

**Pros**:
- Direct fix for the binding issue
- Ensures binaries match current Node.js version
- Minimal code changes required

**Cons**:
- Requires build tools (Visual Studio on Windows)
- May need administrator privileges
- Time-consuming for large projects

---

### Solution 2: Use Pre-built Binaries

**Approach**: Switch to a version of `better-sqlite3` that has pre-built binaries for Node.js v22/ABI 127.

**Steps**:
```json
// packages/storage/package.json
{
  "dependencies": {
    "better-sqlite3": "^11.0.0"  // Check which version supports ABI 127
  }
}
```

**Pros**:
- No compilation required
- Faster installation
- More reliable across environments

**Cons**:
- May need to downgrade to older version
- Potential API differences
- May miss recent bug fixes

---

### Solution 3: Alternative SQLite Library

**Approach**: Replace `better-sqlite3` with an alternative that has better Node.js v22 support.

**Options**:
1. **sql.js**: Pure JavaScript implementation (no native bindings)
   ```json
   {
     "dependencies": {
       "sql.js": "^1.8.0"
     }
   }
   ```

2. **@vscode/sqlite3**: Microsoft's fork with better maintenance
   ```json
   {
     "dependencies": {
       "@vscode/sqlite3": "^5.1.6"
     }
   }
   ```

**Pros**:
- Better long-term maintainability
- Potentially better Node.js version support
- sql.js requires no native compilation

**Cons**:
- Significant refactoring required
- API differences
- Performance implications (especially sql.js)
- Migration effort for existing code

---

### Solution 4: Docker/Container Development

**Approach**: Use Docker containers with pre-configured Node.js environments.

**Dockerfile Example**:
```dockerfile
FROM node:22-alpine

RUN apk add --no-cache python3 make g++

WORKDIR /app
COPY . .
RUN pnpm install
RUN pnpm rebuild

CMD ["pnpm", "test"]
```

**Pros**:
- Consistent environment across team members
- Isolates native module issues
- Easier CI/CD integration

**Cons**:
- Requires Docker setup
- Slower development iteration
- Additional infrastructure complexity

---

### Solution 5: Conditional Test Execution

**Approach**: Skip SQLite tests when native bindings are unavailable, focus on JSON storage tests.

**Implementation**:
```typescript
// packages/storage/src/sqlite/__tests__/helper.ts
import { canInitializeSQLite } from '../utils/check-native-binding.js';

export const describeIf = canInitializeSQLite() ? describe : describe.skip;

describeIf('SqliteCheckpointStorage', () => {
  // Tests only run if SQLite is available
});
```

**Pros**:
- Allows other tests to proceed
- Clear indication of skipped tests
- Graceful degradation

**Cons**:
- Doesn't fix the underlying issue
- Reduced test coverage
- Masks potential production issues

---

## Immediate Action Items

### Priority 1: Fix Development Environment

1. **Verify Build Tools Installation**
   ```powershell
   # Check if Visual Studio Build Tools are installed
   vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64
   
   # If not installed, download from:
   # https://visualstudio.microsoft.com/visual-cpp-build-tools/
   ```

2. **Rebuild Native Modules**
   ```bash
   cd d:\项目\agent\wf-agent
   pnpm rebuild better-sqlite3
   ```

3. **Verify Fix**
   ```bash
   cd packages/storage
   pnpm test src/sqlite/__tests__/base-sqlite-storage.test.ts
   ```

### Priority 2: Document Environment Requirements

Add to `AGENTS.md` or create `.nvmrc`:
```markdown
## Native Module Requirements

For SQLite storage functionality, ensure:
- Visual Studio Build Tools 2019+ installed (Windows)
- Python 3.x installed
- Node.js v22.14.0 with matching ABI

Run `pnpm rebuild` after:
- Node.js version changes
- Initial repository clone
- Dependency updates
```

### Priority 3: Add CI/CD Safeguards

Update GitHub Actions or CI configuration:
```yaml
# .github/workflows/test.yml
- name: Rebuild native modules
  run: pnpm rebuild
  
- name: Run storage tests
  run: |
    cd packages/storage
    pnpm test --reporter=verbose
```

---

## Long-term Recommendations

### 1. Abstract Storage Backend

Create a storage abstraction layer that allows easy switching between backends:

```typescript
interface StorageBackend {
  initialize(): Promise<void>;
  save(id: string, data: Uint8Array, metadata: any): Promise<void>;
  load(id: string): Promise<Uint8Array | null>;
  // ... other methods
}

class SqliteBackend implements StorageBackend { /* ... */ }
class JsonBackend implements StorageBackend { /* ... */ }
class InMemoryBackend implements StorageBackend { /* ... */ }
```

**Benefits**:
- Easier testing (use in-memory backend)
- Flexible deployment options
- Reduced coupling to specific libraries

### 2. Implement Fallback Strategy

```typescript
class ResilientStorage {
  private primary: StorageBackend;
  private fallback: StorageBackend;
  
  async save(id: string, data: Uint8Array, metadata: any): Promise<void> {
    try {
      await this.primary.save(id, data, metadata);
    } catch (error) {
      console.warn('Primary storage failed, using fallback');
      await this.fallback.save(id, data, metadata);
    }
  }
}
```

### 3. Add Health Checks

```typescript
async function checkStorageHealth(): Promise<StorageHealthReport> {
  return {
    sqlite: await testSqliteConnection(),
    json: await testJsonStorage(),
    nativeModules: await checkNativeBindings(),
  };
}
```

### 4. Consider WASM-based SQLite

Explore [wa-sqlite](https://github.com/rhashimoto/wa-sqlite) or similar WebAssembly-based SQLite implementations:
- No native compilation required
- Cross-platform compatibility
- Runs in browsers and Node.js

---

## Related Files

### Core Implementation
- `packages/storage/src/sqlite/base-sqlite-storage.ts` - Base class
- `packages/storage/src/sqlite/sqlite-checkpoint-storage.ts` - Checkpoint storage
- `packages/storage/src/sqlite/sqlite-task-storage.ts` - Task storage
- `packages/storage/src/sqlite/sqlite-workflow-storage.ts` - Workflow storage

### Configuration
- `packages/storage/package.json` - Dependencies
- `configs/database/database.toml` - Database configuration

### Tests
- `packages/storage/src/sqlite/__tests__/base-sqlite-storage.test.ts`
- `packages/storage/src/sqlite/__tests__/sqlite-checkpoint-storage.test.ts`
- `packages/storage/src/sqlite/__tests__/sqlite-task-storage.test.ts`
- `packages/storage/src/sqlite/__tests__/sqlite-workflow-storage.test.ts`

---

## References

- [better-sqlite3 Documentation](https://github.com/WiseLibs/better-sqlite3)
- [Node.js ABI Versions](https://nodejs.org/en/download/releases/)
- [pnpm Native Modules](https://pnpm.io/cli/rebuild)
- [Node-API (N-API)](https://nodejs.org/api/n-api.html)

---

## Status

- **Issue Created**: 2026-04-30
- **Severity**: Medium (blocks SQLite tests, doesn't affect JSON storage)
- **Status**: Investigation Complete, Awaiting Fix Implementation
- **Assigned To**: Development Team
- **Priority**: P2 (Should fix before next release)

---

## Notes

This issue was discovered during the storage compression refactoring task. While the compression module itself works perfectly (32/32 tests passing), the SQLite binding issue prevented comprehensive testing of the full storage system.

The JSON storage backend remains fully functional and tested, providing a viable alternative for environments where SQLite native modules cannot be built.
