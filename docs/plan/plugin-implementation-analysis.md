# Plugin 模块实现分析与改造方案

## 概述

本文档基于 `crates/wf-plugin` 的源代码，系统分析 Lua 插件和 Native（动态库）插件的实现现状，评估是否符合最佳实践，并分阶段推进改造。

---

## 1. 配置格式分析：是否应改用 TOML

### 现状

| 配置项 | 当前格式 | 位置 |
|--------|----------|------|
| `PluginManifest` | `toml::from_str`（**已用 TOML**） | `engine.rs:492` / `manifest.rs` |
| `PluginSystemConfig.config` | `HashMap<String, serde_json::Value>` | `engine.rs:27` |
| `PluginContext.config` | `serde_json::Value` | `context.rs:10` |
| Plugin 内 config 字段 | `Option<serde_json::Value>` | `manifest.rs:19` |

### 分析

- `PluginManifest` 已使用 TOML 解析（`toml::from_str`），反序列化为 Rust struct
- `wf-config` 生态也以 TOML 为主格式（`Cargo.toml` 中有 `toml` 依赖，`parser.rs` 以 TOML 为首选）
- **问题在于** `PluginSystemConfig.config` 使用 `serde_json::Value` 存储 per-plugin 自定义配置。这意味着：
  - 即使整个配置管线是 TOML，plugin 内部 config 字段仍然使用 JSON Value
  - Lua/Native handler 在用 `serde_json::Value`，而 Rust 生态中 `toml::Value` 更一致
  - 但在 Lua 侧需要 JSON 序列化（`serde_json::to_string` 给 native 插件传递配置），TOML 不适合直接跨语言交换

### 建议

| 组件 | 建议 | 理由 |
|------|------|------|
| `PluginManifest` | **保留 TOML**（已是最佳实践） | 与 `wf-config` 一致，Rust 原生支持 |
| `PluginSystemConfig.config` | **保持 `Value`**，但限制为仅内部使用 | 该 map 最终传给 `PluginContext`，在 Lua 侧和 Native 侧都需要 JSON 化传递 |
| `PluginContext.config` | **保持 `Value`** | 是跨语言序列化的中间格式，JSON 是最通用的选择 |
| `manifest.config` | **支持同时解析 TOML table** | 用 `#[serde(untagged)]` 或 `serde_json::Value` 兼容任意格式 |

**结论**：Plugin 的 manifest 文件格式（`plugin.toml`）已经是 TOML，无需改动。Per-plugin 自定义配置需要跨语言传递（Lua/Native），使用 `serde_json::Value` 作为中性格式是合理的选择。**不强行统一为 TOML**。

---

## 2. Lua 插件实现分析

### 加载流程 (`lua/loader.rs`)

```rust
let lua = mlua::Lua::new();                    // 全新 Lua state
let result: mlua::Value = lua.load(&script).eval();  // 执行脚本
// 期望返回 table -> 设到全局 plugin
Arc::new(LuaPlugin::new(manifest.clone(), lua))
```

### 生命周期 (`lua/plugin.rs`)

| Hook | 实现 |
|------|------|
| `on_load` | `call_hook("on_load")` → 全局 `plugin` table 中查找函数 → call |
| `on_unload` | 同上 |
| `on_activate` | 同上 |
| `on_deactivate` | 同上 |
| `register_contributions` | 内嵌 Lua 辅助脚本提取贡献 → 创建 `RegistryKey` → 注册 Rust Handler |

### 发现的缺陷

| # | 严重度 | 问题 | 位置 |
|---|--------|------|------|
| L1 | **高** | `futures::executor::block_on` 桥接 async next() | `plugin.rs:248` |
| L2 | **中** | `Arc<Mutex<mlua::Lua>>` 全局锁，所有 handler 串行化 | `plugin.rs:17` |
| L3 | **中** | 无任何 Lua 沙箱（`mlua::Lua::new()` 开放全部 stdlib） | `loader.rs:16` |
| L4 | **中** | 内嵌 50 行 Lua 字符串提取贡献，运行时编译，不可维护 | `plugin.rs:293-338` |
| L5 | **低** | `lua.create_table().unwrap()` 多次 — OOM 时 panic | `plugin.rs:43-69` |
| L6 | **低** | `on_config_change` 未传递给 Lua 侧 | 缺失实现 |
| L7 | **低** | `call_hook` 静默忽略缺失的函数，无日志 | `plugin.rs:34` |
| L8 | **低** | `from_lua_value` 中数字转换 `unwrap_or(0.into())` 静默吞精度 | `plugin.rs:81` |

---

## 3. Native 插件实现分析

### ABI (`native/abi.rs`)

```c
// C 侧导出的 6 个符号
wf_plugin_get_manifest(buf, len) -> i32          // 必需
wf_plugin_on_load(ctx) -> i32                    // 可选
wf_plugin_on_unload(ctx) -> i32
wf_plugin_on_activate(ctx) -> i32
wf_plugin_on_deactivate(ctx) -> i32
wf_plugin_register_contributions(registrar) -> i32
```

