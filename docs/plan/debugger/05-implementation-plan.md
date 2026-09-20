# 落地方案与待拍板开放点

本文件给出 `debugger` 作为 app 层独立编译目标的具体落地步骤、依赖清单、阶段划分与待确认事项。

## 一、Crate 落地结构

目录与 `wf-headless` 对齐（`crates/app/cli/wf-headless/Cargo.toml` 的 `[[bin]]` 写法），但置于 app 层独立子目录：

```
crates/app/debugger/
├── Cargo.toml      # 包名 wf-debugger；[[bin]] name = "wf-debug"；path = "src/main.rs"
├── src/
│   ├── lib.rs      # pub mod model / replay / observe / hook_dbg / trigger_dbg / format / assert / cli
│   ├── model.rs
│   ├── replay.rs
│   ├── observe.rs
│   ├── hook_dbg.rs
│   ├── trigger_dbg.rs
│   ├── format.rs
│   ├── assert.rs
│   └── cli.rs
└── examples/
    └── sample_trace.rs   # 内置 mock 交互历史样例
```

遵守 AGENTS.md：每个 crate 的 `lib.rs` 直接声明 `pub mod`，扁平文件、禁止 `mod.rs`。

### 1.1 Cargo.toml 关键项（草拟，非完整实现）

- `[package]`：`name = "wf-debugger"`，`version/edition` 取 workspace 值；
- `[lib]`：默认存在（`src/lib.rs`），承载可单测的核心逻辑；
- `[[bin]]`：`name = "wf-debug"`，`path = "src/main.rs"`，仅做 CLI 装配，逻辑全部走 `lib`；
- `[dependencies]`：仅 `wf-workflow` / `wf-agent` / `wf-execution-shared` / `wf-types` / `wf-core` / `wf-common`，以及 `serde`/`serde_json`/`clap`/`anyhow`/`similar`（均已在 workspace 声明）；
- **不**引入 `wf-runtime` / `wf-server` / `wf-api` / `wf-llm`（避免重基础设施）；
- 可选 feature：如 `branch-exhaustion`（默认开）、`json-output`（默认开）。

### 1.2 工作区注册

根 `Cargo.toml` 的 `members` 数组追加 `"crates/app/debugger"`（参照现有 `crates/app/cli/wf-headless` 的条目位置）。

### 1.3 DAG 校验

落地后用 `cargo tree -p wf-debugger` 确认其依赖图只向下（engine/foundation），且没有任何生产 crate 反向依赖 `wf-debugger`，满足 AGENTS.md 的严格 DAG 约束。

## 二、依赖清单（仅向下）

| 依赖 | 用途 |
|------|------|
| `wf-workflow` | 图遍历、路由、变量、消息上下文、hook 发射器、trigger 子系统 |
| `wf-agent` | `AgentLoopEntity`、迭代模型、agent hook、`TriggeredAgentExecutionManager` |
| `wf-execution-shared` | `fire`/`FireSummary`、条件求值、共享类型 |
| `wf-types` | `StaticNodeType`/`Edge`/`Condition`/`Hook*`/`Trigger*`/`Message`/`BaseEvent` |
| `wf-core` | `condition::ConditionEvaluator`、`EventBus`、`now` |
| `wf-common` | ID 生成 |
| `serde`/`serde_json`/`clap`/`anyhow`/`similar` | 解析、CLI、错误、diff（均已在 workspace） |

## 三、实施阶段

> 阶段命名用「阶段一/二/…」以避免与代码注释标识符混淆（AGENTS.md 禁止在代码注释中使用 P1/§4.1/phase3 等结构标识符）。

