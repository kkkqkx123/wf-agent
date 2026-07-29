# Rust 插件系统设计方案

> date: 2026-07-29

## 概述

基于 TS 版插件系统架构设计 Rust 版插件系统，实现两种插件形式：**Lua 脚本插件**（轻量、安全）和**动态库插件**（高性能、原生）。依赖图中新增 `wf-plugin` crate，位于 `wf-core` 下游、`wf-execution-shared` 上游。

```
wf-core → wf-plugin → wf-execution-shared → wf-agent → wf-workflow
```

## 一、TS 版架构参考

### 1.1 核心模块

| TS 模块 | 职责 |
|---|---|
| `PluginEngine` | 生命周期编排：discover → load → activate → deactivate → unload |
| `PluginLoader` | 文件系统扫描、manifest 解析、模块动态加载 |
| `PluginRegistry` | 插件记录存储、状态追踪 |
| `PluginDependencyResolver` | 拓扑排序、循环检测、semver 校验 |
| `PluginGuard` | 超时控制、错误隔离 |
| `ContributionManager` | 7 种贡献类型的注册/查询/冲突管理 |
| `ContributionBridge` | 将插件贡献同步到 SDK 注册表 |
| `PluginEventBus` | 生命周期事件的发布/订阅 |
| `PluginContext` | 插件运行时上下文（DI 容器、日志、配置） |

### 1.2 生命周期阶段

```
DISCOVERED → LOADING → LOADED → ACTIVATING → ACTIVE
                                            ↓
                                   DEACTIVATING → DEACTIVATED
```

激活顺序：`onLoad` → `registerContributions` → `bridge.sync` → `onActivate`

### 1.3 七种贡献类型

`node-type`, `tool-type`, `llm-provider`, `formatter`, `hook-handler`, `event-handler`, `middleware`

## 二、Rust 版设计

### 2.1 crate 结构

新增 `crates/wf-plugin`，依赖 `wf-types`、`wf-common`、`wf-core`。

```
wf-plugin/src/
├── lib.rs                   # pub mod + pub use
├── manifest.rs              # PluginManifest 定义
├── engine.rs                # PluginEngine 生命周期管理器
├── registry.rs              # PluginRegistry 状态管理
├── loader.rs                # PluginLoader：文件扫描、校验
├── guard.rs                 # PluginGuard 超时/错误隔离
├── plugin.rs                # Plugin trait 定义
├── context.rs               # PluginContext 上下文
├── events.rs                # 生命周期事件
├── contributions/
│   ├── mod.rs               # pub mod
│   ├── manager.rs           # ContributionManager
│   ├── types.rs             # ContributionType、abstractions
│   ├── bridge.rs            # ContributionBridge
│   └── registries/
│       ├── mod.rs
│       ├── node_type.rs     # NodeTypeRegistry
│       ├── tool_type.rs     # ToolTypeRegistry
│       ├── event_handler.rs
│       ├── hook_handler.rs
│       ├── llm_provider.rs
│       ├── formatter.rs
│       └── middleware.rs
├── lua/
│   ├── mod.rs
│   ├── runtime.rs           # LuaRuntime 封装
│   ├── loader.rs            # Lua 插件加载器
│   └── sandbox.rs           # Lua 沙箱策略
└── native/
    ├── mod.rs
    ├── loader.rs            # 动态库加载器
    └── abi.rs               # C ABI 定义
```

### 2.2 Plugin trait 定义

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    async fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_unload(&self, ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_deactivate(&self, ctx: &PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_config_change(&self, config: &Value) -> PluginResult<()> { Ok(()) }

    fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar);
}
```

### 2.3 两种 Plugin 实现

#### LuaPlugin

```rust
pub struct LuaPlugin {
    manifest: PluginManifest,
    lua_ctx: LuaContext,
}

