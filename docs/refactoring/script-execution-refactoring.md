# Script Execution Refactoring - Migration Guide

## Overview

This document describes the refactoring of script execution in the WF-Agent framework. The `packages/script-executors` package has been removed and script execution now uses the Terminal Service directly.

## What Changed

### 1. Removed `packages/script-executors` Package

The entire `script-executors` package has been deleted. This included:
- `GenericProcessExecutor`
- `BaseScriptExecutor`
- Command strategies
- Retry and timeout controllers (moved to common-utils if needed)

### 2. Simplified Script Execution

**Before:**
```typescript
// Complex executor hierarchy with type-specific executors
import { GenericProcessExecutor } from "@wf-agent/script-executors";

const executor = new GenericProcessExecutor();
const result = await executor.execute(script, options);
```

**After:**
```typescript
// Simple execution using Terminal Service
import { getTerminalService } from "@wf-agent/sdk/services";

const terminalService = getTerminalService();
const result = await terminalService.executeOneOff(command, options);
```

### 3. Deprecated ScriptType Distinctions

Script types (`SHELL`, `CMD`, `POWERSHELL`, `PYTHON`, `JAVASCRIPT`) are now deprecated. All scripts are treated as shell commands.

**Before:**
```toml
# configs/scripts/inline/hello-world.toml
name = "hello"
script_type = "SHELL"
content = "echo Hello World"
```

**After:**
```toml
# Scripts are just shell commands - specify interpreter in the command
name = "hello"
content = "echo Hello World"

# For Python scripts
name = "python-script"
content = "python3 script.py"

# For JavaScript
name = "js-script"  
content = "node script.js"

# For PowerShell
name = "ps-script"
content = "pwsh -Command 'Write-Host Hello'"
```

## Migration Steps

### For Script Definitions

If you have existing scripts with specific types, update them to include the interpreter in the command:

**Python Scripts:**
```toml
# Before
script_type = "PYTHON"
content = "print('Hello')"

# After
content = "python3 -c \"print('Hello')\""
```

**JavaScript Scripts:**
```toml
# Before
script_type = "JAVASCRIPT"
content = "console.log('Hello')"

# After
content = "node -e \"console.log('Hello')\""
```

**PowerShell Scripts:**
```toml
# Before
script_type = "POWERSHELL"
content = "Write-Host 'Hello'"

# After (Windows)
content = "powershell -Command \"Write-Host 'Hello'\""

# Or (Cross-platform with pwsh)
content = "pwsh -Command \"Write-Host 'Hello'\""
```

### For Custom Executors

If you were extending `BaseScriptExecutor`:

**Before:**
```typescript
import { BaseScriptExecutor } from "@wf-agent/script-executors";

class CustomExecutor extends BaseScriptExecutor {
  protected async doExecute(script: Script, context?: ExecutionContext) {
    // Custom execution logic
  }
}
```

**After:**
```typescript
import { getTerminalService } from "@wf-agent/sdk/services";

class CustomExecutor {
  private terminalService = getTerminalService();

  async execute(command: string, options?: any) {
    // Use terminal service directly
    return await this.terminalService.executeOneOff(command, options);
  }
}
```

### For Script Registry Usage

No changes needed! The `ScriptRegistry` API remains the same:

```typescript
import { getContainer } from "@wf-agent/sdk/core/di/container-config.js";
import * as Identifiers from "@wf-agent/sdk/core/di/service-identifiers.js";

const container = getContainer();
const scriptRegistry = container.get(Identifiers.ScriptRegistry);

// Still works the same way
const result = await scriptRegistry.execute("my-script", options);
```

## Benefits

### 1. Simplified Architecture
- ✅ One less package to maintain
- ✅ Single execution path (Terminal Service)
- ✅ Reduced code duplication

### 2. Better Shell Support
- ✅ Automatic shell detection
- ✅ Cross-platform compatibility
- ✅ Session management for stateful operations

### 3. More Flexibility
- ✅ Any command can be executed (not limited by script type)
- ✅ Easy to specify interpreters inline
- ✅ No artificial type restrictions

### 4. Easier to Understand
- ✅ Scripts are just shell commands
- ✅ No complex executor hierarchy
- ✅ Clear execution flow

## Backward Compatibility

The `ScriptType` enum is kept but marked as `@deprecated`. Existing scripts will continue to work:
- The `type` field is ignored during execution
- All scripts default to `'SHELL'` type in results
- Type validation is disabled

## Timeline

- **Immediate**: Package removed, new executor in place
- **Short-term**: Update documentation and examples
- **Long-term**: Consider removing `ScriptType` entirely in next major version

## Examples

### Example 1: Simple Shell Command

```toml
# configs/scripts/inline/greet.toml
name = "greet"
description = "Greet the user"
content = "echo Hello, World!"
```

### Example 2: Python Script

```toml
# configs/scripts/inline/process-data.toml
name = "process-data"
description = "Process data with Python"
content = """
python3 << 'EOF'
import json
data = {"message": "Hello from Python"}
print(json.dumps(data))
EOF
"""
```

### Example 3: Node.js Script

```toml
# configs/scripts/inline/fetch-data.toml
name = "fetch-data"
description = "Fetch data using Node.js"
content = """
node -e "
const https = require('https');
https.get('https://api.example.com', (res) => {
  let data = '';
  res.on('data', chunk => data += chunk);
  res.on('end', () => console.log(data));
});
"
"""
```

### Example 4: Multi-line PowerShell

```toml
# configs/scripts/inline/windows-task.toml
name = "windows-task"
description = "Run Windows task"
content = """
pwsh -Command "
Get-Process | Where-Object { $_.CPU -gt 10 } | Select-Object Name, CPU
"
"""
```

## FAQ

### Q: Why remove script types?
A: Script types added unnecessary complexity. A Python script is just a shell command that runs `python`. By treating everything as shell commands, we simplify the architecture while maintaining full functionality.

### Q: Can I still run Python/JavaScript/etc.?
A: Yes! Just include the interpreter in your command:
- Python: `python3 script.py` or `python3 -c "code"`
- JavaScript: `node script.js` or `node -e "code"`
- PowerShell: `pwsh -Command "code"`

### Q: What about file-based scripts?
A: File paths work the same way:
```toml
content = "python3 /path/to/script.py"
```

### Q: Is there any performance impact?
A: No. The Terminal Service uses the same `child_process.spawn()` under the hood. Performance is identical or slightly better due to reduced abstraction layers.

### Q: How do I handle retries?
A: Implement retry logic at the application level or use the SDK's workflow retry mechanisms. The simple execution model makes it easier to add custom retry logic when needed.

## Need Help?

If you encounter issues during migration:
1. Check this guide for examples
2. Review the Terminal Service documentation
3. Open an issue on GitHub with your specific use case
