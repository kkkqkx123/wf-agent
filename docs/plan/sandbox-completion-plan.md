# Rust 沙箱功能补充方案

## 前提

基于对 TS 参考实现与当前 RS 实现的全面对比，现有 8 个策略/功能点缺失或为存根。
当前 RS 覆盖率约 60%。以下分 4 阶段补齐剩余功能，每阶段产出可独立验证。

---

## Phase 1：策略实现补全（高优先级）

### 1.1 JS `vm-context` 策略

**现状**：仅有 `node --eval` 子进程，无任何隔离。TS 使用 `node:vm` 实现真正的上下文隔离。

**方案**：使用 `rusty_v8` 或 `deno_core` 嵌入 V8，或通过 `node --eval` 注入包裹代码实现受限全局变量。

推荐路径（渐进式）：

```
Step 1a: JS 子进程包裹策略（短中期）
  生成包裹 JS 代码，注入到 node --eval：
  - 替换 require() 为安全版本（模块白名单/黑名单）
  - 删除 eval/Function/globalThis/global
  - 用只读 Proxy 包裹 fs、child_process、process
  - 限制 setTimeout/setInterval 最大延时

Step 1b: rusty_v8 原生嵌入（长期）
  目标：与 Lua mlua 同级别的 Rust 原生优势
  挑战：v8 编译复杂，体积大
```

**实现文件**：`crates/wf-sandbox/src/strategy/js/vm_context.rs`

**验收条件**：
- `test_js_vm_context_deny_require_fs` — require('fs') 写入被拒绝
- `test_js_vm_context_deny_eval` — eval() 被拒绝  
- `test_js_vm_context_allow_safe_math` — 纯计算通过
- `test_js_vm_context_deny_child_process` — require('child_process') 被拒绝

---

### 1.2 Python `builtin-hook` 增强

**现状**：硬编码字符串匹配 `import os` 等 7 个模式，无真正的 Python 代码注入。

**方案**：生成包裹 Python 代码，与 TS 一致：

```python
import sys
sys.path.clear()

# Safe __import__
_original_import = __builtins__.__import__
def _safe_import(name, *args, **kwargs):
    ALLOWED = {ALLOWED_MODULES}
    DENIED = {DENIED_MODULES}
    if DENIED and name in DENIED: raise ImportError(f"module denied: {name}")
    if ALLOWED and name not in ALLOWED: raise ImportError(f"module not allowed: {name}")
    return _original_import(name, *args, **kwargs)
__builtins__.__import__ = _safe_import

# Safe open
_original_open = __builtins__.open
def _safe_open(file, mode='r', *args, **kwargs):
    if RESTRICT_OPEN and 'w' in mode:
        ALLOWED_WRITE = {ALLOWED_WRITE_PATHS}
        if not any(file.startswith(p) for p in ALLOWED_WRITE):
            raise PermissionError(f"write to {file} not allowed")
    return _original_open(file, mode, *args, **kwargs)
__builtins__.open = _safe_open

# Disable dangerous builtins
import builtins
for _name in ['eval', 'exec', 'compile', '__import__']:
    setattr(builtins, _name, None)

# User code
{USER_CODE}
```

**实现文件**：`crates/wf-sandbox/src/strategy/python/builtin_hook.rs`（重写）

**验收条件**：
- `test_python_builtin_hook_deny_os` — import os 被拒绝
- `test_python_builtin_hook_deny_subprocess` — import subprocess 被拒绝
- `test_python_builtin_hook_deny_eval` — eval() 被拒绝
- `test_python_builtin_hook_allow_print` — print 通过

---

### 1.3 Python `ast-analyzer` 增强

**现状**：仅对 5 个模式进行正则匹配。

**方案**：通过子进程调用 Python `ast` 模块的真实解析：

```
生成 Python 脚本，使用 ast.parse() 分析代码：
- 遍历 AST，收集所有 Import/ImportFrom/Call 节点
- 检查被导入模块是否在拒绝列表中
- 检查函数调用是否危险（eval/exec/compile/open）
- 返回 JSON 格式的分析报告
- 如果有违规，拒绝执行
```

**实现文件**：`crates/wf-sandbox/src/strategy/python/ast_analyzer.rs`（重写）

**验收条件**：
- `test_python_ast_analyzer_detect_import_os` — 检测 import os
- `test_python_ast_analyzer_allow_safe_math` — 纯数学通过
- `test_python_ast_analyzer_detect_eval_call` — 检测 eval() 调用
- `test_python_ast_analyzer_detect_exec_call` — 检测 exec() 调用

---

### 1.4 Shell Seccomp 真实实现

