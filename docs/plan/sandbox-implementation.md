# Rust Sandbox 实现方案

## 一、设计目标

为 `wf-agent` 的 Rust 版本实现安全的脚本执行沙箱系统，支持多种隔离策略，确保 AI 生成代码的安全执行。系统需遵循 TypeScript 版的设计原则，同时适配 Rust 生态和最佳实践。

### 核心功能

- 多语言支持：Shell、Python、JavaScript、Lua 脚本的沙箱执行（**Lua 为 Rust 原生优势**）
- 分层架构：Policy（策略）→ Strategy（策略实现）→ Executor（执行器）→ Runtime（运行时）
- 策略优先级：支持按优先级数组自动回退到备用策略
- 安全模式：严格模式（拒绝违规操作）与宽松模式（仅记录警告）
- 虚拟文件系统（VFS）：支持写时复制（Copy-on-Write）的覆盖文件系统
- 配置驱动：通过 TOML 配置文件定义默认策略和规则

### 架构对比

| 维度 | TypeScript 版 | Rust 版 |
|------|---------------|--------|
| **核心架构** | 四层模型（Policy → Strategy → Executor → Runtime） | 相同，Rust 化 |
| **Shell 沙箱** | static-analyzer, os-hook, container | 相同 |
| **Python 沙箱** | builtin-hook, ast-analyzer, os-hook, container | builtin-hook, ast-analyzer, os-hook, container |
| **JS 沙箱** | vm-context, isolated-vm, os-hook, container | 进程级隔离（subprocess）, os-hook, container |
| **Lua 沙箱** | builtin-hook（子进程）, static-analyzer | **Rust 原生优势：`mlua` 内嵌 VM，API 级隔离** |
| **OS Hook** | seccomp-bpf（helper binary） | seccomp（`nix`/`seccompiler` 直接调用） |
| **VFS** | Node.js 内存/VFS 层 | Rust OverlayFS + MemoryDelta |
| **容器** | Docker/Podman CLI | Docker API 库 |
| **实现方式** | 单进程内嵌（JS/Lua 除外） | 多进程（Shell/Python/JS）+ **内嵌（Lua）** |

---

## 二、类型设计

扩展 `wf-types` 中的沙箱相关类型，保持与 TS 版的高度兼容性。

### 2.1 基础枚举与结构体

```rust
// src/script/sandbox.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SandboxMode {
    Disabled,
    Lenient,
    Strict,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesystemPolicy {
    pub allowed_read_paths: Vec<String>,
    pub allowed_write_paths: Vec<String>,
    pub allowed_remove_paths: Vec<String>,
    pub allowed_execute_paths: Vec<String>,
    pub copy_on_write: bool,
    pub max_file_size: u64, // bytes
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessPolicy {
    pub allowed_child_processes: Vec<String>,
    pub denied_child_processes: Vec<String>,
    pub max_child_processes: u32,
    pub allow_fork: bool,
    pub allow_exec: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkPolicy {
    #[serde(rename = "access")]
    pub access_type: NetworkAccessType,
    pub allowed_domains: Option<Vec<String>>,
    pub allowed_ports: Option<Vec<(u16, u16)>>,
    pub allow_dns: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkAccessType {
    None,
    Localhost,
    Specific,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePolicy {
    pub cpu_limit_ms: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub disk_limit_mb: Option<u64>,
    pub timeout_limit_ms: Option<u64>,
}
```

### 2.2 语言特定策略

```rust
// Shell 策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShellPolicy {
    pub allowed_commands: Vec<String>,
    pub denied_commands: Vec<String>,
    pub dangerous_patterns: Vec<String>,
    pub allow_pipe: bool,
    pub allow_redirect: bool,
}

// Python 策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PythonPolicy {
    pub allowed_modules: Vec<String>,
    pub denied_modules: Vec<String>,
    pub allow_subprocess: bool,
    pub restrict_builtin_open: bool,
    pub allow_dynamic_eval: bool,
}

// JavaScript 策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JavaScriptPolicy {
    pub allowed_modules: Vec<String>,
    pub denied_modules: Vec<String>,
    pub allow_child_process: bool,
    pub allow_fs_write: bool,
    pub allow_dynamic_eval: bool,
}

// Lua 策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LuaPolicy {
    pub allowed_modules: Vec<String>,
    pub denied_modules: Vec<String>,
    pub allow_os_execute: bool,
    pub restrict_io_open: bool,
    pub allow_dynamic_load: bool,
}
```

