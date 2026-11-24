# compaction-engine-demo

一个最小的第三方压缩引擎示例，演示如何通过 `CompactionEngine` 契约完整替换 `builtin`。

## 行为

- 引擎名：`demo-view-only`（配置 `compaction.engine = "demo-view-only"`）。
- 自动入口 `compact_if_needed` 永远返回 `None`（不自行触发压缩）。
- 手动入口 `compact_now` 返回一个 legacy 单摘要 `ViewPlan`；空快照返回 `Noop`。
- 能力声明：`destructive_replace = false`。若返回 `Replace`，宿主 ShrinkProof 闸门会以「未声明能力」拒绝——演示能力边界。
- `ProgressSink` 上报 `plan` / `done` 阶段，可被 `TracingProgressSink` 或 TUI/Web 消费。

## 接入

宿主启动时调用 `compaction_engine_demo::register()` 注册进程级工厂，随后配置 `compaction.engine` 即可让所有执行实例解析到该引擎。关闭时调用 `unregister()`。

## 验证

```bash
cd crates/infra/wf-plugin/examples/compaction-engine-demo
cargo test
```

单测覆盖：工厂注册 → 构建 → 重名拒绝 → 注销，证明第三方引擎可替换 `builtin`。
