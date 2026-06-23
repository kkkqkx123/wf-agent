# SQLite Native Module Fix - Solution Summary

## Problem
SQLite native module binding failures on Windows with Node.js v22.14.0 (ABI 127):
```
Error: Could not locate the bindings file.
```

see [sqlite-binding-issue](../issue/sqlite-native-module-binding-issue.md)

## Root Cause
The `better-sqlite3@12.6.2` package did not have prebuilt binaries for Node.js v22 (ABI 127), and the native module was not compiled during installation.

## Solution Applied

### Step 1: Rebuild Native Module
```bash
cd packages/storage
npm rebuild better-sqlite3 --build-from-source
```

This command:
- Compiled the C++ native bindings from source
- Created `better_sqlite3.node` in `build/Release/`
- Matched the binary to Node.js v22.14.0 (ABI 127)

### Step 2: Verification
```bash
# Test native module loading
node -e "const Database = require('better-sqlite3'); const db = new Database(':memory:'); console.log('✓ Works!'); db.close();"

# Run all SQLite tests
pnpm test src/sqlite/__tests__/
```

## Important

1. **Native Module Requirements**: better-sqlite3 requires platform-specific `.node` files compiled via node-gyp
2. **Windows Build Tools**: Visual Studio Build Tools must be installed for compilation
3. **Path Issues**: Paths with spaces or non-ASCII characters can cause compilation failures
4. **Prebuilt Binaries**: Not all Node.js versions have prebuilt binaries available

## Prevention for Future

If you encounter this issue again:

### Quick Fix
```bash
cd packages/storage
npm rebuild better-sqlite3 --build-from-source
```

### If Rebuild Fails
1. Ensure Visual Studio Build Tools are installed:
   ```powershell
   & "$env:ProgramFiles\nodejs\install_tools.bat"
   ```

2. Clean reinstall:
   ```bash
   rm -rf node_modules
   pnpm install
   npm rebuild better-sqlite3 --build-from-source
   ```

3. Consider moving project to ASCII-only path if compilation fails due to Chinese characters in path

## Environment Details
- **OS**: Windows 25H2
- **Node.js**: v22.14.0 (ABI 127)
- **Platform**: win32-x64
- **better-sqlite3**: v12.6.2
- **Package Manager**: pnpm

## Date Fixed
2026-04-30