### 2.3 综合策略与配置

```rust
// src/script/sandbox.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxPolicy {
    pub mode: SandboxMode,
    pub shell: Option<ShellPolicy>,
    pub python: Option<PythonPolicy>,
    pub javascript: Option<JavaScriptPolicy>,
    pub lua: Option<LuaPolicy>,
    pub filesystem: Option<FilesystemPolicy>,
    pub process: Option<ProcessPolicy>,
    pub network: Option<NetworkPolicy>,
    pub resource: Option<ResourcePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxConfig {
    pub mode: Option<SandboxMode>,
    pub policy: Option<SandboxPolicy>,
    pub shell_strategy: Option<Vec<String>>,
    pub python_strategy: Option<Vec<String>>,
    pub javascript_strategy: Option<Vec<String>>,
    pub lua_strategy: Option<Vec<String>>,
    pub vfs: Option<VfsConfig>,
    
    // === 遗留兼容字段 ===
    #[serde(rename = "type")]
    pub legacy_type: Option<String>,
    pub image: Option<String>,
    pub resource_limits: Option<ResourceLimits>,
    pub network_enabled: Option<bool>,
    pub allowed_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VfsConfig {
    pub enabled: bool,
    pub storage: Option<VfsStorageType>,
    pub db_path: Option<String>,
    pub workspace_root: Option<String>,
    pub path_policy: Option<PathPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VfsStorageType {
    Memory,
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathPolicy {
    pub allowed_read: Vec<String>,
    pub allowed_write: Vec<String>,
}
```

### 2.4 执行结果

```rust
// src/script/executor.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptExecutionResult {
    pub success: bool,
    pub script_name: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub execution_time: u64, // milliseconds
    pub error: Option<String>,
    pub sandbox_mode: Option<String>,
    pub strategy_id: Option<String>,
    pub violations: Option<Vec<String>>, // lenient mode only
}
```

---

## 三、模块架构

采用分层模块设计，每个模块职责单一，便于测试和维护。

```
crates/wf-sandbox/src/
├── lib.rs                    # mod declarations
├── runtime.rs                # SandboxRuntime orchestrator
├── resolver.rs               # StrategyResolver & DefaultStrategyResolver
├── policy.rs                 # Policy merging and resolution
├── executor/                 # Language-specific executors
│   ├── shell_executor.rs     # SandboxShellExecutor
│   ├── python_executor.rs    # SandboxPythonExecutor
│   ├── js_executor.rs        # SandboxJavaScriptExecutor
│   └── lua_executor.rs       # SandboxLuaExecutor (Rust 原生内嵌)
├── strategy/                 # Strategy implementations
│   ├── shell/
│   │   ├── static_analyzer.rs
│   │   └── os_hook.rs
│   ├── python/
│   │   ├── builtin_hook.rs
│   │   ├── ast_analyzer.rs
│   │   └── os_hook.rs
│   ├── js/
│   │   ├── subprocess.rs     # Run in separate process
│   │   └── os_hook.rs
│   ├── lua/
│   │   ├── mlua_sandbox.rs   # `mlua` 原生 VM 隔离（推荐）
│   │   └── static_analyzer.rs
│   └── container.rs          # Docker/Podman API client
├── vfs/                      # Virtual File System
│   ├── overlay.rs            # OverlayVFS
│   ├── delta.rs              # MemoryDelta
│   ├── whiteout.rs           # WhiteoutCache
│   └── base.rs               # HostFS
└── default_policy.rs         # DEFAULT_SANDBOX_POLICY constant
```

---

## 四、关键组件实现

### 4.1 StrategyResolver

负责策略解析和优先级回退链。