// LuaPlugin 通过 mlua 加载 .lua 文件，将 Lua 函数映射到生命周期钩子
// 插件代码通过 Lua 注册贡献：
//   plugin:register_node_type("my_type", { execute = function(ctx) ... end })
```

Lua 插件目录结构：
```
plugins/my-plugin/
├── plugin.toml              # 清单文件
├── main.lua                 # 入口
└── lib/                     # 可选辅助库
```

`plugin.toml` 示例：
```toml
[plugin]
id = "my-lua-plugin"
version = "1.0.0"
entry_point = "main.lua"
contributions = ["node-type", "tool-type"]
```

#### NativePlugin

```rust
pub struct NativePlugin {
    manifest: PluginManifest,
    lib: Arc<libloading::Library>,
    // 从动态库中加载的函数指针
}
```

C ABI 定义 (`wf-plugin/abi.h` 或 Rust 端的 `extern "C"` 声明)：
```c
// 每个插件必须实现的入口函数
int wf_plugin_get_manifest(void *out, size_t *out_len);
int wf_plugin_on_load(void *ctx);
int wf_plugin_register_contributions(void *registrar);
int wf_plugin_on_activate(void *ctx);
int wf_plugin_on_deactivate(void *ctx);
int wf_plugin_on_unload(void *ctx);
```

Native 插件目录结构：
```
plugins/my-plugin/
├── plugin.toml              # 清单文件
├── libmy_plugin.so          # 编译产物（平台相关）
└── assets/                  # 可选资源
```

`plugin.toml` 示例：
```toml
[plugin]
id = "my-native-plugin"
version = "1.0.0"
entry_point = "libmy_plugin.so"
contributions = ["tool-type", "middleware", "event-handler"]
```

### 2.4 Contribution 系统

与 TS 版一致，插件通过 `ContributionRegistrar` 注册 7 种贡献。

```rust
pub trait ContributionRegistrar {
    fn register_node_type(&mut self, type_name: &str, handler: Box<dyn PluginNodeHandler>);
    fn register_tool_type(&mut self, type_name: &str, executor: Box<dyn PluginToolExecutor>);
    fn register_llm_provider(&mut self, name: &str, formatter: Box<dyn PluginLLMFormatter>);
    fn register_formatter(&mut self, name: &str, formatter: Box<dyn PluginLLMFormatter>);
    fn register_event_handler(&mut self, event_type: &str, handler: Box<dyn PluginEventHandler>);
    fn register_hook_handler(&mut self, hook_type: &str, handler: Box<dyn PluginHookHandler>);
    fn register_middleware(&mut self, mw: PluginExecutionMiddleware);
}
```

```rust
// 插件侧的无依赖抽象
#[async_trait]
pub trait PluginNodeHandler: Send + Sync {
    fn node_type(&self) -> &str;
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult>;
}
```

**Bridge 机制**：`ContributionBridge` 将 `ContributionManager` 中的插件贡献同步到现有的 Rust 注册表：

| 插件贡献类型 | 桥接目标 |
|---|---|
| `node-type` | `wf-workflow::HandlerRegistry` （作为 `NodeHandler` 适配） |
| `tool-type` | `wf-tools::ToolRegistry` （新的 `PluginExecutor` 包装） |
| `llm-provider` | `wf-llm` 的 provider 注册表 |
| `formatter` | `wf-llm` 的 formatter 注册表 |
| `event-handler` | `wf-core::EventBus` 订阅者 |
| `hook-handler` | `wf-execution-shared::HookHandlerRegistry` |
| `middleware` | 执行管道的中间件链 |

### 2.5 运行时集成

#### 新建 `wf-plugin` crate 的依赖位置

```
wf-core  wf-sandbox  wf-config
    ↓         ↓
   wf-plugin
    ↓
wf-execution-shared  →  wf-tools  wf-llm
    ↓
