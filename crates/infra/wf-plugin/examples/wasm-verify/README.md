# wasm-verify 端到端验证工具

通过真实的宿主插件加载路径（`load_wasm_plugin_with_base` → 生命周期 →
贡献注册 → `echo` 工具分发）验证各语言的 component-model wasm 示例。

它是独立 crate（`[workspace]` 与主工作区隔离），只消费同目录下的 guest
示例作为 fixture，不会被主工作区构建带进来。

## 构建

```sh
cargo build --offline
```

## 运行

先用各示例自己的构建方式产出 `plugin.wasm`，再把示例目录传给本工具：

```sh
# Rust 示例
cargo run --offline -- ../wasm-echo

# Go 示例（需先 `make build`）
cargo run --offline -- ../wasm-go

# Python 示例（需先 `make build`）
cargo run --offline -- ../wasm-python
```

也可以一次性验证全部：

```sh
cargo run --offline -- ../wasm-echo ../wasm-go ../wasm-python
```

每个 guest 都会打印注册的工具、`echo` 调用结果和 `== OK <id>`；任一 guest
失败则打印 `!! FAIL` 并以非零码退出。

## 说明

- guest 的 `.wasm` 是构建产物，不随仓库提交，务必先构建。
- 该工具是手动验证入口；确定性回归由 `wf-plugin` 的单元测试覆盖
  （见 `src/wasm/component.rs` 中的 reactor 初始化测试）。