- **阶段一 · 骨架**：建 crate、注册工作区、`lib.rs` 模块声明、CLI 空壳、一个 `examples/sample_trace.rs` 内置 mock 数据。验收：`cargo build -p wf-debugger` 通过、`wf-debug --help` 可用。
- **阶段二 · 模型与回放**：`model.rs`（trace/断言/diff 结构）、`replay.rs`（导入式回放 + 静态综合回放骨架）。验收：能读入 mock trace 并逐步行进。
- **阶段三 · 观察与格式化**：`observe.rs`（变量/消息 diff 提取）、`format.rs`（文本 + JSON）。验收：逐步输出含 input/result/变量 diff/消息 diff；JSON 形态可 `jq` 解析。
- **阶段四 · 断言**：`assert.rs`，含「出错给实际值」契约与非零退出码。验收：内置样例含一条故意失败的断言，文本/JSON 均给出期望 vs 实际。
- **阶段五 · Hook 调试**：`hook_dbg.rs` 复用 `fire()`。验收：对 mock `HookDefinition`/`HookContext` 输出命中/条件/payload/handler 结果/veto 原因。
- **阶段六 · Trigger 调试**：`trigger_dbg.rs` 复用 `matcher/arbiter/governor`。验收：对 mock `BaseEvent` 输出候选/落选/仲裁/许可/状态记录。
- **阶段七 · 分支穷尽（可选）**：在静态综合回放中对 `Route`/`Edge`/`Fork`/`Loop` 做分支枚举 + 死边检测。验收：样例含一个 `Route` 节点，穷尽模式列出全部候选分支与命中情况。
- **阶段八 · 打磨**：时间线视图、覆盖率小结、`--no-color`/CI 友好；补充单元/集成测试。

## 四、关键复用点（避免重写）

| 目标 | 复用 |
|------|------|
| 路由语义 | `RouteHandler::execute_inner`（`crates/engine/wf-workflow/src/handler/route.rs:43`） |
| 变量求值 | `evaluate_expression` / `VariableResolver`（`variable.rs:631/544`） |
| 条件表达式 | `wf_core::condition::ConditionEvaluator`（`route.rs:5`） |
| hook 触发 | `fire()`（`crates/engine/wf-execution-shared/src/hooks/fire.rs:85`） |
| trigger 链路 | `matcher/arbiter/governor`（`matcher.rs:23`/`arbiter.rs:28`/`governor.rs:49`） |
| 图分析 | `analyze_graph`/`detect_cycles`/`topological_sort`（`crates/engine/wf-workflow/src/lib.rs:32`） |

## 五、待拍板开放点（Open Questions）

1. **crate / 二进制命名**：用户称模块为 `debugger`；按 app 层 `wf-` 前缀约定，建议包名 `wf-debugger`、二进制 `wf-debug`、目录 `crates/app/debugger`。是否接受，或坚持裸名 `debugger`？
2. **数据来源优先级**：初版以「导入式回放（mock trace JSON）」为主，还是「静态综合（纯 graph 定义）」为主？二者是否都需在一期交付？
3. **mock 交互历史的获取渠道**：是否提供从真实运行的 checkpoint / `NodeExecutionRecord` 历史（`state.rs:9`）导出 trace 的工具，还是仅手写/测试构造？
4. **分支穷尽的范围**：一期仅枚举 `Route` 节点的 `conditions` + `default`，还是一并覆盖 `Conditional` 边、`Fork` 并行分支、`Loop` 迭代边界（见 `02` 4.1）？符号化枚举（多变量组合）是否纳入一期？
5. **是否需要真实驱动**：是否要支持「阶段三实时式回放」（真正调用 `fire()`/`matcher` 但用 mock 副作用）而不仅仅是「导入 trace」？前者更贴近真实函数行为但实现更重。
6. **与 `wf-cli-demo` / `tui-debug` 的关系**：debugger 是否需要在 TUI 内提供交互式调试视图，还是仅 CLI + JSON 输出即可？
7. **断言 schema 稳定性**：`assertions` 是否进入仓库内共享的 mock 数据约定（供 `wf-cli-demo` 等其他 crate 复用），还是仅 debugger 内部格式？
8. **独立编译目标的形态**：确认为「独立 crate + `[[bin]]`」而非「现有 crate 下的 `examples/` / feature」，以严格隔离、不膨胀主构建（与 AGENTS.md 的 DAG 约束一致）。

## 六、风险与缓解

- **引擎接口变动**：debugger 依赖 `fire()`、`matcher` 等仍是 `pub` 且签名稳定；若未来改为私有，需在 debugger 之前先固化这些接口的对外契约。缓解：阶段一先 `cargo build` 验证所有计划依赖项确实可达。
- **变量快照成本**：每步 clone 整个 `VariableStore` 可能昂贵；缓解：仅对「发生变化的 key」做浅拷贝 + 指针共享未变值（DashMap 值已是 `Arc<Value>` 语义外的 `Value`，但 JSON 值克隆成本低，首批以全量克隆为主，后续按需优化）。
- **DAG 污染**：若不慎引入 `wf-runtime` 会拉入重基础设施；缓解：阶段一即用 `cargo tree` 卡点，CI 中加入「debugger 不得依赖 runtime/server/api」检查。