wf-agent  →  wf-workflow
```

`wf-plugin` 依赖 `wf-core`（EventBus、ConcurrentRegistry）和 `wf-sandbox`（Lua 沙箱复用），不依赖 `wf-tools`/`wf-llm`/`wf-execution-shared`。Bridge 实现放在 `wf-workflow` 或 `wf-runtime` 层。

#### 初始化流程（在 wf-runtime 中编排）

```
1. Runtime::bootstrap()
2.   ├── 解析 plugins 配置
3.   ├── 创建 PluginEngine
4.   ├── engine.initialize()
5.   │   ├── scan plugins dirs
6.   │   ├── load manifests
7.   │   ├── resolve dependencies
8.   │   ├── load modules (Lua files / .so files)
9.   │   └── activate all
10.  ├── bridge.sync_all() → HandlerRegistry, ToolRegistry, EventBus, etc.
11.  └── 继续启动其他组件
```

#### Lua Runtime 复用

复用 `wf-sandbox` 已有的 `mlua` 依赖和沙箱策略。Lua 插件使用独立的 Lua 状态机实例运行，每个插件一个 `lua::Lua` instance，通过沙箱限制资源访问。

沙箱策略：复用 `wf-sandbox::SandboxPolicyManager`，提供针对 Lua 插件的默认安全策略：
- 禁用 `io`、`os`、`loadfile` 等危险库
- 限制内存使用量（通过 Lua GC 回调）
- 限制执行超时（通过 `lua.set_timeout()` 或 hook 计时器）
- 允许访问插件自身目录、拒绝访问外部目录

#### Native Plugin ABI 细节

```rust
// wf-plugin/native/abi.rs
#[repr(C)]
pub struct PluginAbi {
    pub abi_version: u32,
    pub get_manifest: extern "C" fn(*mut u8, *mut usize) -> i32,
    pub on_load: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub register_contributions: Option<extern "C" fn(*mut ContributionRegistrarC) -> i32>,
    pub on_activate: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_deactivate: Option<extern "C" fn(*const PluginContextC) -> i32>,
    pub on_unload: Option<extern "C" fn(*const PluginContextC) -> i32>,
}
```

数据传递通过 JSON 序列化（`serde_json::Value` 序列化后在 C 侧的 `char*` 中传递）。ContributionRegistrar 通过一组 C 回调函数暴露。

### 2.6 插件配置

```rust
pub struct PluginSystemConfig {
    pub enabled: bool,
    pub paths: Vec<PathBuf>,
    pub auto_activate: bool,
    pub guard_timeout_ms: u64,
    pub override_policy: OverridePolicy,
    pub allow_list: Vec<String>,
    pub block_list: Vec<String>,
    pub config: HashMap<String, Value>,  // per-plugin config
}

pub enum OverridePolicy {
    Forbid,
    Warn,
    Allow,
    Priority,
}
```

### 2.7 依赖解析

复用 TS 版的 Kahn 拓扑排序算法。依赖来自 `plugin.toml` 中的 `dependencies` 字段。

```rust
pub fn resolve_dependencies(
    manifests: &[PluginManifest],
) -> Result<ResolvedGraph, DependencyError>;

pub struct ResolvedGraph {
    pub load_order: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub missing: Vec<String>,
}
```

### 2.8 Guard 机制

```rust
pub struct PluginGuard {
    timeout: Duration,
}

