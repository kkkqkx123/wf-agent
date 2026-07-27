# 沙箱系统架构

## 一、概述

沙箱系统位于 `crates/wf-sandbox`，为所有语言脚本提供隔离执行能力。
类型定义位于 `crates/wf-types/src/script/sandbox.rs`。
配置处理位于 `crates/wf-config/src/processor/infrastructure.rs`。

### 设计原则

- **分层架构**：Policy → Strategy → Executor → Runtime
- **策略可组合**：全局默认策略 + Profile 规则 + 执行时覆盖
- **优先级回退**：首选策略不可用时自动按优先级降级
- **宽松模式**：严格模式拒绝违规，宽松模式仅记录

### 依赖关系

```
wf-sandbox
├── wf-types   ← 所有沙箱类型定义
├── wf-common  ← 通用工具
├── tokio      ← 异步进程管理
├── libc       ← seccomp BPF (Linux)
├── mlua       ← Lua VM 嵌入 (可选 feature)
├── regex      ← 静态分析
└── serde_json ← Python/JS 代码生成
```

---

## 二、四层架构

```
┌──────────────────────────────────────────────────────┐
│   SandboxRuntime                                     │
│   ┌────────────────────────────────────────────────┐ │
│   │  Policy Layer                                  │ │
│   │  SandboxPolicyManager::merge(default, config)  │ │
│   │  default_sandbox_policy()  ← 全局默认策略      │ │
│   └──────────┬─────────────────────────────────────┘ │
│   ┌──────────▼─────────────────────────────────────┐ │
│   │  Strategy Layer                                │ │
│   │  StrategyResolver::resolve_best(lang, ids)      │ │
│   │  DefaultStrategyResolver  ← 按优先级回退       │ │
│   └──────────┬─────────────────────────────────────┘ │
│   ┌──────────▼─────────────────────────────────────┐ │
│   │  Executor Layer                                │ │
│   │  Shell / Python / JavaScript / Lua Executor    │ │
│   │  (薄包装，委托给 SandboxRuntime.execute)        │ │
│   └──────────┬─────────────────────────────────────┘ │
│   ┌──────────▼─────────────────────────────────────┐ │
│   │  Strategy Implementation Layer                 │ │
│   │  Shell: static-analyzer / os-hook (seccomp)    │ │
│   │  Python: builtin-hook / ast-analyzer / os-hook │ │
│   │  JS: vm-context / subprocess / os-hook         │ │
│   │  Lua: mlua-sandbox / static-analyzer           │ │
│   └────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

### 2.1 Policy 层

```rust
// crates/wf-types/src/script/sandbox.rs

SandboxMode       → Disabled | Lenient | Strict | Custom
FilesystemPolicy  → 路径读写白名单 + CoW + 文件大小限制
ProcessPolicy     → 子进程白名单/黑名单 + fork/exec 控制
NetworkPolicy     → None | Localhost | Specific | All
ResourcePolicy    → CPU/内存/磁盘/超时限制
ShellPolicy       → 命令白名单/黑名单 + 危险模式 + pipe/redirect
PythonPolicy      → 模块白名单/黑名单 + subprocess + open + eval
JavaScriptPolicy  → 模块白名单/黑名单 + FS写入 + child_process + eval
LuaPolicy         → 模块白名单/黑名单 + os.execute + io.open + 动态加载

SandboxPolicy     → 聚合以上所有子策略 + mode
SandboxConfig     → 用户配置（mode + policy override + 策略选择 + VFS + 遗留字段）
```

策略合并规则：`SandboxPolicyManager::merge(base, overrides)` — 覆盖字段优先，缺失字段从 base 继承。

### 2.2 Strategy 层

```rust
// crates/wf-sandbox/src/resolver.rs

trait StrategyImplementation {
    fn id(&self) -> &str;          // 唯一标识
    fn priority(&self) -> i32;     // 数字越高优先级越高
    fn is_available(&self) -> bool;
    async fn execute(&self, options, policy) -> Result<ScriptExecutionResult>;
}

trait StrategyResolver {
    fn resolve_best(&self, language, preferred_ids) -> Option<Arc<dyn StrategyImplementation>>;
    fn register_strategy(&mut self, language, strategy);
}

DefaultStrategyResolver:
  是否定 preferred_ids？→ 按顺序尝试 → 全部不可用？→ 按 priority 降序尝试所有注册策略
