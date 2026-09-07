# wf-cli mini 模式去留决策分析

> 背景：`docs/ref/wf-cli-mini-对比与改进建议.md` 指出 mini 输入/输出无法隔离，根因是"双渲染路径拼合"（输出直写终端 scrollback + 输入 inline viewport 重绘）。上一轮的改进建议 A（全屏单树）在架构上对齐 opencode，但**消灭了 mini 的性能优势**；用户据此提出：若全屏化无法减少开销，直接移除 mini 可能更合适。本文档核实事实并给出决策建议。

---

## 一、结论先行

**建议移除 mini 模式。** 理由可归为三条事实：

1. **mini 的核心渲染栈与完整 TUI 的 Session 屏完全共享**——移除 mini 不损失任何既有投资；
2. **完整 TUI 已经具备 mini 想提供的交互能力**（会话输入/输出、面板、审批、提问、replay），且多屏切换时输出随屏切换是完整 TUI 的既有能力；
3. **隔离问题的唯一根治方案（全屏单树）恰好消灭 mini 的存在理由（性能）**——"轻量"与"正确隔离"在 mini 的 inline 模型下不可兼得，这是结构性矛盾，不是实现瑕疵。

---

## 二、事实核查

### 2.1 入口与分发（mini 与完整 TUI 是并行的两套应用）

```
main.rs → lib.rs run() → ModeResolver::resolve() (mode.rs)
                        → run_interactive() (lib.rs L252-281)
                            ├─ CliMode::Mini → MiniApp::new(opts)?.run()
                            └─ CliMode::Tui  → TuiApp::new(adapter).run()
```

- `--tui` / `--mini` 互斥（`args.rs` L82）；`--prompt/-p` 仅限 mini（L93）。
- TTY 默认形态由 `WF_CLI_MODE` 决定，默认 `"mini"`（`mode.rs` L125）——**当前 TTY 默认进的是 mini**。
- `CliMode::Mini` 与 `CliMode::Tui` 各自 bootstrap 一个 `DomainAdapter`、各自一套 `run()`，是**两个独立的交互应用**。

### 2.2 模块共享度（关键事实）

| 模块 | 行数 | 使用方 |
| :--- | :--- | :--- |
| `footer.rs` | 755 | **mini.rs + session.rs（完整 TUI）** |
| `composer.rs` | 478 | footer.rs（→ 两者） |
| `scrollback.rs` | 413 | **mini.rs + session.rs** |
| `reducer.rs` | 724 | **mini.rs + session.rs** |
| `markdown.rs` | 813 | **mini.rs + session.rs + render.rs** |
| `approval.rs` | 335 | **mini.rs + session.rs** |
| `question.rs` | 461 | **mini.rs + session.rs** |
| `panels.rs` | 767 | footer.rs + mini.rs |
| `queue.rs` | 140 | mini.rs + panels.rs |
| `replay.rs` | 573 | **mini.rs + session.rs** |
| `turn.rs` | — | run.rs + remote.rs + mini.rs + session.rs |
| `sink.rs` | 208 | **仅 mini.rs**（MiniSink/MiniOutputEvent） |
| `splash.rs` | 24 | **仅 mini.rs** |
| `mini.rs` | 2386 | MiniApp 本体 |
| `tui.rs` / `screens.rs` / `session.rs` | 1131/583/1075 | 完整 TUI（TuiApp / 8 屏 / SessionController） |

**结论**：mini 专属代码 ≈ `mini.rs`(2386) + `sink.rs`(208) + `splash.rs`(24) ≈ **2600 行**；会话渲染的全部核心资产（footer/composer/scrollback/reducer/markdown/approval/question/panels/replay/turn）**都被完整 TUI 的 Session 屏复用**。移除 mini 不删除任何共享渲染投资。

### 2.3 完整 TUI 现状（不是规划，是已实现）

- `TuiApp`（tui.rs）：alt-screen 全屏、8 屏导航（`screens.rs` `ScreenKind`：Dashboard/Workflow/Executions/Session/Checkpoints/Search/Settings/Help）、导航栈 push/pop、数据按屏缓存（TTL）、modal 栈、主题热更新、双按 Ctrl-C 退出。
- **输出随屏切换是既有能力**：`tui.rs` `draw()`（L427-454）按 `current_kind()` 分发——`Session` 屏走 `session.draw()`（复用 footer/composer/scrollback），其余屏走 `screens.draw()`。用户提出的"切换界面时输出也需要切换"，完整 TUI 天然满足。
- `SessionController`（session.rs）：与 mini 相同的 reducer/markdown 流式管线、footer 视图路由（Permission/Question/Prompt）、composer 输入、replay 加载。**mini 的交互能力在完整 TUI 中全部存在**。

