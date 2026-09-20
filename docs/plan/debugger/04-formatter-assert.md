# 格式化输出与断言

本文件覆盖用户需求中的「展示同时提供格式化输出和断言功能」与「出错时给出实际值」。

## 一、格式化输出（format.rs）

debugger 支持两种输出形态，通过 CLI 子命令/flag 切换：

### 1.1 文本形态（默认，面向人读）

- **逐步流（step stream）**：按执行顺序逐「步」打印，每步含：
  - 标题行：`[step N] <node_type> <node_name> (<node_id>)` + 耗时；
  - 输出块：`input` / `result`（payload-capped，超长折叠）；
  - 变量 diff 块：新增/删除/修改三类，修改项内嵌 `similar` 文本 diff；
  - 消息 diff 块：新增/改写/删除消息（按 `message.id` 对齐）；
  - 出错块（仅当失败）：醒目的「期望 vs 实际」对比，附实际 `error` / 实际 `result`。
- **路由/分支块**：在 `Route`/`Fork`/`Loop` 决策点打印候选分支与其命中情况（见 `02` 的分支穷尽）。
- **hook 块 / trigger 块**：见 `03` 的展示表。
- 颜色与折叠可关（`--no-color`、`--full`），便于 CI 日志对比。

### 1.2 JSON 形态（面向机读 / 断言 / 集成）

- 直接序列化内部 trace + 观察结果 + 断言结果为一个稳定 schema 的 JSON：
  - `steps[]`：每步的 input/result/diffs/hooks/triggers；
  - `diffs`：`variable_diffs` / `message_diffs` 结构化数组；
  - `assertions[]`：每项期望值、实际值、pass/fail；
  - `summary`：步数、失败数、未覆盖分支数。
- JSON 形态不折叠、不染色，方便 `jq` 提取与回归比对。

### 1.3 复用与一致性

- 变量/消息 diff 文本由工作区既有 `similar = "3"` 生成（`Cargo.toml`）；
- 时间用 `wf_common::now()` 口径（`crates/engine/wf-workflow/src/state.rs:90` 用法），与引擎一致。

## 二、断言引擎（assert.rs）

### 2.1 断言来源

断言可写在 mock 交互历史里（`Trace.assertions`，见 `01` 的格式草稿），也可由 CLI 以文件/内联方式提供。两类：

1. **每步断言**：针对某 `step` 的 `result` / 某变量 / 某消息期望值；
2. **路由断言**：针对某决策点应选中的 `target_node_id`（分支穷尽模式下尤其有用）；
3. **hook/trigger 断言**：期望某触发点命中/落选、某 handler 的 `outcome`、某 trigger 是否 `Granted`。

### 2.2 断言执行与「实际值」

- 对每个断言：取期望值 `E` 与实际值 `A`（来自回放后的 trace）；
- 比较采用与引擎一致的语义：`evaluate_expression` 求值（`crates/engine/wf-workflow/src/variable.rs:631`）、`VariableResolver` 插值（同文件 `:544`）、JSON 值相等；
- 失败时**不抛异常中断**，而是记录 `AssertionResult { name, expected, actual, pass:false, message }` 并继续；
- 输出永远同时给出 `expected` 与 `actual`（即用户要的「出错时给出实际值」）；
- 退出码非零当有任一断言失败（便于 CI 门槛）。

### 2.3 断言与格式化联动

- 文本形态：失败断言在对应步内就地高亮「期望 vs 实际」；
- JSON 形态：`assertions[]` 字段随 trace 一同输出，`pass=false` 携带 `actual`；
- 两者共用同一份 `AssertOutcome`，避免双份实现。

## 三、出错处理的统一契约

| 场景 | 实际值来源 | 展示要点 |
|------|------------|----------|
| 节点步失败 | `NodeExecutionRecord.error` / `result`（`state.rs:9`） | 标红 `error` 文本 + 实际 `result` |
| 变量不符 | `VariableStore` 快照（`variable.rs:5`） | 期望 vs 实际值并排 + `similar` diff |
| 消息缺失/错误 | `__msg_ctx__` 上下文（`message_context.rs:18`） | 期望消息 vs 实际 content |
| hook 否决 | `FireSummary.outcome`（`fire.rs:37`） | 标红 `Veto.reason` |
| trigger 落选 | `matcher/arbiter/governor` 判别（`matcher.rs:23`/`arbiter.rs:28`/`governor.rs:49`） | 逐模板落选原因 + 实际 `FirePermit` |
| 分支未覆盖 | 分支穷尽枚举结果 | 死边/未覆盖边告警 |

## 四、展示/断言之外的辅助

- **时间线视图**：将 node 步、hook 火点、trigger 事件按 `start_time` / 事件时间戳归并到一条时间线，直观看到「hook 触发 → 审计事件 → trigger 匹配」的因果；
- **覆盖率小结**：基于分支穷尽结果给出边/分支覆盖百分比；
- 以上均基于 `01`~`03` 已经收集到的数据结构，不引入新依赖。