**现状**：存根，直接 `sh -c` 无过滤。

**方案**：使用 `nix` crate 的 `seccomp` 模块构建系统调用过滤器。

```rust
use nix::sched::CloneFlags;
use nix::sys::signal::Signal;

fn build_seccomp_filter(policy: &SandboxPolicy) -> Result<SockFilter, Box<dyn Error>> {
    let mut ctx = SeccompCtx::new(Action::Deny)?;
    
    // Always allow basic I/O and process lifecycle
    ctx.allow_syscall(Syscall::read)?;
    ctx.allow_syscall(Syscall::write)?;
    ctx.allow_syscall(Syscall::exit)?;
    ctx.allow_syscall(Syscall::exit_group)?;
    
    // Conditional based on policy
    let fs_policy = policy.filesystem.as_ref();
    if fs_policy.map_or(true, |p| p.allowed_read_paths.is_not_empty()) {
        ctx.allow_syscall(Syscall::open)?;
        ctx.allow_syscall(Syscall::openat)?;
        ctx.allow_syscall(Syscall::stat)?;
        ctx.allow_syscall(Syscall::lstat)?;
    }
    
    if !policy.network.as_ref().map_or(true, |p| p.access_type == NetworkAccessType::None) {
        ctx.allow_syscall(Syscall::socket)?;
        ctx.allow_syscall(Syscall::connect)?;
        ctx.allow_syscall(Syscall::sendto)?;
        ctx.allow_syscall(Syscall::recvfrom)?;
    }
    
    // Deny dangerous syscalls
    ctx.deny_syscall(Syscall::clone)?;      // fork
    ctx.deny_syscall(Syscall::execve)?;     // exec unless allowed
    ctx.deny_syscall(Syscall::ptrace)?;
    
    Ok(ctx.compile()?)
}
```

执行流程：
1. fork() 子进程
2. 子进程应用 seccomp 过滤器（`prctl(PR_SET_NO_NEW_PRIVS, 1)` + 安装过滤器）
3. 子进程 exec() 目标命令
4. 父进程收集 stdout/stderr/exit_code
5. 如果子进程被 seccomp 杀死 → 返回违规

**实现文件**：`crates/wf-sandbox/src/strategy/shell/os_hook.rs`（重写）

**依赖**：`nix = "0.28"`, 仅在 `target_os = "linux"` 时编译

**验收条件**：
- `test_seccomp_deny_fork` — fork() 被拒绝
- `test_seccomp_deny_network` — socket() 拒绝（当 network=none）
- `test_seccomp_allow_basic_echo` — echo 通过
- `test_seccomp_deny_ptrace` — ptrace 被拒绝

---

### 1.5 Container 策略真实实现

**现状**：`is_available() = false` 的 CLI 存根。

**方案 A（推荐）**：改用 `bollard`（Docker API Rust 库）替代 CLI。

```rust
use bollard::{Docker, container::Config};

pub struct ContainerStrategy {
    docker: Docker,
    image: String,
}

impl ContainerStrategy {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let docker = Docker::connect_with_socket_defaults()?;
        Ok(Self { docker, image: "alpine:latest".into() })
    }
}

impl StrategyImplementation<ScriptExecutionResult> for ContainerStrategy {
    fn is_available(&self) -> bool {
        Docker::connect_with_socket_defaults().is_ok()
    }
    
    async fn execute(&self, options: StrategyExecuteOptions, policy: &SandboxPolicy) -> ... {
        let config = Config {
            image: Some(self.image.as_str()),
            cmd: Some(vec!["sh", "-c", &options.command]),
            network_disabled: Some(policy.network.as_ref()
                .map_or(true, |n| n.access_type == NetworkAccessType::None)),
            ..Default::default()
        };
        let container = self.docker.create_container::<&str, &str>(None, config).await?;
        // Start, wait, get logs, remove
    }
}
```

**方案 B（轻量）**：保留 CLI 但改为 true `is_available()` + `which docker` 检测。

**验收条件**：
- 需要 Docker 守护进程运行
- `test_container_docker_echo` — 容器内 echo 通过

---

## Phase 2：类型补全 + Profile 系统（中优先级）

### 2.1 新增类型

在 `crates/wf-types/src/script/sandbox.rs` 中补充：

