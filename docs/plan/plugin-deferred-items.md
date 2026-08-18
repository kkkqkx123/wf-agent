# Plugin 待决项决策

## 1. Lua Mutex 串行化优化

**决策：放弃**

`Arc<Mutex<mlua::Lua>>` 是正确的设计。Lua VM 本身是单线程模型，同一状态无法并行执行；不同插件各自有独立 state + Mutex，已经可以并行。按 category 拆分 state 会引入跨 state 的 `RegistryKey` 无效问题，且无法共享闭包。当前不存在性能问题。

---

## 2. `from_lua_value` 数字精度丢失

**决策：修改**

NaN/Infinity 静默转换为 0 是 bug。

### 实现

在 `from_lua_value` 的 `Number(n)` 分支增加守卫：

```rust
mlua::Value::Number(n) => {
    if n.is_nan() || n.is_infinite() {
        Some(Value::Null)
    } else {
        Some(Value::Number(serde_json::Number::from_f64(n).unwrap_or(0.into())))
    }
}
```

不改签名，不连锁影响，2 行改动。

### 修改范围

- `crates/wf-plugin/src/lua/plugin.rs:91-93`

---

## 3. ContributionBridge 实装

**决策：放弃**

`ContributionBridge` 是可选的外部同步接口（`Option<Arc<dyn ContributionBridge>>`）。内部存储已由 `ContributionManager` 完整实现（7 个 registry + query methods），消费者可以直接通过 `get_node_handler()`、`get_tool_executor()` 等获取 handler。在外部消费系统（wf-tools、wf-workflow）接口稳定前，bridge 保持 trait 定义即可，不需要默认实现。当前无功能缺失。

---

## 4. Manifest `sdk_version` 校验升级为 error

**决策：修改**

当前 `sdk_version` 不匹配仅 `tracing::warn`，不会阻止加载。这将导致不兼容的插件在运行时行为异常。

### 实现

`engine.rs:108-119` 将 `warn` 改为 `Err`：

```rust
if !req.matches(&ver) {
    return Err(PluginError::InvalidManifest(format!(
        "plugin sdk version '{}' not satisfied by host '{}'", sdk_req, self.sdk_version
    )));
}
```

### 修改范围

- `crates/wf-plugin/src/engine.rs` `load_plugin` 方法

---

## 5. 扩展名检测的 `_base` 参数未使用

**决策：修改**

`load_plugin_module_with_base` 接收 manifest 父目录 `_base` 但丢弃了它；Lua/Native loader 内部独立重新计算 base path。当 manifest 不在标准目录布局中时（如 `load_single` 加载任意位置 manifest），路径解析会失败。

### 实现

`engine.rs:533-538`：将 `_base` 转发给 loader：

```rust
async fn load_plugin_module_with_base(manifest: &PluginManifest, base: &Path) -> PluginResult<Arc<dyn Plugin>> {
    match resolve_plugin_type(manifest)? {
        PluginType::Lua => load_lua_plugin_with_base(manifest, base).await,
        PluginType::Native => load_native_plugin_with_base(manifest, base),
    }
}
```

**Lua loader** (`lua/loader.rs`)：新增 `load_lua_plugin_with_base`，当提供 `base` 时直接 `base.join(&manifest.entry_point)`，不再调用 `determine_base_path`：

```rust
pub async fn load_lua_plugin_with_base(manifest: &PluginManifest, base: &Path) -> PluginResult<Arc<dyn Plugin>> {
    let entry_path = base.join(&manifest.entry_point);
    // ... 其余逻辑不变
}
```

**Native loader** (`native/loader.rs`)：同理，新增 `load_native_plugin_with_base`。

已有调用点 `load_single`（line 150）传入了正确 `plugin_dir`；`discover` 和 `reload` 路径使用无 base 的重载（依然走目录搜索 fallback）。

### 修改范围

- `crates/wf-plugin/src/engine.rs:533-538`
- `crates/wf-plugin/src/lua/loader.rs`: 新增 `load_lua_plugin_with_base`
- `crates/wf-plugin/src/native/loader.rs`: 新增 `load_native_plugin_with_base`

---

## 6. `register_contributions` 是同步方法

**决策：放弃**

`register_contributions` 仅做内存级别的 handler 注册（Lua 侧获取 `RegistryKey`、Native 侧保存函数指针），不涉及 I/O 或阻塞操作。`Plugin::register_contributions` 签名保持同步是合理的：
- Lua 侧在 phase 1 持有 `Mutex` 提取 `RegistryKey`，phase 2 注册到 `registrar` → 纯内存操作
- Native 侧只是回调 FFI 函数指针，同步执行
- 注册仅在 `activate` 时执行一次，不是性能或并发关键路径

用 `spawn_blocking` 包装反而增加不必要开销。
