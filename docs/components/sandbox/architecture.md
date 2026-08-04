# 沙箱系统架构

## 一、概述

沙箱系统位于 `crates/wf-sandbox`，为所有语言脚本提供隔离执行能力。
类型定义位于 `crates/wf-types/src/script/sandbox.rs`。
配置处理位于 `crates/wf-config/src/processor/infrastructure.rs`。

### 设计原则

- **分层架构**：Policy → Strategy (链) → Runtime
- **策略可组合**：全局默认策略 + Profile 规则 + 执行时覆盖
- **显式有序策略链**：分析层门禁 + 执行层隔离串行纵深防御，无隐式 priority 回退
- **Fail-closed**：链中策略缺失/不可用时报错，不静默降级
- **宽松模式**：严格模式拒绝违规，宽松模式记录违规但仍真正执行
- **模式合并无哨兵**：`SandboxPolicy.mode` 为 `Option`，"未指定"继承 base，"显式 Strict"真实生效

### 依赖关系

```
wf-sandbox
├── wf-types   ← 所有沙箱类型定义
├── wf-common  ← 通用工具
├── tokio      ← 异步进程管理
├── libc       ← seccomp BPF (Linux)
├── mlua       ← Lua VM 嵌入 (可选 feature)
├── regex      ← 静态分析
├── shlex      ← shell 命令分词
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
│   │  Strategy Chain Layer                          │ │
│   │  StrategyResolver::resolve_chain(lang, ids)    │ │
│   │  配置链 / 每语言默认链 + fail-closed           │ │
│   │  ── 分析层全序通过 ──▶ 执行层取第一个可用 ──▶  │ │
│   └────────────────────────────────────────────────┘ │
│   ┌────────────────────────────────────────────────┐ │
│   │  Strategy Implementation Layer                 │ │
│   │  Analysis: shell static-analyzer / python      │ │
│   │            ast-analyzer / lua static-analyzer  │ │
│   │  Execution: shell os-hook (seccomp) / python   │ │
│   │            builtin-hook / js vm-context / lua  │ │
│   │            mlua-sandbox / direct(裸执行)       │ │
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

SandboxPolicy     → 聚合以上所有子策略 + mode(Option)
SandboxConfig     → 用户配置（mode + policy override + 策略链选择 + VFS + 遗留字段）
```

策略合并规则：`SandboxPolicyManager::merge(base, overrides)` — 覆盖字段优先，缺失字段从 base 继承。
`mode` 采用 `Option<SandboxMode>`：`None` 表示未指定（继承 base），`Some(Strict)` 是真实覆盖——消除了旧实现中"Strict 既是默认值又当哨兵"无法区分"未指定"与"明确 Strict"的问题。

### 2.2 Strategy 链层

```rust
// crates/wf-sandbox/src/resolver.rs

enum StrategyKind { Analysis, Execution }

trait StrategyImplementation {
    fn id(&self) -> &str;          // 唯一标识
    fn kind(&self) -> StrategyKind; // Analysis=门禁不执行 / Execution=真正执行
    fn is_available(&self) -> bool;
    async fn execute(&self, options, policy) -> Result<ScriptExecutionResult>;
}

trait StrategyResolver {
    fn resolve_chain(&self, language, preferred_ids) -> Result<Vec<Arc<dyn StrategyImplementation>>, String>;
    fn register_strategy(&mut self, language, strategy);
}
```

链语义：
- `preferred_ids` 非空时即链顺序；为空时使用每语言默认链常量。
- **未知策略 ID 直接报错（fail-closed）**，不再静默丢弃。
- Runtime 执行：分析层按链序全数通过 → 执行层按链序取第一个可用者运行。
- 执行层全部不可用时显式报错（附不可用 ID 列表）。

每语言默认链：

| 语言 | 默认链 | 说明 |
|------|--------|------|
| shell | `[static-analyzer, os-hook]` | tokenize 门禁 + seccomp BPF |
| python | `[ast-analyzer, builtin-hook]` | AST 门禁 + 内置函数挂钩 |
| javascript | `[vm-context]` | 包裹 require/fs 的 node 执行 |
| lua | `[mlua-sandbox, static-analyzer]` | 内嵌 VM + 标识符级 tokenize 门禁 |

注册的全部策略：

| 语言 | 策略 ID | kind | 说明 |
|------|---------|------|------|
| shell | static-analyzer | Analysis | 命令替换拒绝 + shlex 分词分析 + 读写路径检查（门禁，不执行） |
| shell | os-hook | Execution | 真实 seccomp BPF + rlimit，映射 network/process/filesystem 策略 |
| shell | container | Execution | Docker 容器隔离（`docker --version` 探测可用性） |
| python | ast-analyzer | Analysis | Python AST 真实解析（门禁，不执行） |
| python | builtin-hook | Execution | 生成安全包围代码 |
| python | direct | Execution | 裸 python3 -c，无隔离，仅显式配置可用 |
| javascript | vm-context | Execution | 生成安全包裹代码 |
| javascript | subprocess | Execution | node --eval |
| javascript | direct | Execution | 裸 node --eval，无隔离，仅显式配置可用 |
| lua | mlua-sandbox | Execution | 内嵌 VM API 隔离 |
| lua | static-analyzer | Analysis | 标识符级 tokenize 分析（门禁，不执行） |

