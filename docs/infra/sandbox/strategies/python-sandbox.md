# Python 沙箱策略

## 概述

Python 脚本执行是最高风险的场景之一。通过 `import os; os.remove('/etc')` 可以完全绕过 shell 命令拦截。

提供两种轻量级策略 + 两种进阶策略, 按优先级选择：

| 策略 | 标识 | 级别 | 原理 |
|------|------|------|------|
| Builtin Hook | `builtin-hook` | 轻量 | 生成受限 Python 脚本, 替换 builtins |
| AST Analyzer | `ast-analyzer` | 轻量 | AST 解析 → 高危检测 → 受限执行 |
| OS Hook | `os-hook` | OS级 | 通过 OS 机制限制 Python 进程 |
| Pyodide WASM | `pyodide-wasm` | 重量 | WASM 运行时, 进程内执行 |

## 策略一: builtin-hook

### 原理

生成一个新的 Python 脚本, 在其中注入安全限制代码, 然后通过子进程执行：

```
原始脚本:
  import os; os.remove("/etc/passwd")

包装后脚本:
  import sys
  sys.path = []                        # 清空路径
  sys.modules["os"] = None             # 禁用 os 模块
  
  import builtins
  _original_open = builtins.open
  def _safe_open(path, mode='r', ...):
      if 'w' in mode and not path.startswith('/workspace/'):
          raise PermissionError(f"Write denied: {path}")
      return _original_open(path, mode, ...)
  builtins.open = _safe_open
  
  # 禁用高危模块
  _denied = ['os', 'subprocess', 'shutil', 'ctypes', 'socket']
  _original_import = builtins.__import__
  def _safe_import(name, *args, **kwargs):
      if name in _denied:
          raise ImportError(f"Module denied: {name}")
      return _original_import(name, *args, **kwargs)
  builtins.__import__ = _safe_import
  
  # ——— 用户代码 ———
  import os; os.remove("/etc/passwd")  # 执行到 os.remove 前会失败
```

### 核心代码

```typescript
export class PythonBuiltinHookStrategy implements StrategyImplementation<...> {
  id = "builtin-hook";
  priority = 20;

  isAvailable(): boolean {
    // 需要本机安装 Python
    return this.checkPythonAvailable();
  }

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    const code = options.command;
    const pyPolicy = policy.python || DEFAULT_PYTHON_POLICY;

    // 1. 生成包装脚本
    const wrappedCode = this.wrapWithSandbox(code, pyPolicy);

    // 2. 写入临时文件
    const tmpFile = await writeTempScript(wrappedCode, ".py");

    // 3. 受限子进程执行
    return this.executePython(tmpFile, {
      timeout: policy.resource?.timeoutLimit,
      env: {
        ...options.env,
        PYTHONPATH: "",      // 清空 Python 路径
        PYTHONDONTWRITEBYTECODE: "1",
      },
    });
  }
}
```

### 限制

- 不能防范 `python -c` 内联绕过 (可以在内联代码中先还原 builtins)
- Python 子进程内的 `subprocess.Popen("rm -rf /etc", shell=True)` 无法拦截 (除非 deny subprocess)

## 策略二: ast-analyzer

### 原理

在 builtin-hook 之上增加 AST 预分析。在代码执行前先解析 Python 语法树, 检查高危调用：

```python
import ast

tree = ast.parse(code)
for node in ast.walk(tree):
    # 检测: import os, from os import remove
    if isinstance(node, (ast.Import, ast.ImportFrom)):
        if node.module in DENIED_MODULES:
            raise SecurityError(f"Module denied: {node.module}")
    
    # 检测: os.remove(...) → Attribute(os, remove) → Call
    if isinstance(node, ast.Call):
        if isinstance(node.func, ast.Attribute):
            if node.func.attr in DENIED_FUNCTIONS:
                # 检查调用者是否为 os/sys/subprocess 等
                if isinstance(node.func.value, ast.Name):
                    if node.func.value.id in DENIED_MODULES:
                        raise SecurityError(...)
    
    # 检测: eval(), exec(), compile(), open(mode='w')
    if isinstance(node, ast.Call):
        if isinstance(node.func, ast.Name):
            if node.func.id in DENIED_BUILTINS:
                raise SecurityError(...)
```

### 核心代码

```typescript
export class PythonASTAnalyzerStrategy implements StrategyImplementation<...> {
  id = "ast-analyzer";
  priority = 25; // 比 builtin-hook 优先级高

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    const code = options.command;

    // AST 分析 (通过子进程运行 Python ast 模块)
    const analysis = await this.analyzeAST(code, policy.python);

    if (!analysis.safe) {
      return {
        success: false,
        error: `Security violation: ${analysis.violations.join(", ")}`,
      };
    }

    // 分析通过后, 同样用 builtin-hook 方式执行
    return this.executeWithBuiltinHook(code, policy);
  }

  private async analyzeAST(
    code: string,
    policy: PythonPolicy
  ): Promise<{ safe: boolean; violations: string[] }> {
    // 生成 AST 分析脚本
    const analyzerScript = `
import ast, sys
code = sys.stdin.read()
tree = ast.parse(code)
# ... 安全检查逻辑 ...
print(json.dumps({ "safe": true/false, "violations": [...] }))
`;

    const result = await execPython(analyzerScript, { input: code });
    return JSON.parse(result.stdout);
  }
}
```

### AST 无法检测的绕过

- 动态 `getattr(__import__('os'), 'remove')('/etc/passwd')`
- `compile()` + `exec()` 动态构造
- 字符串拼接: `__import__('o' + 's')`
- `sys.modules` 手动恢复

## 策略三: pyodide-wasm (可选增强)

### 原理

使用 Pyodide (Python WASM 运行时) 在 Node.js 进程内执行 Python 代码, 完全不接触 OS 文件系统。

```typescript
export class PyodideWasmStrategy implements StrategyImplementation<...> {
  priority = 5; // 优先级低, 仅作为备选

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    // 使用 shared Pyodide 实例
    if (!this.pyodide) {
      this.pyodide = await loadPyodide({
        indexURL: "https://cdn.jsdelivr.net/pyodide/v0.25.0/full/",
      });
    }

    // 设置受限 FS
    this.pyodide.FS.mkdir("/workspace");
    this.pyodide.FS.mount(
      this.pyodide.FS.filesystems.WORKERFS,
      { files: [], dirs: [] },
      "/workspace"
    );

    // 执行 (无 OS 访问能力)
    const result = await this.pyodide.runPythonAsync(options.command);
    return { success: true, stdout: String(result) };
  }
}
```

### 限制

- 需要网络加载 Pyodide WASM 包 (~12MB)
- Python 标准库不全 (部分 C 扩展模块不可用)
- 不能访问本机文件系统 (对需要读写文件的工作流不适用)