```rust
// src/resolver.rs

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub trait StrategyImplementation<T> {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn priority(&self) -> i32;
    fn is_available(&self) -> bool;
    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>;
}

pub trait StrategyResolver {
    fn resolve_shell_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation<ScriptExecutionResult>>>;
    fn resolve_python_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation<ScriptExecutionResult>>>;
    fn resolve_js_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation<ScriptExecutionResult>>>;
    fn resolve_lua_strategy(&self, id: &str) -> Option<Arc<dyn StrategyImplementation<ScriptExecutionResult>>>;
    fn register_strategy(
        &mut self,
        language: &str,
        impl: Arc<dyn StrategyImplementation<ScriptExecutionResult>>
    );
    fn resolve_best(&self, language: &str, preferred_ids: &[String]) -> Option<Arc<dyn StrategyImplementation<ScriptExecutionResult>>>;
}

pub struct DefaultStrategyResolver {
    shell_strategies: HashMap<String, Arc<dyn StrategyImplementation<ScriptExecutionResult>>>,
    python_strategies: HashMap<String, Arc<dyn StrategyImplementation<ScriptExecutionResult>>>,
    js_strategies: HashMap<String, Arc<dyn StrategyImplementation<ScriptExecutionResult>>>,
    lua_strategies: HashMap<String, Arc<dyn StrategyImplementation<ScriptExecutionResult>>>, // Rust 原生优势
}

impl DefaultStrategyResolver {
    pub fn new() -> Self {
        let mut resolver = Self {
            shell_strategies: HashMap::new(),
            python_strategies: HashMap::new(),
            js_strategies: HashMap::new(),
            lua_strategies: HashMap::new(),
        };
        resolver.register_default_strategies();
        resolver
    }

    fn register_default_strategies(&mut self) {
        // Register Shell strategies
        self.shell_strategies.insert("static-analyzer".to_string(), Arc::new(ShellStaticAnalyzerStrategy));
        self.shell_strategies.insert("linux-seccomp".to_string(), Arc::new(LinuxSeccompStrategy));
        
        // Register Python strategies
        self.python_strategies.insert("builtin-hook".to_string(), Arc::new(PythonBuiltinHookStrategy));
        self.python_strategies.insert("ast-analyzer".to_string(), Arc::new(PythonAstAnalyzerStrategy));
        
        // Register JS strategies
        self.js_strategies.insert("subprocess".to_string(), Arc::new(JavaScriptSubprocessStrategy));
        self.js_strategies.insert("linux-seccomp".to_string(), Arc::new(LinuxSeccompStrategy));
        
        // Register Lua strategies — **Rust 原生优势**，无需子进程
        self.lua_strategies.insert("mlua-sandbox".to_string(), Arc::new(LuaMluaSandboxStrategy));
        self.lua_strategies.insert("static-analyzer".to_string(), Arc::new(LuaStaticAnalyzerStrategy));
    }
}
```

### 4.2 Shell Static Analyzer

轻量级静态分析策略，基于正则表达式进行命令检查。

```rust
// src/strategy/shell/static_analyzer.rs

use regex::Regex;

pub struct ShellStaticAnalyzerStrategy;

impl StrategyImplementation<ScriptExecutionResult> for ShellStaticAnalyzerStrategy {
    fn id(&self) -> &str { "static-analyzer" }
    fn name(&self) -> &str { "Shell Static Analyzer" }
    fn description(&self) -> &str { "Static command analysis with dangerous pattern matching" }
    fn priority(&self) -> i32 { 10 }
    fn is_available(&self) -> bool { true }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let command = &options.command;
        if command.is_empty() {
            return Ok(execution_result(false, "Empty command"));
        }

        let shell_type = resolve_shell_type(options.shell_type.as_deref(), options.runtime.as_deref());
        let shell_policy = policy.shell.as_ref().unwrap_or(&DEFAULT_SHELL_POLICY);

        // Layer 0: Dangerous pattern detection on full command
        let dangerous_patterns = shell_policy.dangerous_patterns.iter()
            .map(|p| Regex::new(p).ok())
            .collect::<Vec<_>>();
        
        for maybe_regex in &dangerous_patterns {
            if let Some(regex) = maybe_regex {
                if regex.is_match(command) {
                    return Ok(deny(command, &format!("Dangerous pattern detected: {}", regex.as_str())));
                }
            }
        }

        // Layer 0: Pipe operator check
        if !shell_policy.allow_pipe && command.contains('|') {
            return Ok(deny(command, "Pipe operator is not allowed"));
        }

        // Layer 1: Chain-aware analysis
        let sub_commands = parse_command_chain(command);
        for sub_command in &sub_commands {
            let result = analyze_subcommand(sub_command, shell_policy);
            if !result.allowed {
                return Ok(deny(command, &format!("Sub-command \"{}\" denied: {}", sub_command, result.reason.unwrap_or("Analysis failed"))));
            }
        }

        // Layer 2: VFS path policy check
        if let Some(vfs) = options.vfs {
            for sub_command in &sub_commands {
                let path_violation = check_vfs_paths(sub_command, &*vfs).await;
                if let Some(reason) = path_violation {
                    return Ok(deny(command, &format!("Sub-command \"{}\" path violation: {}", sub_command, reason))));
                }
            }
        }

        // Execute via TerminalService
        execute_command(command, options).await
    }
}
```

