# WASM 插件作者指南

> 面向插件作者。宿主实现见 `crates/infra/wf-plugin/src/wasm/`，
> 契约定义见 `crates/foundation/wf-plugin-sdk/src/wasm.rs`。
> 完整可构建示例见 `crates/infra/wf-plugin/examples/` 下的
> `wasm-echo/`（Rust）、`wasm-go/`（TinyGo）、`wasm-python/`（componentize-py）。

---

## 一、总体模型

Wasm 插件有两条加载路径，宿主按二进制头部自动分流，上层行为一致：

- **Core Module**（默认）：标准的 Core Module（非 Component），以 JSON 为唯一的
  数据编码，与宿主通过线性内存传递字符串（本指南第二、三节描述的即此路径）。
- **Component Model**：用 WIT（`crates/infra/wf-plugin/wit/plugin.wit`，
  `wf:plugin/plugin` 世界）编译出的 Component，走强类型绑定 + WASI p2 上下文，
  但 `register`/`dispatch` 的载荷仍为 JSON 字符串，与 Core 路径语义相同。
  生命周期钩子行为有一处差异：`on-deactivate`/`on-unload` 无输入参数，
  且 guest 必须实现完整 world（缺失导出会直接加载失败，不像 Core 路径那样视为成功）。

- 宿主负责：编译模块、按权限组装 WASI 上下文、每次调用创建独立
  `Store`（调用间无共享内存）、fuel/epoch/内存上限、超时中断、
  guest `stdout`/`stderr` 接入宿主日志。
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
| `wf_abi_version` | `() -> u32` | 否 | 缺省视为 v1；返回非 1 则加载失败 |
| `wf_last_error` | `() -> i64` | 否 | 与 `wf_register` 同一打包 `(ptr, len)`；钩子返回非 0 时宿主读取并拼入错误 |
| `wf_on_load` 等 5 个钩子 | `(ptr: i32, len: i32) -> i32` | 否 | 缺省视同成功；返回非 0 即失败，错误含 code 与 `wf_last_error` 明细 |
| `wf_register` | `() -> i64` | 有贡献时必需 | 返回打包的 `(ptr, len)`，指向贡献声明 JSON |
| `wf_dispatch` | 6×i32 → i64 | 有贡献时必需 | 见下 |

`wf_register` 返回的 JSON 形如：

```json
{
  "node_types": [],
  "tool_types": ["echo"],
  "llm_providers": [],
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
如需强类型绑定可用 `wit/plugin.wit`（`wf:plugin/plugin` 世界）经由
Component 路径（`wasm32-wasip2` + `wit-bindgen`/`jco` 等）构建。

Component 路径另有两个可直接构建的示例：

- `examples/wasm-go/`：TinyGo `wasip2` 目标配合 `wit-bindgen-go` 生成的绑定，
  `make deps && make build` 产出 `plugin.wasm`；`wit-component/` 暂存组件
  world 与 TinyGo 自带的 WASI WIT，`internal/` 为已提交的生成绑定。
- `examples/wasm-python/`：`componentize-py`，`make build` 直接按
  `wit/plugin.wit` 世界把 `plugin/` 包打包为 `plugin.wasm`。

两个示例都声明 `tool_types = ["echo"]`，与 Rust 示例一样返回
`{"result":{"echo":true}}`，可通过宿主的 Wasm 加载路径直接运行。

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
allowed_dirs = ["./data"]    # 需同时声明 filesystem 权限，只读
allowed_write_dirs = []       # 需同时声明 filesystem 权限；列出的目录可读写（读写蕴含读）
allowed_env_prefixes = ["MYAPP_"]  # 需同时声明 environment 权限
allow_network = false   # 缺省 false；置 true 会在加载期直接失败（本阶段无 socket 授权）
store_pool_size = 0     # 缺省 0（关闭）；>0 时启用实例池：Core 路径还要求 guest 导出 wf_heap_reset
```

权限声明沿用 `permissions` 数组。注意：授权列表没有对应权限时
不授予任何能力；`shell` 权限对 wasm 恒为拒绝，需要执行命令的
插件应通过宿主工具贡献间接调用。

---

## 五、调试指南

| 现象 | 含义 | 排查 |
|---|---|---|
| `does not export 'memory'/'alloc'` | 导出缺失 | 检查 `crate-type` 与 `#[no_mangle]`，确认链接后符号存在（`wasm-tools print` 或 `wasm2js` 查看） |
| `hook 'wf_on_load' failed with code 7: bad config` | 钩子返回非 0 | code 为 guest 返回码；冒号后为 `wf_last_error` 明细（无该导出时仅 code） |
| `uses incompatible abi version 99, host expects 1` | `wf_abi_version` 与宿主不一致 | 升级 guest/宿主使版本一致；删除该导出则按 v1 处理 |
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

- 中间件支持布尔与信封两种回答：guest 返回 JSON 布尔值表示沿用传入 context
  继续/截断，或返回 `{"proceed": bool, "context": <replacement>}` 改写下游
  context（两键可选，缺省为 `true`/传入 context）；非法输出按继续处理。
  guest 无法反向回调宿主的 `next` 链之外的双向调用。
- LLM wire 协议通过 `llm_providers` 声明暴露（逐项对应 `llm-codec` 分发）。
- 网络能力恒关闭；`allow_network = true` 会在加载期直接失败（需移除或置 false）；
  `shell` 权限对 wasm 恒为拒绝。
- guest `stdout` 输出接入宿主日志（`tracing::info`），`stderr` 接入警告日志
  （`tracing::warn`）；单流捕获上限 64KiB，超出会 trap，
  每次调用最多转发 4KiB / 20 行，超出部分截断并计数。
- 会话复用：`store_pool_size > 0` 时启用实例池（缺省关闭），成功调用的
  会话会被复用，失败/trap 的会话直接丢弃。Core 路径还要求 guest 导出
  `wf_heap_reset`（加载时探测一次，不支持则打日志关闭池化）；
  Component 路径无复位契约要求，但 guest 须容忍实例复用
  （全局变量在池化调用间保持，处理器应写成无状态）。
- 贡献刷新：宿主在加载时调用一次 `register` 并缓存结果；配置变更成功后
  会重调 `register`，声明变化时自动取消旧贡献并重新注册
  （引擎 `refresh_plugin_contributions` 也可显式触发）。
- 数据编码统一为 JSON（双路径一致）：这是有意的设计——Go/Python/Rust
  guest 无需共享绑定即可实现契约；分发载荷多为小控制文档，编解码开销
  相对 guest 调用本身可忽略。Component 路径暂不启用 WIT 结构化载荷，
  启用将是破坏性契约升级，需另行版本协商。
- Guest-to-Host 回调目前仅有日志（Core 的 `wf_host::log`、
  Component 的 `wf:plugin/host-log`）。tool/LLM/event 调用暂不开放：
  需先定跨后端（Lua/Native 对齐）设计、重入策略与权限模型，
  wasm 单开接口会分裂插件模型，故 deferred 到统一设计。
- 编译缓存为进程内有界缓存（按模块字节 blake3 去重，最多保留 32 个产物），
  宿主重启后失效。