```

注册的全部 11 个策略：

| 语言 | 策略 ID | 优先级 | 说明 |
|------|---------|--------|------|
| shell | static-analyzer | 10 | 正则命令分析 |
| shell | os-hook | 50 | 真实 seccomp BPF |
| python | builtin-hook | 20 | 生成安全包围代码 |
| python | ast-analyzer | 15 | Python AST 真实解析 |
| python | os-hook | 30 | 子进程直接执行 |
| javascript | vm-context | 25 | 生成安全包裹代码 |
| javascript | subprocess | 20 | node --eval |
| javascript | os-hook | 30 | 子进程直接执行 |
| lua | mlua-sandbox | 100 | 内嵌 VM API 隔离 |
| lua | static-analyzer | 10 | 正则分析回退 |

### 2.3 Executor 层

4 个薄包装器，均为 23 行，直接委托给 `SandboxRuntime::execute`：

- `SandboxShellExecutor` → `execute("shell", code, config)`
- `SandboxPythonExecutor` → `execute("python", code, config)`
- `SandboxJavaScriptExecutor` → `execute("javascript", code, config)`
- `SandboxLuaExecutor` → `execute("lua", code, config)`

### 2.4 Runtime 层

`SandboxRuntime` 是核心入口，执行流程：

```
execute(language, command, config):
  1. 确定 mode：config.mode → default_policy.mode → Strict
  2. mode == Disabled? → execute_direct (sh -c 直接执行)
  3. 合并策略：SandboxPolicyManager::merge(&default, &config.policy)
  4. 解析首选策略 ID：按语言从 config 获取 shell_strategy / python_strategy / ...
  5. Resolve 最佳策略：resolver.resolve_best(language, preferred_ids)
  6. 构建 StrategyExecuteOptions（含 VFS）
  7. 执行策略：strategy.execute(options, &merged_policy)
  8. 宽松模式：将 error 转为 violations，设置 success = true
  9. 返回 ScriptExecutionResult
```

---

## 三、策略实现详情

### 3.1 Shell static-analyzer

文件：`src/strategy/shell/static_analyzer.rs`

4 层分析：
- **Layer 0**：全局危险模式正则匹配（`rm -rf`，fork bomb 等）
- **Layer 0**：管道操作符检查（`|`）
- **Layer 1**：命令链解析（`;` `&&` `||` `|`），子命令白名单/黑名单
- **Layer 2**：VFS 路径检查（存根，VFS 未完整集成）

### 3.2 Shell os-hook (Seccomp BPF)

文件：`src/strategy/shell/os_hook.rs`

真实 Linux seccomp-bpf 系统调用过滤（黑名单方案）：

```
pre_exec (fork 后子进程):
  1. prctl(PR_SET_NO_NEW_PRIVS, 1) — 防止绕过
  2. syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0, &bpf_prog)
  3. execve("/bin/sh", ["sh", "-c", command])

BPF 过滤器: LD nr → JEQ 链匹配拒绝列表 → 默认 ALLOW
```

拒绝的 syscall 类别（约 45 个）：
- 调试：`ptrace`, `process_vm_readv/writev`
- 内核：`bpf`, `kexec`, `init_module`, `delete_module`
- 系统管理：`mount`, `umount2`, `chroot`, `pivot_root`, `swapon`, `swapoff`, `reboot`
- 硬件：`iopl`, `ioperm`
- 权限：`setuid`, `setgid`, `setreuid`, `setresuid`
- 性能：`perf_event_open`
- 网络（当 policy 禁止时）：`socket`, `connect`, `sendto`, `recvfrom` 等 18 个

### 3.3 Python builtin-hook

文件：`src/strategy/python/builtin_hook.rs`

生成长包裹 Python 代码注入 `python3 -c`：

```python
# 生成的包裹代码
_sys.path.clear()
_original_import = __builtins__.__import__
# 安全 import：白名单 + 黑名单检查
def _safe_import(name, *args, **kwargs):
    if _denied_modules and base in _denied_modules: raise ImportError
    if _allowed_modules and base not in _allowed_modules: raise ImportError
    return _original_import(name, *args, **kwargs)
__builtins__.__import__ = _safe_import

# 安全 open：写模式拦截
_safe_open: if 'w'/'a'/'+' in mode → PermissionError

# 禁用动态执行
__builtins__.eval = None
__builtins__.exec = None
__builtins__.compile = None

# 用户代码
{USER_CODE}
```

### 3.4 Python ast-analyzer

文件：`src/strategy/python/ast_analyzer.rs`

通过子进程调用 Python `ast` 模块真实解析 AST：

```python
tree = ast.parse(code)
for node in ast.walk(tree):
    if isinstance(node, ast.Import):  # 检查模块名
    if isinstance(node, ast.Call):    # 检测 eval/exec/open/子进程
