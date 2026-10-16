# `wf-cli-demo/examples/tui` Demo 规范

本目录是 `tui` 相关演示与人工渲染审查的统一入口。
所有 example 共享一个约定：**合成数据、无外部网络依赖、必须可退出、必须注册在 Cargo `[[example]]` 并在本文列出。**

## 1. 命名约定

- 前缀固定为 `tui_`。
- 集成 smoke test 使用 `tui_<surface>_showcase`，例如：
  - `tui_management_showcase`：所有 management surface integrated smoke test。
- 工具/诊断使用 `tui_<tool>`，例如 `tui_diff_record`、`tui_key_receiver`、`tui_bench`。
- 主题/组件/渲染专题分别使用：
  - `tui_style_gallery`
  - `tui_components_gallery`
  - `tui_markdown_showcase`
- 避免使用泛化 `screen` 表达单个 feature；"`screen`" 只应描述 `ScreenKind` / `Screens` stack 的集成导航本身。

## 2. 何时新建 Example

仅当下列条件满足 **至少 2 项** 且 manual check 无法用集成 demo 稳定复现时，才新增 example：

- 有单独的交互模式（tab/filter/streaming/diff/command completion）；
- 需要同步 data 与 state 的时序；
- 可以用合成数据低成本构造；
- 风险/审查收益明显。

不推荐：
- 为每个 `ScreenKind` 单独写 example；
- 为每个 crate 为每个 module 单独开 demo；
- example 里直接接真实 server；
- 未在 `Cargo.toml` 或本 README registration 的 example。

组件/Style/markdown 演示优先扩展现有 gallery，而不是新建 guard demo。

## 3. Directory inventory

| Example | File | 목적 | 运行 |
|---|---|---|---|
| `tui_management_showcase` | `management_showcase.rs` | 管理 surface integrated smoke | `cargo run -p wf-cli-demo --example tui_management_showcase` |
| `tui_style_gallery` | `style_gallery.rs` | 主题、动画、motion | `cargo run -p wf-cli-demo --example tui_style_gallery` |
| `tui_components_gallery` | `components_gallery.rs` | modal/overlay/widget | `cargo run -p wf-cli-demo --example tui_components_gallery` |
| `tui_markdown_showcase` | `markdown_showcase.rs` | markdown / streaming | `cargo run -p wf-cli-demo --example tui_markdown_showcase` |
| `tui_animation` | `animation.rs` | animation/token stream | `cargo run -p wf-cli-demo --example tui_animation` |
| `tui_diff_record` | `diff_record.rs` | diff recorder | `cargo run -p wf-cli-demo --example tui_diff_record` |
| `tui_key_receiver` | `key_receiver.rs` | key resolution | `cargo run -p wf-cli-demo --example tui_key_receiver` |
| `tui_bench` | `bench.rs` | perf probe | `cargo run -p wf-cli-demo --example tui_bench` |
| `tui_scenario` | `scenario.rs` | scenario record/replay | `cargo run -p wf-cli-demo --example tui_scenario` |
| `tui_component_output` | `component_output.rs` | golden output | `cargo run -p wf-cli-demo --example tui_component_output` |

## 4. Implementation checklist for new examples

1. Add `[[example]]` in `Cargo.toml` (name + path).
2. Header doc explains:
   - what surface it demonstrates;
   - synthetic data boundary;
   - interaction keys;
   - run command.
3. Ensure raw mode / alternate screen restore reliably in `Result` cleanup (use cleanup on exit).
4. Avoid `unwrap()` in runtime paths; use `expect()` only when explicit constraint failure.
5. If adding golden-output integration, cite deterministic environment assumptions in comments.
6. Update this README inventory and planning docs.
7. Run:
   - `cargo build -p wf-cli-demo --example <name>`
   - `cargo clippy -p wf-cli-demo --all-targets`
   - `cargo fmt --check`