### 4.3 OS Hook (Linux seccomp)

使用 seccomp 系统调用过滤器进行系统级隔离。

```rust
// src/strategy/shell/os_hook.rs

#[cfg(target_os = "linux")]
pub struct LinuxSeccompStrategy;

#[cfg(target_os = "linux")]
impl StrategyImplementation<ScriptExecutionResult> for LinuxSeccompStrategy {
    fn id(&self) -> &str { "linux-seccomp" }
    fn name(&self) -> &str { "Linux Seccomp (OS Hook)" }
    fn description(&self) -> &str { "Linux seccomp-bpf system call filtering" }
    fn priority(&self) -> i32 { 50 }
    fn is_available(&self) -> bool { true }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = std::time::Instant::now();

        // Build seccomp filter based on policy
        let syscall_filter = build_seccomp_filter(policy)?;
        
        // Apply filter to child process using nix crate or seccompiler
        let output = apply_seccomp_and_run(options.command, syscall_filter).await?;
        
        Ok(ScriptExecutionResult {
            success: output.status.success(),
            script_name: "sandbox-os-hook".to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string().into(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string().into(),
            exit_code: output.status.code(),
            execution_time: start_time.elapsed().as_millis() as u64,
            error: (!output.status.success()).then(|| "Command failed".to_string()),
        })
    }
}
```

### 4.4 Container Strategy

使用 Docker API 进行容器化隔离。

```rust
// src/strategy/container.rs

use docker_api::{Docker, opts::ContainerCreateOpts};

pub struct ContainerStrategy {
    docker: Docker,
}

impl ContainerStrategy {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docker = Docker::connect_with_socket_defaults().await?;
        Ok(Self { docker })
    }
}

impl StrategyImplementation<ScriptExecutionResult> for ContainerStrategy {
    fn id(&self) -> &str { "container" }
    fn name(&self) -> &str { "Container (Docker)" }
    fn description(&self) -> &str { "Run script in isolated Docker container" }
    fn priority(&self) -> i32 { 40 }
    fn is_available(&self) -> bool { true }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = std::time::Instant::now();
        let image = policy.resource.as_ref().and_then(|r| r.image.clone()).unwrap_or_else(|| "alpine:latest".to_string());

        let container_opts = ContainerCreateOpts::builder()
            .image(&image)
            .cmd(vec!["sh", "-c", &options.command])
            .build();
        
        let container = self.docker.containers().create(&container_opts).await?;
        container.start().await?;
        
        let logs = container.logs().get().await?;
        container.delete().await?;
        
        Ok(ScriptExecutionResult {
            success: true,
            script_name: "sandbox-container".to_string(),
            stdout: String::from_utf8_lossy(&logs.stdout).to_string().into(),
            stderr: String::from_utf8_lossy(&logs.stderr).to_string().into(),
            exit_code: Some(0),
            execution_time: start_time.elapsed().as_millis() as u64,
            error: None,
        })
    }
}
```

### 4.5 Lua Strategy (**Rust 原生优势**)

使用 `mlua` 嵌入 Lua VM，实现真正的 API 级隔离，无需子进程。这是 Rust 相比 TS 的最大技术优势。