```rust
/// 策略选择枚举（字符串 ID）
pub struct ShellStrategyId(pub &'static str);  // "static-analyzer", "os-hook"
pub struct PythonStrategyId(pub &'static str); // "builtin-hook", "ast-analyzer", "os-hook"
pub struct JavaScriptStrategyId(pub &'static str); // "subprocess", "vm-context", "os-hook"
pub struct LuaStrategyId(pub &'static str);    // "mlua-sandbox", "static-analyzer"

/// 沙箱 Profile 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProfile {
    pub name: String,
    pub description: Option<String>,
    pub mode: Option<SandboxMode>,
    pub shell_strategy: Option<Vec<String>>,
    pub python_strategy: Option<Vec<String>>,
    pub javascript_strategy: Option<Vec<String>>,
    pub lua_strategy: Option<Vec<String>>,
    pub policy: Option<SandboxPolicy>,
    pub vfs: Option<VfsConfig>,
}

/// Profile 匹配规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProfileRule {
    pub match_field: String,      // "language", "workflow_id", "node_id"
    pub match_pattern: String,    // glob pattern
    pub profile: String,          // 引用的 profile name
}

/// 全局沙箱配置（对应 TOML 配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxGlobalConfig {
    pub mode: Option<SandboxMode>,
    pub profiles: Vec<SandboxProfile>,
    pub rules: Vec<SandboxProfileRule>,
    pub default_profile: Option<String>,
    pub audit_logging: bool,
}

/// 安全审计事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: AuditEventType,
    pub language: String,
    pub script_name: String,
    pub violation: Option<String>,
    pub strategy_id: Option<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    ExecutionAllowed,
    ExecutionDenied,
    ExecutionViolation,
    StrategyFallback,
    ConfigError,
}

/// 执行结果元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptMetadata {
    pub name: String,
    pub language: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}
```

### 2.2 Profile 解析器

**实现文件**：`crates/wf-sandbox/src/profile.rs`

```
SandboxProfileResolver:
- resolve(config, language, context) -> (SandboxMode, Vec<String>, SandboxPolicy, VfsConfig)
  1. 如果没有 profile，返回默认值
  2. 查找 matching rule for context
  3. 加载对应 profile
  4. 合并 SandboxGlobalConfig.mode + profile.mode + SandboxConfig.mode
  5. 合并策略层（global → profile → config override）
```

**验收条件**：
- `test_profile_resolve_default` — 无 profile 时返回默认值
- `test_profile_resolve_by_language` — 按语言匹配 profile
- `test_profile_resolve_override_mode` — 配置覆盖 profile 的 mode

---

## Phase 3：运行时集成（中优先级）

### 3.1 VFS 连接

将 `StrategyExecuteOptions.vfs` 从 `None` 改为由 `SandboxRuntime` 初始化。

```
SandboxRuntime.execute():
  1. resolve config (获取 VFS 配置)
  2. if vfs.enabled:
     - 创建 OverlayVFS(workspace_root, path_policy)  
     - 包裹在 VfsProvider trait 中
  3. pass vfs 到 StrategyExecuteOptions
```

同时实现 VFS 路径检查器（当前返回 `None` 的存根）。

### 3.2 wf-core 集成

在 `crates/wf-core/src/state.rs` 或新文件添加 `wf-core` 与沙箱的连接：

```
WorkflowNodeHandler:
  - 当 node_type == "SCRIPT" 时:
    1. 从 node config 提取 SandboxConfig
    2. 创建或获取 SandboxRuntime
    3. 调用 runtime.execute(language, command, config)
    4. 将 ScriptExecutionResult 映射到 node output
```

具体位置参考 TS 的 `script-handler.ts`：

- 在 wf-core 中新增 `src/handlers/script_handler.rs`
- 注册到 `WorkflowEngine` 的 node handler 工厂
- 处理 SCRIPT 和 INTERACTIVE_SCRIPT 节点

### 3.3 默认策略注册补全

在 `DefaultStrategyResolver::register_default_strategies()` 中注册全部策略：

```rust
fn register_default_strategies(&mut self) {
    // Shell
    self.shell_strategies.insert("static-analyzer", Arc::new(ShellStaticAnalyzerStrategy));
    self.shell_strategies.insert("os-hook", Arc::new(LinuxSeccompStrategy));
    
    // Python
    self.python_strategies.insert("builtin-hook", Arc::new(PythonBuiltinHookStrategy));
    self.python_strategies.insert("ast-analyzer", Arc::new(PythonAstAnalyzerStrategy));
    self.python_strategies.insert("os-hook", Arc::new(PythonOsHookStrategy));
    
    // JavaScript
    self.js_strategies.insert("subprocess", Arc::new(JavaScriptSubprocessStrategy));
    self.js_strategies.insert("vm-context", Arc::new(JavaScriptVmContextStrategy));
    self.js_strategies.insert("os-hook", Arc::new(JavaScriptOsHookStrategy));
    
    // Lua
    self.lua_strategies.insert("mlua-sandbox", Arc::new(LuaMluaSandboxStrategy));
    self.lua_strategies.insert("static-analyzer", Arc::new(LuaStaticAnalyzerStrategy));
}
```