### 2.3 Runtime 层

`SandboxRuntime` 是核心入口，执行流程：

```
execute(language, command, config):
  1. 确定 mode：config.mode → default_policy.mode → Strict
  2. mode == Disabled? → execute_direct (sh -c 直接执行)
  3. 合并策略：SandboxPolicyManager::merge(&default, &config.policy)
  4. 解析策略链：按语言取 shell_strategy / python_strategy / ...
     - 非空 = 链顺序；空 = 每语言默认链
  5. resolve_chain：未知 ID → 显式报错（fail-closed）
  6. 分析层（kind == Analysis，链序）：全数通过才继续
     - Strict：首个违规即拒绝
     - Lenient：记录 violation，继续交给执行层真正执行
     - 分析策略不可用 → 显式报错（不跳过门禁）
  7. 执行层（kind == Execution，链序）：取第一个可用者运行
     - 全部不可用 → 显式报错（附缺失 ID 列表）
  8. 统一经 execute_with_timeout 强制 timeout_limit_ms
  9. 返回 ScriptExecutionResult（Lenient 附加 analysis violations）
```

---

## 三、策略实现详情

### 3.1 Shell static-analyzer（分析门禁）

文件：`src/strategy/shell/static_analyzer.rs`

分析流水线（**仅门禁，不再执行命令**；实际执行由链中 Execution 策略完成）：
- **Layer 0**：全局危险模式正则匹配（`rm -rf`，fork bomb 等）
- **Layer 0**：管道操作符检查（`|`）
- **Layer 1**：命令替换拒绝——`$(...)` 与反引号命令替换直接拒绝（bash/powershell），防止 `echo $(rm -rf /)` 隐藏危险命令
- **Layer 2**：命令链解析（`;` `&&` `||` `|`，引号/反斜杠感知，保留原文供 shlex 重新分词），子命令经 `shlex` 分词后逐条白名单/黑名单分析
- **Layer 3**：VFS 路径检查——token 级提取读写路径（重定向目标按**写路径**校验，`>&2`/heredoc 不误判），`SecurityValidator` + `check_read`/`check_write` 双向校验

### 3.2 Shell os-hook (Seccomp BPF)

文件：`src/strategy/shell/os_hook.rs`

真实 Linux seccomp-bpf 系统调用过滤（黑名单方案）：

```
pre_exec (fork 后子进程):
  1. setrlimit(RLIMIT_AS)  — memory_limit_mb（policy.resource）
  2. setrlimit(RLIMIT_FSIZE) — max_file_size（policy.filesystem）
  3. prctl(PR_SET_NO_NEW_PRIVS, 1) — 防止绕过
  4. syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, 0, &bpf_prog)
  5. execve("/bin/sh", ["sh", "-c", command])

BPF 过滤器: LD nr → JEQ 链匹配拒绝列表 → 默认 ALLOW
```

拒绝的 syscall 类别（约 45 个基础项）：
- 调试：`ptrace`, `process_vm_readv/writev`
- 内核：`bpf`, `kexec`, `init_module`, `delete_module`
- 系统管理：`mount`, `umount2`, `chroot`, `pivot_root`, `swapon`, `swapoff`, `reboot`
- 硬件：`iopl`, `ioperm`
- 权限：`setuid`, `setgid`, `setreuid`, `setresuid`
- 性能：`perf_event_open`
- 网络（当 policy 禁止时）：`socket`, `connect`, `sendto`, `recvfrom` 等 18 个

**策略映射（P1 补齐）**：
- `process.allow_exec=false` → 拒绝 `execve`/`execveat`
- `process.allow_fork=false` → 拒绝 `fork`/`vfork`/`clone`/`clone3`
- `filesystem.allowed_write_paths` 为空 → 拒绝 fsMod 类系统调用（`unlink`/`unlinkat`/`rmdir`/`rename`/`mkdir`/`chmod` 等 28 个），从系统调用层兜住 `rm -rf`
- ShellPolicy 命令级管控由链中 static-analyzer 门禁承担；其系统级命令（chroot/mount/reboot/insmod 等）已由基础拒绝列表覆盖
- 默认策略 `allow_exec=true`、`allow_fork=true`，保证默认链可执行外部命令；用户显式收紧时系统调用层立即生效

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

### 3.4 Python ast-analyzer（分析门禁）

文件：`src/strategy/python/ast_analyzer.rs`