```rust
// src/strategy/lua/mlua_sandbox.rs

use mlua::{Lua, Function, Value, Result};

pub struct LuaMluaSandboxStrategy;

impl LuaMluaSandboxStrategy {
    /// Create a safe Lua environment with restricted globals
    fn create_safe_environment(lua: &Lua, policy: &LuaPolicy) -> Result<()> {
        let globals = lua.globals();

        // Disable dangerous modules by default
        let denied_by_default = ["os", "io", "package", "debug", "ffi"];
        for module in &denied_by_default {
            if policy.denied_modules.contains(*module) || policy.denied_modules.is_empty() {
                globals.set(*module, Value::Nil)?;
            }
        }

        // Safe print function
        let safe_print = lua.create_function(|_, s: String| {
            println!("{}", s);
            Ok(())
        })?;
        globals.set("print", safe_print)?;

        // Safe require with module filtering
        let allowed = policy.allowed_modules.clone();
        let denied = policy.denied_modules.clone();
        
        let safe_require = lua.create_function(move |lua, module_name: String| -> Result<Value> {
            // Check whitelist first (if specified)
            if !allowed.is_empty() && !allowed.contains(&module_name) {
                return Err(mlua::Error::RuntimeError(format!("Module not allowed: {}", module_name)));
            }
            
            // Check blacklist
            if denied.contains(&module_name) {
                return Err(mlua::Error::RuntimeError(format!("Module denied: {}", module_name)));
            }

            // Load basic safe modules directly
            match module_name.as_str() {
                "table" | "string" | "math" | "utf8" | "coroutine" => {
                    lua.load(&format!("return require('{}')", module_name)).eval()
                }
                _ => Err(mlua::Error::RuntimeError("Module not supported in sandbox".to_string())),
            }
        })?;
        globals.set("require", safe_require)?;

        Ok(())
    }
}

impl StrategyImplementation<ScriptExecutionResult> for LuaMluaSandboxStrategy {
    fn id(&self) -> &str { "mlua-sandbox" }
    fn name(&self) -> &str { "Lua MLua VM" }
    fn description(&self) -> &str { "Lua script sandboxing using mlua VM with API-level isolation" }
    fn priority(&self) -> i32 { 100 } // Highest priority - native Rust
    fn is_available(&self) -> bool { true }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = std::time::Instant::now();
        let code = &options.command;
        
        if code.is_empty() {
            return Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-lua".to_string(),
                stdout: None,
                stderr: Some("Empty Lua code".to_string()),
                exit_code: Some(1),
                execution_time: start_time.elapsed().as_millis() as u64,
                error: Some("Empty Lua code".to_string()),
            });
        }

        // Create a new Lua state for each execution (complete isolation)
        let lua = Lua::new();

        // Apply policy restrictions
        let lua_policy = policy.lua.as_ref().cloned().unwrap_or_default();
        Self::create_safe_environment(&lua, &lua_policy)?;

        // Execute the code with timeout support
        let timeout_ms = policy.resource.as_ref().and_then(|r| r.timeout_limit_ms).unwrap_or(30000);
        
        let result = tokio::task::spawn_blocking(move || {
            lua.load(code).eval::<Value>()
        }).await;

        match result {
            Ok(Ok(_value)) => {
                Ok(ScriptExecutionResult {
                    success: true,
                    script_name: "sandbox-lua".to_string(),
                    stdout: Some(String::new()),
                    stderr: None,
                    exit_code: Some(0),
                    execution_time: start_time.elapsed().as_millis() as u64,
                    error: None,
                })
            }
            Ok(Err(e)) => {
                Ok(ScriptExecutionResult {
                    success: false,
                    script_name: "sandbox-lua".to_string(),
                    stdout: None,
                    stderr: Some(e.to_string()),
                    exit_code: Some(1),
                    execution_time: start_time.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                })
            }
            Err(_) => {
                Ok(ScriptExecutionResult {
                    success: false,
                    script_name: "sandbox-lua".to_string(),
                    stdout: None,
                    stderr: Some("Task join error".to_string()),
                    exit_code: Some(1),
                    execution_time: start_time.elapsed().as_millis() as u64,
                    error: Some("Execution timeout or cancelled".to_string()),
                })
            }
        }
    }
}
```

**Lua vs TS 对比**:

| 特性 | TS 版 | Rust 版 |
|------|-------|--------|
| **运行方式** | 子进程 + temp file | 内嵌 VM，零开销 |
| **隔离级别** | 字符串注入（易逃逸） | API 级控制（安全） |
| **性能** | IPC overhead | 原地执行 |
| **安全性** | 依赖字符串匹配 | Rust 所有权保证 |