impl PluginGuard {
    pub async fn execute<F, T>(&self, plugin_id: &str, f: F) -> PluginResult<T>
    where
        F: Future<Output = PluginResult<T>> + Send,
    {
        tokio::time::timeout(self.timeout, f).await
            .map_err(|_| PluginError::Timeout { plugin_id: plugin_id.to_owned() })?
    }
}
```

## 三、实现优先级

### P0 — 基础框架

| 任务 | 说明 |
|---|---|
| `wf-plugin` crate 骨架 | Cargo.toml、lib.rs 模块声明 |
| `PluginManifest` 类型 | 清单序列化/反序列化 |
| `Plugin` trait | 完整生命周期接口 |
| `PluginEngine` | 发现→加载→激活全流程 |
| `PluginRegistry` | DashMap 状态追踪 |
| `PluginGuard` | 超时控制 |
| 单元测试 | 覆盖核心生命周期 |

### P1 — Lua 插件

| 任务 | 说明 |
|---|---|
| `LuaPlugin` 实现 | 基于 mlua 加载 lua 文件 |
| Lua 沙箱策略 | 复用 wf-sandbox 的 sandbox 配置 |
| Lua 侧 API | 贡献注册函数映射 |
| 集成测试 | 测试 Lua 插件完整生命周期 |

### P1 — 贡献系统

| 任务 | 说明 |
|---|---|
| `ContributionManager` | 注册/查询/冲突管理 |
| `ContributionRegistrar` trait | 7 种接口 |
| 7 个子注册表实现 | 内部存储 |
| `ContributionBridge` | 桥接到现有注册表 |
| OverridePolicy | 冲突策略（Forbid/Warn/Allow/Priority） |

### P2 — Native 插件

| 任务 | 说明 |
|---|---|
| C ABI 定义 | 函数指针表 |
| `NativePlugin` 实现 | 基于 libloading |
| JSON 序列化桥接 | serde_json ↔ C char* |
| 集成测试 | 测试 native 插件完整生命周期 |

### P2 — 高级功能

| 任务 | 说明 |
|---|---|
| 依赖解析器 | 拓扑排序、循环检测 |
| 热重载 | 重新扫描 + 重新激活 |
| 事件总线 | 生命周期事件发布/订阅 |
| wf-runtime 集成 | 初始化编排 |

## 四、关键设计决策

### 4.1 为什么新增 `wf-plugin` crate？

- `wf-tools`、`wf-llm`、`wf-execution-shared` 都是被上层使用的，如果插件系统依赖它们会形成循环依赖
- `wf-plugin` 只定义抽象 trait（Plugin、PluginNodeHandler、PluginToolExecutor 等）和贡献注册接口
- Bridge 适配器放在 `wf-workflow` 或 `wf-runtime` 层（因它们已依赖所有需要桥接的 crate）

### 4.2 为什么 Lua 插件优先于 Native？

- Lua 更安全（沙箱完善，wf-sandbox 已有 mlua 依赖）
- Lua 开发成本低（无需处理 FFI 和 C ABI）
- Lua 热更新天然支持（重新加载 lua 文件即可）
- 适合大部分"配置型"和"轻量逻辑型"插件

### 4.3 ContributionRegistrar 设计

TS 版使用对象属性（`registrar.nodeTypes.registerNodeType(...)`），Rust 版使用统一方法签名。原因是 Rust 的 trait 对象不支持关联字段，统一方法签名更符合 Rust 习惯。

### 4.4 序列化策略

Native 插件数据传递统一使用 JSON 序列化（serde_json），避免复杂的内存共享协议。Lua 插件通过 mlua 的 Value 类型直接传递表。

### 4.5 桥接时机

Bridge 不放在 `wf-plugin` 内部，而是放在依赖了所有目标注册表的 crate（`wf-workflow` 或 `wf-runtime`）中。`wf-plugin` 只定义 `ContributionBridge` trait，由上层注入具体实现。

## 五、与 TS 版差异对比

| 维度 | TS 版 | Rust 版 |
|---|---|---|
| 插件形式 | JS 模块（ESM import） | Lua 脚本 + 动态库 (.so/.dylib/.dll) |
| 安全隔离 | 无（同进程信任） | Lua 沙箱 + Native 无沙箱 |
| 序列化 | 直接传对象引用 | JSON（Native）、mlua Value（Lua） |
| 贡献注册 | 7 个子属性访问器 | 统一 trait 方法 |
| DI 容器 | tsyringe 容器 | Rust 侧使用 Arc 直接传递依赖 |
| 热重载 | import() cache 问题 | Lua：重新加载文件；Native：重新 dlopen |
| sdkVersion | semver 校验 | 固定版本号校验（简化） |