`ContributionRegistrarC` 包含 7 个 `extern "C" fn` 回调指针，供 C 端插件注册贡献。

### 加载 (`native/loader.rs`)

```rust
let lib = unsafe { libloading::Library::new(&lib_path) }?;
let plugin = NativePlugin::new(manifest.clone(), lib)?;
```

### 发现的缺陷

| # | 严重度 | 问题 | 位置 |
|---|--------|------|------|
| N1 | **致命** | 所有 Handler 均为 stub（`NativeNodeHandler.execute` 返回 `Err("...stub")`） | `plugin.rs:206-253` |
| N2 | **高** | `load_manifest` 固定 4096 字节缓冲区，超限则 UB（`set_len` > `capacity`） | `abi.rs:27-33` |
| N3 | **高** | 无 ABI 版本字段，host/plugin 版本不匹配时内存错位 | `abi.rs:6-22` |
| N4 | **中** | `transmute` 对 registrar 指针语义不清（实际是空操作，但表明设计意图模糊） | `plugin.rs:64-66` |
| N5 | **低** | 缺少 `unsafe` 注释说明 FFI 安全性假设 | `plugin.rs:92-113` |
| N6 | **低** | 无 constructor 防护 — `dlopen` 时已在加载阶段执行库的静态构造器 | `loader.rs:12-15` |

---

## 4. 跨层/架构问题

| # | 严重度 | 问题 | 说明 |
|---|--------|------|------|
| C1 | **中** | Guard 错误嵌套 | `PluginError::Timeout` 被 `PluginGuardError` 再次包裹 |
| C2 | **中** | 生命周期中 `register_contributions` 是同步方法，但在异步上下文中调用可能阻塞 | `engine.rs:241-246` |
| C3 | **低** | 扩展名硬编码分发，manifest 无显式 `type` 字段 | `engine.rs:512-518` |
| C4 | **低** | `ContributionBridge` 仅为日志占位，未同步到外部注册表 | `bootstrap.rs:128` |

---

## 5. 分阶段改造方案

### Phase 1 — 配置格式评估与梳理 ✅

**目标**：处理配置格式一致性。

- [x] 分析 `PluginSystemConfig.config` 是否需要改 TOML
- [x] **结论**：保留 `serde_json::Value`，因为 config 字段需要跨语言序列化（Lua → JSON, Native → JSON via CString），TOML 不适合作为跨语言中间格式
- [x] 在 `manifest.rs` 中添加 `#[serde(deny_unknown_fields)]` 防止拼写错误

### Phase 2 — 修复 Native Plugin 关键缺陷 ✅

1. ✅ **修复 `set_len` UB** — 改用两阶段协议：先调用 `wf_plugin_get_manifest(nullptr, &len)` 获取大小，再分配 `vec![0u8; len]` 并第二次调用
2. ✅ **增加 ABI 版本** — `PluginContextC` 和 `ContributionRegistrarC` 添加 `abi_version: u32` 字段，常量 `WF_PLUGIN_ABI_VERSION = 1`
3. ✅ **实现 Native Handler Dispatch** — 新增 `wf_plugin_dispatch_handler` ABI 符号，`payload_json` 从忽略变为传递给 handler 调用。6 个 handler trait 改为通过 JSON 序列化输入 → FFI dispatch → 反序列化输出

### Phase 3 — 修复 Lua Plugin 缺陷 ✅

1. ✅ **替换 `block_on`** — Middleware 桥接改用 `tokio::task::block_in_place` + `futures::executor::block_on`，安全支持 tokio multi-thread runtime
2. ✅ **消除 `unwrap`** — 所有 `create_table()`, `set()`, `get()` 改为 `?` 或 `map_err` 返回 `PluginResult`
3. ✅ **添加沙箱** — `lua/loader.rs` 中 `apply_sandbox()` 剥离 `os`/`io`/`package`/`debug`/`ffi` 全局，替换 `print`/`require` 为安全版本

### Phase 4 — 中优先级改进 ✅

1. ⏸ **Lua Mutex 优化** — 暂缓。`Arc<Mutex<mlua::Lua>>` 持有锁的时间仅覆盖同步 `func.call()`，不涉及 await 点，在插件级别不会产生实际死锁
2. ✅ **`call_hook` 添加 debug 日志** — 当 Lua 表缺少指定 hook 函数时输出 `tracing::debug!`
3. ✅ **沙箱集成** — 已内联实现

### Phase 5 — 低优先级清理 ✅

1. ✅ **Guard 错误扁平化** — 移除 `PluginGuardError`，`guard.execute` 直接透传内部错误。同时从 `error.rs` 移除了未使用的 `PluginGuardError` 变体
2. ⏸ **添加 manifest `type` 字段** — 暂缓，当前扩展名分发机制足够可靠
3. ⏸ **完善 `on_config_change`** — 暂缓，需要同时改动 Lua 和 Native 侧