通过子进程调用 Python `ast` 模块真实解析 AST（**仅门禁，不执行用户代码**）：

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
- 通过 `tokio::task::spawn_blocking` 调用，外层 `execute_with_timeout`

### 3.7 Lua static-analyzer（分析门禁）

文件：`src/strategy/lua/static_analyzer.rs`

**标识符级 tokenize**（非正则）：解析注释（`--`/`--[[ ]]`）、字符串字面量、运算符，在 token 流上做模式匹配（**仅门禁，不执行**）。
执行由链中 Execution 策略（mlua-sandbox）完成。

覆盖模式（对齐 TS 参考实现并加固绕过）：
- 危险函数调用：`os.execute`（受 `allow_os_execute` 控制）/`os.remove`/`os.rename`/`os.exit`/`io.popen`/`loadstring`/`load`/`dofile`/`loadfile`，其中 `load`/`loadstring` 受 `allow_dynamic_load` 控制
- **索引访问绕过**：`os["execute"]`、拼接键 `os["exe".."cute"]`、动态索引 `os[var]()` 一律拒绝
- `require()` 模块白名单/黑名单（取首段模块名）
- `io.open` 写模式（`w`/`a`/`x`/`+`）拒绝（受 `restrict_io_open` 控制）
- 全局操纵：`_G[...]`、`_G.xxx`、`setfenv`、`getfenv`

### 3.8 python/js direct（裸执行，显式配置）

文件：`src/strategy/python/direct.rs`、`src/strategy/js/direct.rs`

裸 `python3 -c` / `node --eval` 执行，**零隔离**（原 os-hook 改名）。
不在任何默认链中，仅当用户在配置中显式列出时可用。

### 3.9 Shell container（Docker 隔离）

文件：`src/strategy/container.rs`

`docker run --rm -i --network none alpine:latest sh -c <cmd>` 容器隔离执行。
`is_available()` 通过 `docker --version` 探测（结果缓存）；docker 不可用时返回不可用，
链末执行层全部不可用则显式报错，不再静默降级。遗留 `type: docker` 配置映射到 `["container"]` 链。

---

## 四、VFS 子系统

文件：`src/vfs/`

```
OverlayVFS          → 两层文件系统：MemoryDelta 覆盖层 + HostFS 基础层
MemoryDelta         → HashMap<PathBuf, Vec<u8>> 写时复制
WhiteoutCache       → HashSet<PathBuf> 删除标记
HostFS              → 基于根路径的本地文件系统访问
```

VFS 已通过 config 集成到 `SandboxRuntime`（`options.vfs`）。
`VfsProvider` trait 提供 `read_file`/`write_file`/`exists`/`check_read`/`check_write` 异步接口，
其中 `check_read`/`check_write` 为**纯权限检查**（不产生副作用），供分析门禁预执行校验使用。

**当前 VFS 为预执行检查**（P1 落地）：Shell static-analyzer 从 token 级提取路径——
重定向目标（`>`/`>>`/`2>`/`&>`）按**写路径**经 `check_write` 校验，其余路径按读路径经
`check_read` 校验；`>&2` 文件描述符复制、heredoc（`<<`）、herestring（`<<<`）不误判。
OverlayVFS 的 `read_file`/`write_file` 均强制路径策略。
**不拦截子进程实际 IO**（CoW/路径重定向为长期任务，见 `docs/plan/sandbox-redesign.md`）。

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

`SandboxGlobalConfig::default()` 中 `audit_logging` **默认开启**（true），安全事件默认可追溯。

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
| Shell static-analyzer | 4 层 + bash/powershell/cmd 解析 | 命令替换拒绝 + shlex 分词 + 读写路径检查 | ✅ 等价 |
| Shell os-hook (seccomp) | `seccomp-loader` 二进制 | 真实 BPF 过滤 (libc) + 策略映射 | 🏆 RS 更优 |
| Python builtin-hook | 生成包围代码 | 生成包围代码 | ✅ 等价 |
| Python ast-analyzer | ast 子进程 JSON 分析 | ast 子进程 JSON 分析 | ✅ 等价 |
| JS vm-context | node:vm 上下文隔离 | 生成包裹 JS 代码 | ✅ 等价 |
| Lua mlua-sandbox | 子进程 + 字符串注入 | **内嵌 VM API 隔离** | 🏆 RS 唯一 |
| Lua static-analyzer | 18 模式 | 标识符级 tokenize + 18 模式 + 索引/拼接绕过 | ✅ 等价 |
| VFS + MountTable | MountTable + PathMapper | OverlayVFS + MemoryDelta + 读写双向预检查 | ⚠️ 架构不同 |
| 运行时集成 | 完整 (script-handler) | 仅核心 Runtime | ❌ 待 wf-core 集成 |
| 配置文件/profile | SandboxGlobalConfig + 解析 | 类型已定义，解析器未实现 | ❌ 待实现 |
| 交互式脚本 | 3 模式协调器 + PTY | 未实现 | ❌ 待实现 |