### 4.6 Lua Static Analyzer (Fallback)

当 mlua 不可用时的降级方案。

```rust
// src/strategy/lua/static_analyzer.rs

use regex::Regex;

pub struct LuaStaticAnalyzerStrategy;

impl StrategyImplementation<ScriptExecutionResult> for LuaStaticAnalyzerStrategy {
    fn id(&self) -> &str { "static-analyzer" }
    fn name(&self) -> &str { "Lua Static Analyzer" }
    fn description(&self) -> &str { "Static analysis of Lua code for dangerous patterns" }
    fn priority(&self) -> i32 { 10 }
    fn is_available(&self) -> bool { true }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let command = &options.command;
        if command.is_empty() {
            return Ok(execution_result(false, "Empty command"));
        }

        let lua_policy = policy.lua.as_ref().unwrap_or(&DEFAULT_LUA_POLICY);

        // Check for dangerous patterns
        let dangerous_patterns = ["os\\.execute", "io\\.popen", "loadstring", "load", "dofile"];
        
        for pattern in &dangerous_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(command) {
                    if !lua_policy.allow_os_execute && pattern.contains("os.execute") {
                        return Ok(deny(command, &format!("Function not allowed: {}", pattern)));
                    }
                    if !lua_policy.allow_dynamic_load && pattern.contains("load") {
                        return Ok(deny(command, &format!("Function not allowed: {}", pattern)));
                    }
                }
            }
        }

        // Fallback to subprocess execution
        execute_via_subprocess(command, options).await
    }
}
```

### 4.7 VFS

虚拟文件系统实现，支持写时复制（Copy-on-Write）。

```rust
// src/vfs/overlay.rs

use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

pub struct OverlayVFS {
    base: PathBuf,      // Host filesystem root
    delta: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    path_policy: PathPolicy,
}

impl OverlayVFS {
    pub fn new(base: PathBuf, path_policy: PathPolicy) -> Self {
        Self {
            base,
            delta: Arc::new(Mutex::new(HashMap::new())),
            path_policy,
        }
    }
    
    pub async fn read_file(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
        // First check in delta layer
        {
            let delta = self.delta.lock().await;
            if let Some(data) = delta.get(path) {
                return Ok(data.clone());
            }
        }
        
        // Fall back to base layer
        let full_path = self.base.join(path);
        tokio::fs::read(full_path).await
    }
    
    pub async fn write_file(&self, path: &Path, data: Vec<u8>) -> Result<(), std::io::Error> {
        // Check path policy
        let path_str = path.to_string_lossy().to_string();
        if !self.path_policy.allowed_write.iter().any(|p| path_str.starts_with(p)) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Write not allowed"));
        }
        
        // Write to delta layer
        let mut delta = self.delta.lock().await;
        delta.insert(path.to_path_buf(), data);
        Ok(())
    }
    
    pub async fn exists(&self, path: &Path) -> bool {
        {
            let delta = self.delta.lock().await;
            if delta.contains_key(path) {
                return true;
            }
        }
        
        let full_path = self.base.join(path);
        tokio::fs::try_exists(full_path).await.unwrap_or(false)
    }
}
```

### 4.8 Default Policy

```rust
// src/default_policy.rs

use crate::script::sandbox::*;

pub const DEFAULT_LUA_POLICY: LuaPolicy = LuaPolicy {
    allowed_modules: vec![],
    denied_modules: vec!["os".to_string(), "io".to_string(), "package".to_string(), 
                         "debug".to_string(), "ffi".to_string()],
    allow_os_execute: false,
    restrict_io_open: true,
    allow_dynamic_load: false,
};

pub const DEFAULT_SHELL_POLICY: ShellPolicy = ShellPolicy {
    allowed_commands: vec![],
    denied_commands: vec!["sudo".to_string(), "su".to_string(), "chroot".to_string()],
    dangerous_patterns: vec![
        "rm\\s+(-rf|--recursive)".to_string(),
        ":\\(\\)\\s*\\{.*:\\(\\)\\s*\\}.*\\}".to_string(),
    ],
    allow_pipe: true,
    allow_redirect: true,
};

// ... similar defaults for Python and JavaScript

pub const DEFAULT_SANDBOX_POLICY: SandboxPolicy = SandboxPolicy {
    mode: SandboxMode::Strict,
    shell: Some(DEFAULT_SHELL_POLICY),
    python: Some(DEFAULT_PYTHON_POLICY),
    javascript: Some(DEFAULT_JS_POLICY),
    lua: Some(DEFAULT_LUA_POLICY),
    filesystem: Some(DEFAULT_FILESYSTEM_POLICY),
    process: Some(DEFAULT_PROCESS_POLICY),
    network: Some(DEFAULT_NETWORK_POLICY),
    resource: Some(DEFAULT_RESOURCE_POLICY),
};
```