return JSON({"safe": bool, "violations": [...]})
```

### 3.5 JavaScript vm-context

文件：`src/strategy/js/vm_context.rs`

生成长包裹 JS 代码注入 `node --eval`：

```javascript
(function() {
    const MODULE_ALLOWLIST = new Set([...]);
    const MODULE_DENYLIST = new Set([...]);
    
    function safeRequire(name) {
        if (MODULE_DENYLIST.has(name)) throw Error;
        if (MODULE_ALLOWLIST.size > 0 && !MODULE_ALLOWLIST.has(name)) throw Error;
        if (!ALLOW_CHILD_PROCESS && name === 'child_process') throw Error;
        if (name === 'fs') {
            const fs = require('fs');
            return new Proxy(fs, {
                get(target, prop) {
                    if (!ALLOW_FS_WRITE && writeOps.has(prop))
                        return () => { throw Error };
                    if (readOnly.has(prop)) return target[prop];
                    ...
                }
            });
        }
        return require(name);
    }
    
    if (!ALLOW_DYNAMIC_EVAL) {
        global.eval = undefined;
        globalThis.Function = undefined;
    }
    global.require = safeRequire;
    globalThis.require = safeRequire;
    
    // 用户代码在此上下文中执行
    var userCode = function(require, global, globalThis) { ... };
    userCode(safeRequire, safeGlobal, safeGlobal);
})();
```

### 3.6 Lua mlua-sandbox (Rust 原生优势)

文件：`src/strategy/lua/mlua_sandbox.rs`

使用 `mlua` crate 内嵌 Lua VM，真正的 API 级隔离：
- 每次执行创建全新 `Lua::new()` 状态
- 清理危险全局变量：`os`/`io`/`package`/`debug`/`ffi` 设为 `nil`
- 替换 `print` 为安全版本
- 替换 `require` 为白名单/黑名单过滤版
- 通过 `tokio::task::spawn_blocking` 调用

### 3.7 Lua static-analyzer

文件：`src/strategy/lua/static_analyzer.rs`

正则检测 `os\.execute`/`io\.popen`/`loadstring`/`load`/`dofile`。
通过后委托 `lua -e` 子进程执行。

---

## 四、VFS 子系统

文件：`src/vfs/`

```
OverlayVFS          → 两层文件系统：MemoryDelta 覆盖层 + HostFS 基础层
MemoryDelta         → HashMap<PathBuf, Vec<u8>> 写时复制
WhiteoutCache       → HashSet<PathBuf> 删除标记
HostFS              → 基于根路径的本地文件系统访问
```

VFS 已通过 config 集成到 `SandboxRuntime`（`options.vfs`），但策略层中仅 Shell static-analyzer 有 VFS 检查存根。
`VfsProvider` trait 提供 `read_file`/`write_file`/`exists` 异步接口。

---

## 五、安全验证器

文件：`crates/wf-sandbox/src/security.rs`

```rust
SecurityValidator:
  validate_expression(expr) → Vec<SecurityViolation>
    - 最大长度 1000
    - 禁止 __proto__/constructor/prototype
    - 禁止连续点号
    - 括号嵌套深度 ≤ 10

  validate_path(path) → Vec<SecurityViolation>
    - 禁止 ../ 目录遍历
    - 禁止 // 空组件
    - 禁止空字节

  validate_array_index(index, length) → Vec<SecurityViolation>
    - 非负
    - 不越界

  validate_value_type(value) → Vec<SecurityViolation>
    - 递归检查 JSON 值，拒绝函数/类实例
```

---

## 六、配置处理

文件：`crates/wf-config/src/processor/infrastructure.rs`

```rust
merge_sandbox_with_defaults(user: &SandboxConfig) → SandboxConfig
  - mode 默认: Strict
  - resource_limits 默认: memory=512MB, disk=1024MB

validate_sandbox_config(config: &SandboxConfig) → ConfigResult<()>
  - resource_limits.memory ≥ 1
  - resource_limits.disk ≥ 1