### 2.4 性能论点核实（用户的前提成立）

| 模型 | 每帧渲染范围 | 输出路径 | 性能特征 |
| :--- | :--- | :--- | :--- |
| mini 现状（inline） | 仅底部 N 行（footer） | `insert_before` 直写终端 scrollback，**不重绘** | 渲染成本低，但隔离无解 |
| 方案 A（全屏单树） | 全屏 buffer + 布局 | 输出进输出区，随帧重绘 | 渲染成本 ≈ 完整 TUI |
| 完整 TUI | 全屏 buffer + 布局 | 同上 | 同上 |

- ratatui 全屏虽按 cell diff 只输出变化格，但**布局与渲染计算是全屏量级**（每帧遍历全部可见行），与"只画底部 4 行"的 inline 存在量级差。
- 因此：**mini 改成全屏单树后，其性能与完整 TUI 无实质差别，"减少性能开销"的初衷消失**——用户推理成立。
- 反向验证：若保留 inline 形态（方案 B），流式 tail 进 footer、scrollback 批量落定只能缓解混排，**终端整屏滚动/视口被顶走的机制性缺陷仍在**（见对比文档根因 2/4），且 reflow 补丁复杂度继续累积。

---

## 三、移除 vs 保留权衡

| 维度 | 保留 mini（方案 B 修补） | 移除 mini |
| :--- | :--- | :--- |
| 隔离问题 | 结构性缺陷，只能缓解 | 消灭问题源（不再有第二套渲染路径） |
| 性能诉求 | 保住 inline 低开销 | 放弃"轻量"定位，换取架构正确 |
| 交互能力 | 与完整 TUI Session 屏重复 | 完整 TUI 已全覆盖（含输出随屏切换） |
| 维护成本 | 两套事件循环/两套 run 并存 | 收敛为单一交互应用 |
| 代码删除 | 无 | mini.rs/sink.rs/splash.rs ≈ 2600 行 + 分发分支 |
| 兼容性 | — | 项目处于开发期，**明确不要求向后兼容**（AGENTS.md） |
| 退出后回看 | inline 保留 scrollback | 全屏 alt-screen 退出即清屏；可用退出时打印 scrollback 补偿 |

### 3.1 保留 mini 的仅存理由与回应

1. "轻量快速启动" → 完整 TUI 启动后默认落在 Session 屏（导航到 Session 即可），启动路径没有本质差异；
2. "退出后对话留在终端可回看" → 这是 inline 的独有体验，但**正是隔离问题的来源**；可在完整 TUI 退出时打印会话 scrollback 到 stdout 补偿（`replay` 模块已有加载能力）；
3. "性能敏感终端"（慢速串口等）→ 属于边缘场景，不值得为它保留一套结构性缺陷的渲染模型。

### 3.2 移除的影响面（供实施）

| 位置 | 内容 |
| :--- | :--- |
| `src/mini.rs` | 删除 MiniApp（2386 行） |
| `src/sink.rs` / `src/splash.rs` | 删除（mini 专属） |
| `src/lib.rs` | 删除 `pub mod mini/sink/splash` 及导出；`run_interactive` 删除 Mini 分支；`CliMode::Mini` 移除 |
| `src/mode.rs` | `CliMode` 去掉 Mini；`WF_CLI_MODE` 默认值改为 `tui` 或移除 |
| `src/args.rs` | 删除 `--mini`；`--prompt/-p` 语义改为完整 TUI 初始 prompt（或一并移除）；校验同步 |
| `examples/mini_demo.rs` / `mini_panels.rs` / `mini_render.rs` | 删除或迁移到完整 TUI 示例 |
| `tests/mini_pipeline.rs` | 删除或迁移为 Session 屏测试 |
| `docs/cli/05-opencode-mini模式与无头模式设计.md` | 更新：mini 形态取消，交互形态收敛为"headless + 完整 TUI" |

---

## 四、决策建议

**同意移除 mini，交互形态收敛为两档：**

1. **Headless**（`wf run` / stdin 管道 / `--no-tui`）——现有，不动；
2. **完整 TUI**（`--tui`，TTY 默认）——现有，作为唯一交互形态；默认落地 Session 屏，补齐 mini 曾承担的"轻量会话"体验；退出时打印 scrollback 作为 inline 回看补偿。

实施顺序建议：先确认完整 TUI Session 屏对 mini 共享栈的覆盖完整（冒烟），再删除 mini 专属代码与分发分支，最后更新参数/文档/示例。此方案比"修补 inline"更彻底地解决隔离问题，且符合项目"无向后兼容负担、重架构正确性"的开发准则。
