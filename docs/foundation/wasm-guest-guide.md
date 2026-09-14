# WASM 插件作者指南

> 面向插件作者。宿主实现见 `crates/infra/wf-plugin/src/wasm/`，
> 契约定义见 `crates/foundation/wf-plugin-sdk/src/wasm.rs`。
> 完整可构建示例见 `crates/infra/wf-plugin/examples/wasm-echo/`。

---

## 一、总体模型

Wasm 插件是一个标准的 Core Module（非 Component），以 JSON 为唯一的
数据编码，与宿主通过线性内存传递字符串：

- 宿主负责：编译模块、按权限组装 WASI 上下文、每次调用创建独立
  `Store`（调用间无共享内存）、fuel/epoch/内存上限、超时中断。
- 插件负责：导出 `memory` 与 `alloc`，实现 `wf_*` 导出函数，
  用 JSON 描述贡献并处理分发调用。

`plugin.toml` 中 `plugin_type` 可省略，`entry_point` 以 `.wasm`
结尾即自动识别为 Wasm 插件。

---

## 二、导出约定

| 导出 | 签名 | 必需 | 说明 |
|---|---|---|---|
| `memory` | linear memory | 是 | 宿主读写 JSON 的通道 |
| `alloc` | `(size: i32) -> i32` | 有输入调用时必需 | bump 分配器即可 |
| `dealloc` | `(ptr: i32, len: i32)` | 否 | 缺省时泄漏以单次调用为界 |
| `wf_on_load` 等 5 个钩子 | `(ptr: i32, len: i32) -> i32` | 否 | 缺省视同成功；返回非 0 即失败 |
| `wf_register` | `() -> i64` | 有贡献时必需 | 返回打包的 `(ptr, len)`，指向贡献声明 JSON |
| `wf_dispatch` | 6×i32 → i64 | 有贡献时必需 | 见下 |

`wf_register` 返回的 JSON 形如：

```json
{
  "node_types": [],
  "tool_types": ["echo"],
  "llm_providers": [],
  "formatters": [],
  "event_handlers": [],
  "middleware": [{"phase": "pre_tool", "priority": 0}]
}
```

未知字段会被忽略，便于向前兼容。

`wf_dispatch(type_ptr, type_len, name_ptr, name_len, input_ptr, input_len)`
返回输出 JSON 的打包 `(ptr, len)`。打包规则：低 32 位为指针，
高 32 位为长度。`handler_type` 取值为
`node/tool/llm/event/mw` 之一，输入输出 JSON 结构与 Native 插件的
`dispatch` 完全一致。

生命周期钩子的输入为 `{"plugin_id": "...", "config": {...}}`。

---

## 三、Rust 最小实现

参考 `examples/wasm-echo/`，要点如下：

1. `no_std` + `crate-type = ["cdylib"]`，目标
   `wasm32-unknown-unknown`，不依赖 WASI。
2. 用静态字节数组做 bump 分配器，`memory` 由编译器自动导出。
3. 返回静态 JSON 时同样走"拷贝进 guest 内存再打包返回"的路径，
   不要返回宿主无法寻址的指针。
4. `panic = "abort"` 并提供 `panic_handler` 使 panic 直接 trap。
5. release profile 开 `lto` 与 `opt-level = "s"`，示例产物约 0.5KB。

动态逻辑（如解析输入 JSON）建议先用最小的手写解析起步，
待组件模型（`wit/plugin.wit`）落地后再迁移到 `wit-bindgen`
生成的强类型绑定。

---

## 四、manifest 参考

```toml
id = "my-plugin"
version = "1.0.0"
entry_point = "plugin.wasm"

[wasm]
memory_max_mb = 64      # 缺省 64，0 为非法
fuel_limit = 10000000   # 缺省 1000 万，0 表示不设限
call_timeout_ms = 10000 # 缺省跟随引擎 guard 超时
max_module_bytes = 33554432  # 缺省 32MiB
allowed_dirs = ["./data"]    # 需同时声明 filesystem 权限
allowed_env_prefixes = ["MYAPP_"]  # 需同时声明 environment 权限
allow_network = false   # 本阶段恒为拒绝，仅做策略记录
```

权限声明沿用 `permissions` 数组。注意：授权列表没有对应权限时
不授予任何能力；`shell` 权限对 wasm 恒为拒绝，需要执行命令的
插件应通过宿主工具贡献间接调用。

---

## 五、调试指南

| 现象 | 含义 | 排查 |
|---|---|---|
| `does not export 'memory'/'alloc'` | 导出缺失 | 检查 `crate-type` 与 `#[no_mangle]`，确认链接后符号存在（`wasm-tools print` 或 `wasm2js` 查看） |
| `hook 'wf_on_load' returned N` | 钩子返回非 0 | N 为 guest 返回码；WASI 调用返回的 errno 也会原样透出 |
| `Timeout` | epoch 中断触发 | guest 陷入长循环；检查 `call_timeout_ms` 与 fuel 预算 |
| `exhausted its fuel budget` | fuel 耗尽 | 提高 `fuel_limit`，或检查意外循环 |
| `oversized message` | guest 返回的 `(ptr, len)` 越界 | 检查打包顺序（低 32 位指针，高 32 位长度）与分配器 |
| `register decl parse failed` | 声明 JSON 非法 | 确认 `wf_register` 指向的内存为完整 UTF-8 JSON |
| `does not export 'wf_dispatch'` | 声明了贡献但无分发 | 补齐 `wf_dispatch` 或清空声明 |
| 预开目录不可见 | 授权未生效 | 确认 `permissions` 含 `filesystem` 且目录在宿主存在；启动日志会打印实际授予清单 |

单条消息上限 4MiB，模块上限 32MiB（均可在 manifest 侧收紧，
前者为宿主硬顶）。

---

## 六、已知限制（本阶段）

- 中间件语义为简化版：guest 返回 JSON 布尔值决定是否继续，
  不支持 guest 回调宿主的 `next` 链之外的双向调用。
- 网络能力恒关闭；`stdout/stderr` 默认接入黑洞（写入成功但无处可去）。
- 每次调用新建 `Store` + 实例化，超高频场景有优化空间（实例池在路线图中）。
- 组件模型与 WIT 绑定尚未实现，`wit/plugin.wit` 仅为草案。
