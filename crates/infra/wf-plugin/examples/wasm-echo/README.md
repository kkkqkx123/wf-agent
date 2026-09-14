# wasm-echo 示例插件

最小可运行的 wasm 插件，对应宿主契约见 `wf-plugin-sdk/src/wasm.rs`。

## 构建

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

产物为 `target/wasm32-unknown-unknown/release/wasm_echo.wasm`（约 0.5KB）。

## 安装试运行

```sh
mkdir -p plugins/wasm-echo
cp target/wasm32-unknown-unknown/release/wasm_echo.wasm plugins/wasm-echo/
cp plugin.toml plugins/wasm-echo/
```

该插件声明一个名为 `echo` 的工具，每次调用返回固定 JSON
`{"result":{"echo":true}}`。生命周期钩子全部返回成功。

## 实现要点

- `no_std` + `cdylib`，无 WASI 导入，宿主侧无需授予任何能力即可运行。
- `memory` 由 Rust 编译器自动导出，`alloc` 为 bump 分配器，
  `dealloc` 为空实现（宿主每次调用后丢弃整个 store，泄漏有界）。
- 返回值统一为打包后的 `(ptr, len)` i64，高 32 位为长度，低 32 位为指针。
- panic 直接 trap，宿主将其转为插件错误上报。
