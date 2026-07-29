# Plugin 剩余问题分析与改造方案（高/中优先级）

## 1. Manifest `type` 字段替代扩展名检测

### 现状

`engine.rs:509-518` 通过文件后缀判断插件类型：

```rust
if entry.ends_with(".lua")      -> LuaPlugin
if entry.ends_with(".so|.dylib|.dll") -> NativePlugin
```

同时 `load_plugin_module_with_base` 接收 `_base: &Path` 参数但完全未使用。

### 问题

- 无法表达 LuaJIT bytecode (`.lbc`)、WASM 等非常规后缀
- 扩展名与类型绑死，用户无法显式指定
- `_base` 未使用表明目录解析逻辑缺失

### 设计方案

**manifest.rs** — 添加枚举：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Lua,
    Native,
}
```

`PluginManifest` 新增可选字段，无值时 fallback 到按扩展名推断。

**engine.rs** — `load_plugin_module` 和 `load_plugin_module_with_base` 的逻辑改为 match `manifest.plugin_type` + 解析 `_base` 参数：

```
base = match manifest.plugin_type:
  Some(Lua) | None + .lua   -> load_lua_plugin(base?)
  Some(Native) | None + .so -> load_native_plugin(base?)
  else -> error
```

---

## 2. `on_config_change` 传递到 Lua/Native 插件

### 现状

`Plugin` trait 定义 `on_config_change(&self, _config: &Value)` 默认空实现。`PluginEngine::update_plugin_config` 调用它，但 `LuaPlugin` 和 `NativePlugin` 均未覆盖。

### 问题

运行时调用 `engine.update_plugin_config("my-plugin", new_config)` 后插件侧收不到通知，无法热生效。

### 设计方案

**Lua 侧** (`lua/plugin.rs`)：
```rust
async fn on_config_change(&self, config: &Value) -> PluginResult<()> {
    let lua = self.lua.lock()?;
    let plugin_table: mlua::Table = lua.globals().get("plugin")?;
    if let Ok(hook) = plugin_table.get::<mlua::Function>("on_config_change") {
        let cfg_tbl = to_lua_value(&lua, config);
        hook.call::<_, ()>(cfg_tbl)?;
    }
    Ok(())
}
```

**Native 侧** (`native/abi.rs`)：
- 解析 `wf_plugin_on_config_change` 符号（`extern "C" fn(*const PluginContextC) -> i32`）
- 仅包含 config_json，不包含 plugin_id（简化 ABI）

---

## 3. ABI 版本强制执行

### 现状

`PluginContextC` 和 `ContributionRegistrarC` 已包含 `abi_version: u32` 字段，常量 `WF_PLUGIN_ABI_VERSION = 1`。但 `NativePlugin::new` 未校验。

### 风险

旧版 native plugin 的 struct 布局不包含 `abi_version`：
- 旧版认为 `offset_of(plugin_id) == 0`，实际新版 `offset_of(abi_version) == 0, offset_of(plugin_id) == 8`
- 表现为 plugin_id 读取为 0x00000001（abi_version 的值），导致内存安全错误

### 设计方案

在 `load_abi_info` 中增加 abi_version 校验步骤，在首次调用 `wf_plugin_get_manifest` 时读取 version 信息。具体做法：新增一个可选导出 `wf_plugin_abi_version` —— native 插件可以定义一个全局变量：

```c
const uint32_t wf_plugin_abi_version = 1;
```

host 端解析此符号，与 `WF_PLUGIN_ABI_VERSION` 比较，不匹配则拒绝加载。

---

## 4. 内嵌 Lua helper 脚本改为纯 Rust 实现

### 现状

`lua/plugin.rs:340-385` 包含一个 45 行的 Lua 字符串常量，`register_contributions` 时：
1. `lua.load(helper).eval()` 编译返回一个 Lua 函数
2. 调用该函数提取 `plugin_table.register_contributions()` 的返回内容
3. 遍历结果 table，提取函数指针到 `RegistryKey`

### 问题

- 运行时编译，无编译期检查
- 函数名（`execute`/`format`/`handle`）硬编码在字符串中，与 Rust trait 名无同步
- 类型种类（`node`/`tool`/`llm`...）用 magic number 表示（0-6）

### 设计方案

完全用 Rust + mlua API 实现：

1. 锁定 Lua → 获取 `plugin` 全局 table
2. 调用 `plugin_table.get::<_, mlua::Function>("register_contributions")` — 可选
3. 若有，调用返回 `contribs_table`
4. 在 Rust 侧用 `tbl.get::<_, mlua::Table>("node_types")` / `.pairs()` 遍历各子表
5. 每个 handler 函数直接 `create_registry_value` 存 `RegistryKey`
6. 分类后注册到 `registrar`

这样完全消除 Lua 字符串，所有逻辑在 Rust 编译期检查。