```

---

## 七、类型补全（Profile 系统）

文件：`crates/wf-types/src/script/sandbox.rs` 新增类型：

```rust
SandboxProfile        → name + 策略选择 + policy + VFS 配置
SandboxProfileRule    → match_field + match_pattern → profile
SandboxGlobalConfig   → mode + profiles[] + rules[] + default_profile
AuditEvent            → 安全审计事件
ScriptMetadata        → 脚本元数据
SecurityViolation     → 安全违规记录
```

Profile 解析器 `SandboxProfileResolver`（尚未实现，定义在补全计划中）。

---

## 八、与 TypeScript 参考实现对比

| 维度 | TS 实现 | Rust 实现 | 状态 |
|------|---------|-----------|------|
| Shell static-analyzer | 4 层 + bash/powershell/cmd 解析 | 4 层 + 通用解析 | ✅ 等价 |
| Shell os-hook (seccomp) | `seccomp-loader` 二进制 | 真实 BPF 过滤 (libc) | 🏆 RS 更优 |
| Python builtin-hook | 生成包围代码 | 生成包围代码 | ✅ 等价 |
| Python ast-analyzer | ast 子进程 JSON 分析 | ast 子进程 JSON 分析 | ✅ 等价 |
| JS vm-context | node:vm 上下文隔离 | 生成包裹 JS 代码 | ✅ 等价 |
| Lua mlua-sandbox | 子进程 + 字符串注入 | **内嵌 VM API 隔离** | 🏆 RS 唯一 |
| Lua static-analyzer | 18 模式 | 5 模式 | ⚠️ RS 较简单 |
| VFS + MountTable | MountTable + PathMapper | OverlayVFS + MemoryDelta | ⚠️ 架构不同 |
| 运行时集成 | 完整 (script-handler) | 仅核心 Runtime | ❌ 待 wf-core 集成 |
| 配置文件/profile | SandboxGlobalConfig + 解析 | 类型已定义，解析器未实现 | ❌ 待实现 |
| 交互式脚本 | 3 模式协调器 + PTY | 未实现 | ❌ 待实现 |

## 九、测试覆盖

```
wf-sandbox: 46 测试
  ├── DefaultPolicy:       3
  ├── Policy (merge):      2
  ├── Resolver:            3
  ├── Runtime:             1
  ├── Security:           11
  ├── Shell static-anal:   4
  ├── Shell os-hook:       6
  ├── Python builtin-hook: 4
  ├── Python ast-analyzer: 4
  ├── JS vm-context:       5
  ├── Lua static-anal:     1
  └── VFS overlay:         2

wf-config:   88 测试（含沙箱配置 2）
总计:       134 测试
```

## 十、文件清单

```
crates/wf-sandbox/src/
├── lib.rs                   ← 模块声明 + 公共导出
├── runtime.rs               ← SandboxRuntime 协调器 (217 行)
├── resolver.rs              ← StrategyResolver + DefaultStrategyResolver (309 行)
├── policy.rs                ← SandboxPolicyManager 合并/默认 (140 行)
├── default_policy.rs        ← DEFAULT_SANDBOX_POLICY 常量 (111 行)
├── security.rs              ← SecurityValidator (235 行)
├── executor/
│   ├── shell_executor.rs    ← SandboxShellExecutor
│   ├── python_executor.rs   ← SandboxPythonExecutor
│   ├── js_executor.rs       ← SandboxJavaScriptExecutor
│   └── lua_executor.rs      ← SandboxLuaExecutor
├── strategy/
│   ├── container.rs         ← Docker CLI (is_available=false)
│   ├── shell.rs             ← mod 声明
│   │   ├── static_analyzer.rs (316 行)
│   │   └── os_hook.rs       ← 真实 seccomp BPF (372 行)
│   ├── python.rs            ← mod 声明
│   │   ├── builtin_hook.rs  ← 生成包围代码 (206 行)
│   │   ├── ast_analyzer.rs  ← 真实 AST 解析 (302 行)
│   │   └── os_hook.rs       ← 直接 python3 -c
│   ├── js.rs                ← mod 声明
│   │   ├── vm_context.rs    ← JS 包裹代码 (330 行)
│   │   ├── subprocess.rs    ← node --eval
│   │   └── os_hook.rs       ← 直接 node --eval
│   └── lua.rs               ← mod 声明
│       ├── mlua_sandbox.rs  ← mlua 内嵌 VM (feature-gated)
│       └── static_analyzer.rs (148 行)
└── vfs/
    ├── overlay.rs           ← OverlayVFS (132 行)
    ├── delta.rs             ← MemoryDelta HashMap 包装
    ├── whiteout.rs          ← WhiteoutCache HashSet 包装
    └── base.rs              ← HostFS 根路径访问

crates/wf-types/src/script/
└── sandbox.rs               ← 全部类型定义 (260 行)
