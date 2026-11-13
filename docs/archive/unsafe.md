# Unsafe 使用台账

> 仓库规范：除底层操作外禁用 `unsafe`，所有 `unsafe` 必须在此登记。

## 一、登记项

### U1 `wf-plugin-sdk` 原生插件 FFI 边界（`crates/foundation/wf-plugin-sdk/src/plugin.rs`）

| 位置 | 形式 | 说明 |
|---|---|---|
| `PluginState` 的 `unsafe impl Sync` | `unsafe impl` | 插件单例经 `Mutex` 保护，宿主串行驱动钩子，跨线程共享安全 |
| `context_config` | `unsafe fn` | 解引用宿主传入的 `PluginContextC` 指针与 C 字符串，调用方需保证指针有效 |
| `__private::run_config_hook` | `pub unsafe fn` | 同上，经宏生成的 `extern "C"` 导出调用，宿主保证 ABI 入参有效 |
| `__private::forward_registrations` | `pub unsafe fn` | 解引用宿主传入的注册器指针并调用其回调，宿主保证有效 |
| `__private::write_manifest_bytes` | `pub unsafe fn` | 向宿主缓冲区写清单字节，调用方保证 `out`/`len` 指向有效可写内存 |
| `__private::dispatch_into_buffer` | `pub unsafe fn` | 读写宿主传入的输入字符串与输出缓冲，调用方保证有效 |
| `export_plugin!` 展开的 5 处调用 | `unsafe` 块 | 上述四函数的唯一调用点，位于宏生成的 `extern "C"` 导出内，入参即宿主 ABI 入参 |

调用约定：所有 `unsafe fn` 的安全前置均为宿主 ABI 入参有效性，由宿主侧加载器保证；插件作者只经 `export_plugin!` 间接使用，不直接接触裸指针。

### U2 测试内的 `unsafe` 调用（同文件 `#[cfg(test)]`）

`forward_registrations_requires_host` 用空指针调用对应 `unsafe fn`，预期返回错误码而非崩溃。该用例验证空指针守卫，不解引用空指针。

### U3 sidecar 优雅停机信号（`crates/infra/wf-integration/src/transport/sidecar.rs`）

| 位置 | 形式 | 说明 |
|---|---|---|
| `send_termination`（仅 Unix） | `unsafe` 块调用 `libc::kill` | 入参只有子进程 pid 与 SIGTERM 信号，pid 来自存活的 `tokio::process::Child` 句柄；不解引用任何指针、不转移所有权、不涉及内存读写。非 Unix 平台退化为 `start_kill`，无 `unsafe` |

调用约定：`libc::kill` 只做信号投递，安全前提是 pid 指向本进程创建的存活子进程，由 `RunningSidecar` 与监督任务的生命周期保证。

## 二、已排除项

宿主注册器回调字段为普通（非 `unsafe`）`extern "C" fn` 指针，调用无需 `unsafe` 块；此前误加的包裹已删除。测试中对宏生成的普通 `extern "C"` 导出的调用同样无需 `unsafe` 包裹，已清理。