---

## Phase 4：安全审计 + 安全验证器（低优先级）

### 4.1 安全验证器

对应 TS `security-validator.ts`：

```rust
// crates/wf-sandbox/src/security.rs

pub struct SecurityValidator;

impl SecurityValidator {
    pub fn validate_expression(expr: &str) -> Result<(), SecurityViolation> {
        // 最大长度 1000
        // 禁止 __proto__/constructor/prototype
        // 无连续点号
        // 深度限制 10 层
    }
    
    pub fn validate_path(path: &str) -> Result<(), SecurityViolation> {
        // 合法路径正则
        // 禁止路径遍历攻击 ../../
        // 禁止空组件 //
    }
    
    pub fn validate_array_index(index: i64, length: usize) -> Result<(), SecurityViolation> {
        // 非负
        // 不越界
    }
    
    pub fn validate_value_type(value: &serde_json::Value) -> Result<(), SecurityViolation> {
        // 无函数、无类实例（JSON only）
    }
}
```

### 4.2 审计日志集成

在 `SandboxRuntime` 中增加可选审计：

```
SandboxRuntime {
    auditor: Option<Arc<dyn Auditor>>,
}

trait Auditor: Send + Sync {
    fn record(&self, event: AuditEvent);
}

struct ConsoleAuditor;
struct FileAuditor { path: PathBuf };
struct DatabaseAuditor { storage: Arc<dyn CheckpointStorage> };
```

每次策略执行、违规、回退都记录 `AuditEvent`。

---

## 实施路线图

| 阶段 | 内容 | 预估工作量 | 测试数 |
|------|------|-----------|--------|
| **Phase 1** | JS vm-context, Python builtin-hook 增强, Python AST 增强, Shell seccomp 真实实现, Container 真实实现 | 8-10 人日 | 25+ |
| **Phase 2** | 类型补全（Profile/GlobalConfig/AuditEvent/ScriptMetadata）, Profile 解析器实现 | 3-4 人日 | 8+ |
| **Phase 3** | VFS 连接, wf-core 集成, 默认策略注册补全 | 4-5 人日 | 10+ |
| **Phase 4** | 安全验证器, 审计日志系统 | 2-3 人日 | 8+ |
| **总计** | 完成 TS 沙箱全功能覆盖 | 17-22 人日 | 50+ |

### 优先级建议

1. **Phase 1 先做** — 补全核心策略实现，消除安全缺口
2. **Phase 2 紧跟** — Profile 系统是配置驱动沙箱的前提
3. **Phase 3 与 Phase 1 可并行** — 运行时集成和新策略实现互不依赖
4. **Phase 4 最后** — 是增强功能，非必须

### 与 TS 功能对账（完成所有 Phase 后）

| 功能 | TS | RS Phase 0 | RS Phase 1-4 |
|------|----|-----------|--------------|
| SandboxMode (D/L/S/C) | ✅ | ✅ | ✅ |
| 所有 Policy 类型 | ✅ | ✅ | ✅ |
| SandboxConfig + 旧版兼容 | ✅ | ✅ | ✅ |
| SandboxProfile + 规则 | ✅ | ❌ | ✅ |
| SandboxGlobalConfig | ✅ | ❌ | ✅ |
| AuditEvent | ✅ | ❌ | ✅ |
| SecurityValidator | ✅ | ❌ | ✅ |
| Shell static-analyzer | ✅ | ✅ | ✅ |
| Shell seccomp | ✅ | 存根 | ✅ |
| Python builtin-hook | ✅ | 弱 | ✅ |
| Python ast-analyzer | ✅ | 弱 | ✅ |
| JS vm-context | ✅ | ❌ | ✅ |
| JS subprocess | ✅ | ✅ | ✅ |
| Lua mlua-sandbox | ❌(TS无法) | ✅ | ✅ |
| Lua static-analyzer | ✅ | ✅ | ✅ |
| Container | ❌ | 存根 | ✅ |
| VFS + MountTable | ✅ | 未连接 | ✅ |
| Default all strategies | ✅ | 部分 | ✅ |
| Profile/Config resolution | ✅ | ❌ | ✅ |
| Runtime integration | ✅ | ❌ | ✅ |
| Interactive scripts | ✅ | ❌ | ❌(可选) |
| wf-config pipeline | ✅ | 部分 | ✅ |