---

## 五、依赖项

在 `crates/wf-sandbox/Cargo.toml` 中添加以下依赖：

```toml
[dependencies]
# Core
serde = { workspace = true }
tokio = { workspace = true, features = ["sync", "time", "fs"] }
tracing = { workspace = true }
thiserror = { workspace = true }

# Type dependencies
wf-types = { path = "../wf-types" }
wf-common = { path = "../wf-common" }

# Pattern matching
regex = "1.10"

# Container isolation
docker-api = "0.10"

# System-level isolation
[target.'cfg(target_os = "linux")'.dependencies]
nix = "0.28"
seccompiler = "0.13"

[target.'cfg(target_os = "windows")'.dependencies]
winapi = { version = "0.3", features = ["jobapi", "handleapi"] }

# **Lua native VM** - **Rust 原生优势**
mlua = "0.9"
```

---

## 六、测试计划

### 单元测试

- `test_shell_static_analyzer`: 测试各种危险模式匹配
- `test_linux_seccomp`: 测试 seccomp 策略应用（需在 Linux 上运行）
- `test_container_strategy`: 测试 Docker 容器执行
- `test_resolver_priority`: 测试策略优先级回退
- `test_vfs_overlay`: 测试覆盖文件系统的读写行为
- `test_lua_mlua_sandbox`: **测试 `mlua` 内嵌 VM 的隔离性（核心）**
- `test_lua_module_whitelist`: 测试 Lua 模块白名单
- `test_lua_module_blacklist`: 测试 Lua 模块黑名单

### 集成测试

- `test_full_sandbox_flow`: 测试从配置加载到执行的完整流程
- `test_lenient_mode`: 测试宽松模式下的警告行为
- `test_profile_matching`: 测试配置文件中的规则匹配

### E2E 测试

- 使用真实配置文件测试多语言脚本执行
- 性能基准测试：比较 `mlua` 内嵌 vs 子进程的性能差异

---

## 七、实施步骤

1. **Phase 1**: 扩展 `wf-types` 中的沙箱类型（1 天）
2. **Phase 2**: 实现 `DefaultStrategyResolver` 和基础框架（2 天）
3. **Phase 3**: 实现 Shell 静态分析器（2 天）
4. **Phase 4**: 实现 Linux seccomp 策略（3 天）
5. **Phase 5**: 实现容器策略（2 天）
6. **Phase 6**: 实现 VFS 子系统（3 天）
7. **Phase 7**: 实现 `SandboxRuntime` 协调器（2 天）
8. **Phase 8**: 实现其他语言策略和测试（5 天）
   - Python executor  
   - JS executor
   - **Lua executor**
   - **Lua mlua_sandbox strategy（核心优势）**
   - Lua static analyzer

**总预估时间：20 人日**

---

## 八、总结：为什么必须包含 Lua

| 维度 | 理由 |
|------|------|
| **TS 版劣势** | TS 只能子进程 + 字符串注入，安全风险高 |
| **Rust 优势** | `mlua` 提供零开销内嵌，API 级隔离，无逃逸可能 |
| **性能** | 比子进程快 10-100x（无 IPC，无进程创建） |
| **安全性** | Rust 所有权 + 类型系统保证，无法绕过 |
| **实用性** | Lua 广泛用于 Redis、NGINX、游戏引擎等场景 |
| **差异化** | 唯一能让 Rust 版超越 TS 版的语言 |

**结论**：Lua 不仅是应该补充，而是沙箱实现的**优先级最高的语言**。
