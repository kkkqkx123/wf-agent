# wf-cli Stage 7 剩余问题 — 运行时冒烟收尾：分析与修改方案

> 文档属性：剩余问题分析 + 修改方案（issue 归档）
> 分析基准：2026-09-06 `wf-agent` HEAD（`db77622 update tui`），真实 PTY 冒烟实测（tmux，`wf --tui --storage memory`）
> 上游：`docs/plan/cli/wf-cli-stage7-完整模式-现状复核与细化修改方案.md`（§8.4 运行时未覆盖项）、本会话 R1 验收报告
> 关联：`docs/plan/cli/wf-cli-剩余问题-TUI深化.md`、`docs/analysis/codex-tui.md`、`wf-agent-learnings.md`

---

## 一、背景与范围

`wf-cli` 完整模式（全屏 TUI）的 E1–E4 与 E6 主体已落地并通过 `cargo check / test`。R1 运行时冒烟（对应复核文档 §8.4 五项）在真实 PTY 下执行后，**2 项完整通过（SIGTSTP 挂起恢复、resize 防抖/reflow），2 项机制级通过（SIGUSR2 热更新链路、会话屏实时输入渲染），1 项受阻（长会话 replay 缺数据源）**，并实测发现 1 个真实用户可感知的行为缺口（输入型 Ctrl-Z 不触发挂起）。

本文档把 R1 的受阻项、实测发现与复核文档遗留的开放点统一收敛为 issue 清单，逐项给出根因、修改方案、落点锚点与验收方式；凡涉及跨 crate（wf-runtime、存储层）或需 owner 拍板的事项如实标注"开放"，不代为修改。

---

## 二、剩余问题清单

| 编号 | 分类 | 问题 | 现状证据 / 锚点 | 影响 | 阻塞 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| I1 | 实测发现 | 输入型 Ctrl-Z 不触发挂起，仅外部 SIGTSTP 可走通挂起周期 | raw mode（crossterm cfmakeraw）关闭 ISIG，0x1A 按键被当普通输入消费；PTY 实测 `send-keys C-z` 进程保持 `Sl+` 不变。挂起周期本身可用：`kill -TSTP` → `Tl` → zsh 提示符（`✘ TSTP`）→ `fg` → 完整重绘 | 真实用户按 Ctrl-Z 无反应，与复核文档 §8.4 第 1 项的用户流不符 | 无 |
| I2 | 运行时受阻 | 长会话 replay 冒烟无数据源：`--storage memory` 空库，Executions 无行 → Enter 下钻不可达，`load_replay` 的 Loading→替换只能走错误分支 | `session.rs:237 load_replay`、`tui.rs:405 apply_pending_replay`（仅 Executions Enter 触发）；wf-cli 无"预置持久会话"的工具或 fixture | §8.4 第 4 项（长历史 replay）无法端到端验收 | 需数据源（见方案） |
| I3 | 运行时受阻 | SIGUSR2 热更新在无 OSC 应答环境不可见：probe 未应答 → fallback 缓存主题，两次 USR2 渲染逐字节一致 | `theme.rs:445 probe_theme`（OSC 查询，tmux 不回 → 落 `fallback_theme` `:456`）；无用户可编辑的主题文件路径，cache 仅为 last-known-good 快照（`theme.rs:403 theme_cache_path` 指向 `$XDG_CACHE_HOME/wf-cli/theme.json`） | "主题热更新"对用户不可解释、不可复现 | 无 |
| I4 | 复核文档遗留 | E5 真·cursor 分页补载未做：全量拉取 + 整页替换，滚顶无补更早历史 | `replay.rs:12 replay_scrollack` 全量 `by_agent_loop/by_execution`；`session.rs:70 ReplayPhase{LoadingBeginning, Complete}`，`Partial` 仅为注释预留（`:235-236`） | 超长会话全量拉取的内存/时延成本，滚动查看顶部缺失 | 存储层无分页 API（owner 开放） |
| I5 | 复核文档遗留 | 退出防 failover（`active_shutdown`）未实施，主动退出瞬间可能闪 `Failed` 行 | 全仓无 `active_shutdown`；事件循环末段仅消费通用 `is_shutting_down()`；会话任务靠 run() 清理段 abort | 退出瞬间的错误行/后台重试观感 | wf-runtime owner 未对齐 |
| I6 | 复核文档遗留 | 模态配色不跟随热更新：8 个模态 draw 无 `&Theme` 参数，内部走回退主题 | `modal.rs` Modal trait draw 签名、`tui.rs:430` 仅 `session.draw` 传入 `&self.theme` | SIGUSR2 后 Session 屏与模态配色不一致 | 无（收益低，可后置） |
| I7 | 验收基建 | PTY 冒烟不可重复执行：§8.4 五项均为一次性人工操作，无脚本沉淀 | R1 执行序列（tmux + kill + capture-pane）未固化 | 回归无保障，开发板验收需重造轮子 | 无 |
| I8 | 文档同步 | `docs/plan/cli/wf-cli-剩余问题-TUI深化.md` 仍为"待评审、E1–E6 全未实现"（2026-09-02），与代码事实不符 | 该文档 §1.2 表格仍列 E1–E6 全部缺口；复核文档 §七 要求升级但未执行 | 过期方案误导后续排期 | 无 |

---

## 三、逐项分析与修改方案

### I1 — 输入型 Ctrl-Z 不触发挂起（tui + mini 双形态）

**根因**：raw mode 通过 `cfmakeraw` 关闭 ISIG，Ctrl-Z 按键（0x1A）到达 crossterm 输入流成为普通键事件；应用只在收到**信号** SIGTSTP 时置位 `SUSPEND_PENDING`（`tui.rs:52` / `mini.rs:148`），而 raw 模式下内核不会因按键产生该信号。挂起周期本身（`tui.rs:339 check_suspend`：restore → `signal(SIGTSTP, SIG_DFL)` → `raise` → SIGCONT 后重进 raw/alt → 重查几何 → clear → 全量重绘）实测正确，缺的只是"从按键到达该周期的入口"。

**方案（推荐 A，改动最小）**：
1. 在两形态的输入最顶层识别 `Ctrl+Z` 键事件，直接置位既有挂起标志，复用既有周期。落点：`tui.rs:554 handle_key` 顶部（早于 Session 屏把持键 `:569` 与模态分发）、mini 事件循环的键处理入口（对齐 `mini.rs:718` 一带的 `key.code` 匹配区，注意置于 Composer 插入字符分支 `:719` 之前）；Session 屏流式期间也须生效（挂起属终端级操作，不应被会话把持键吞掉）。
2. 置位动作本身是 async-signal-safe 的原子写，与信号路径共用同一标志，无需新增状态机。
3. `?` Help 弹窗键位说明同步补充 `Ctrl+Z 挂起`（`modal.rs` HelpModal 键列表）。

**为什么能停**：`check_suspend` 内由进程自身 `raise(SIGTSTP)`（SIG_DFL）停止，与终端 ISIG 无关；停止后 shell 作业控制接管（实测 zsh 提示符 `✘ TSTP`），用户 `fg` 即触发 SIGCONT 恢复——真实用户流与复核文档 §8.4 第 1 项完全一致。

**验收**：
- PTY：`tmux send-keys C-z` → 进程进入 `T` 停止态，shell 提示符出现；`fg` → `Sl+`，画面完整重绘；会话屏流式假想态（可用合成事件）下同样生效；
- 回归：Ctrl+C 两按退出语义不变（`Ctrl+Z` 分支置于其前不拦截 c 键）；mini 形态同测；
- `cargo test -p wf-cli` 全绿；`clippy` 无告警。

### I2 — 长会话 replay 冒烟缺数据源

**现状**：`load_replay` 需真实存在的 execution/agent_loop id（`replay.rs:17` 先 `agent_loop_registry::summary` 校验），空库不可达；wf-cli 无预置持久会话的工具或 fixture。

**方案（分层，务实为主）**：
1. **短期（推荐，落成 fixture）**：在真实环境（开发板）跑一次长会话（>200 条消息），将产物 sqlite 数据文件固化为 `tests/fixtures/replay-long.sqlite`（或示例生成的固定库），冒烟脚本与集成测试复制到临时目录后以 `--storage sqlite:<path>` 启动，走 Executions → Enter 下钻，断言"首帧可见 Loading 占位 → 内容整体替换 → UI 不阻塞"。此路不伪造数据、不改生产代码，产物可入库复查。
2. **中期（可选，测试基建）**：新增 dev-only example（`examples/seed_session.rs`，不进发布 feature）经 `wf_api` 实体层/`agent_loop_registry` 公开写路径向 sqlite 注入合成长会话；若 registry 无创建 API，则该 example 需 entity 层支撑，列为开放项与存储 owner 对齐。
3. 失败分支冒烟（错误行替换 Loading 占位）已可在空库通过 nonexistent 会话触发，纳入 I7 脚本作为保底断言。

**验收**：脚本对 fixture 库重复执行 §8.4 第 4 项断言通过；`streamed_equals_full` 类既有测试不回归。

### I3 — SIGUSR2 主题热更新的可解释性（用户可编辑主题文件）

**根因**：热更新链路（`theme_reload_signals` → `probe_theme` → 事件循环更新 `self.theme`，`tui.rs:183-188/303-307`）只依赖 OSC 终端颜色查询；无 OSC 应答（tmux、多数终端模拟器不常驻应答或需真实终端）时 probe 落回缓存主题，用户无法通过"编辑主题文件 + 发 USR2"实现可感知切换。

**方案**：
1. 引入用户主题文件优先：新增 `$XDG_CONFIG_HOME/wf-cli/theme.json`（config 目录，与缓存区分）作为显式主题源；USR2 处理顺序改为 **显式主题文件 → OSC probe → last-known-good 缓存**。
2. `Theme` 序列化结构现成（`theme_cache_roundtrip` 单测已覆盖存取），复用同一 `serde` 格式，仅新增读取路径与 source 标记（如 `ThemeSource::File`），避免与缓存快照混淆。
3. 事件循环消费处不变；Session 屏即刻跟随（`session.draw` 已接 `&Theme`），模态跟随另见 I6。

**验收**：
- PTY：写 light 版 theme.json → `kill -USR2` → `capture-pane -e` 前后字节不同；改回 dark → 再发 USR2 → 回变；
- 无文件时行为与现状一致（fallback 链不变）；非 TTY 降级不 panic；
- 单测覆盖优先级：File > probe > cache 的三态注入。

### I4 — E5 真·cursor 分页补载（存储层开放项）

**现状**：`replay_scrollack` 无分页语义，一次全量拉取后 `session.rs:237` 整页替换；`ReplayPhase::Partial` 是注释预留，滚顶无补载入口。强行在 wf-cli 内做"分页"仍需先全量拉取，属伪造（复核文档 §8.1 决策一致）。

**方案（依赖存储层，分三阶段）**：
1. **存储层**（owner：wf-api/entity + wf-storage）：消息查询 API 增加游标形态（`before_timestamp + limit + has_more`，对齐既有 `sort_by_key(timestamp)` 降序），或提供 continuation token；单测覆盖边界（首条游标、空页、恰好 limit）。
2. **replay 层**：从 `replay_scrollack` 提取共享的行转换逻辑，新增分页读取函数；保留全量版供 mini 模式与既有测试（不删除）。
3. **Session 层**：`ReplayPhase` 扩展 `Partial`；`load_replay` 首屏限量；检测"选中行在顶部 + ScrollUp + Partial"→ 后台补载 → 回投 `ReplayLoaded(earlier)` 时**前插**而非整体替换（现有 `:336` 分支按 phase 区分语义）。
4. 文档化降级：存储层短期不落地时，将"全量拉取 + 长会话内存成本"列入已知限制并标注依赖，不私自造游标。

**验收**：滚动到顶自动补载更早历史；`Partial→LoadingBeginning→Partial/Complete` 状态机单测；mini 路径零回归。

### I5 — 退出防 failover（`active_shutdown`，跨 wf-runtime，开放）

**现状**：主动退出（两按退出 / `/quit` / SIGINT）的 execution 在运行期关闭时仍可能被派发 `Failed`，产生退出瞬间的错误行或后台重试；全仓无 `active_shutdown`。

**方案（两步走，第一步无需 owner）**：
1. **TUI 本地防护（可先行）**：`TuiApp::run` 退出清理段先置本地 graceful 标志再 `session.shutdown() + adapter.shutdown()`，抑制自身 UI 侧错误渲染——改动小、只护 TUI。
2. **运行期全局标记（需 owner 拍板）**：在 `wf-runtime` 关闭路径标记 `active_shutdown`，事件 dispatch（wf-api / wf-agent）见标不派发 `Failed`。落点选项：runtime 关闭路径 vs dispatch 层，与复核文档 §8.1 决策一致，**本文档不代为修改 wf-runtime**。

**验收**：代码审查 + 注入单测（构造退出期间到来的 `Failed` 事件，断言不落屏/不派发）；真机冒烟需运行中会话配合，列为开发板项。

### I6 — 模态配色跟随热更新（可后置）

**现状**：仅 `session.draw` 接收 `&self.theme`（`tui.rs:430`）；Modal trait draw 无 theme 参数，8 个模态内部走回退主题，I3 落地后会出现 Session 屏与模态配色不一致。

**方案（两选一，建议先选 B 并文档化）**：
- **A（跟随）**：Modal trait draw 增加 `&Theme` 参数并透传 `self.theme`——波及 8 个实现 + 调用点，改动面大、视觉收益低；
- **B（维持 + 声明）**：保持现状，在主题文档中注明"模态当前使用内置回退主题"，与 I3 一并重新评估。

**验收**：若选 A，需每个模态快照测试更新；选 B 则无代码改动，仅文档声明。

### I7 — PTY 冒烟脚本化（验收基建）

**方案**：把 R1 的执行序列固化为可重复脚本，置于 `docs/scripts/tui-smoke.sh`（docs 下已有 scripts 目录，符合仓库文档组织），职责：
1. 启动 tmux 会话（可配置尺寸），拉起 `wf --tui --storage <spec>`；
2. 依次断言：Dashboard 首帧渲染 → `kill -TSTP`/`fg` 挂起恢复 → 输入型 Ctrl-Z（I1 落地后改为直接断言按键路径）→ resize 风暴 settle → `kill -USR2`（配合 I3 的 theme.json 切换）→ 会话屏错误路径冒烟 → Executions 空态 Enter 无崩溃；
3. 输出 PASS/FAIL 表与关键 `capture-pane` 证据文件，结束清理 tmux；
4. 每个断言独立函数、失败不中断后续项，退出码聚合，供 CI/开发板复用。
5. README 段说明与复核文档 §8.4 五项的映射关系及 fixture（I2）的挂载方式。

**验收**：开发板执行全绿；无 TTY 环境自动跳过并明示（不伪装通过）。

### I8 — 过期文档同步

**方案**：更新 `docs/plan/cli/wf-cli-剩余问题-TUI深化.md`：E1–E4 由"规划/缺口"改为"已落地"，锚点替换为现状复核文档 §2 的真实行号；E5/E6 标注"部分落地 + 决策记录"并链接复核文档 §8.1；状态头从"待评审"改为"已落地/归档"。

**验收**：文档与 HEAD 代码事实一致；复核文档 §七 要求的同步项闭合。

---

## 四、依赖、owner 与建议执行顺序

| 顺序 | Issue | 依赖 | Owner | 工作量评估 |
| :--- | :--- | :--- | :--- | :--- |
| 1 | I1 Ctrl-Z 按键挂起 | 无 | wf-cli | 小（双形态各一处分支 + 帮助文案） |
| 2 | I7 冒烟脚本 | I1（按键断言） | wf-cli | 中（脚本 + 断言函数） |
| 3 | I3 主题文件热切换 | I7（验收载体） | wf-cli | 中（读路径 + source 标记 + 单测） |
| 4 | I2 replay fixture | I7 | wf-cli（fixture 需真实环境生成） | 中（fixture + 集成断言） |
| 5 | I8 文档同步 | 无 | wf-cli | 小（纯文档） |
| 6 | I4 cursor 分页 | 存储层 API | wf-api / 存储 owner（开放） | 大（三阶段） |
| 7 | I5 active_shutdown | owner 拍板 | wf-runtime owner（开放） | 小（本地）+ 中（全局） |
| 8 | I6 模态主题 | I3（联动评估） | wf-cli | 小（选 B 则仅文档） |

原则：I1/I7/I3 可在本轮立即执行（无外部依赖）；I2 的 fixture 生成依赖真实环境数据；I4/I5 明确归属开放项，先与 owner 对齐再动工，避免单方面加标志/造游标（复核文档 §8.1 的既有决策保持一致）。

---

## 五、验证与验收策略

| 层 | 用例 | 载体 |
| :--- | :--- | :--- |
| 纯函数 | `ThemeSource` 优先级三态；ReplayPhase 状态机（I4 落地后） | `cargo test -p wf-cli` |
| 集成 | fixture 库下 Executions→Enter 下钻断言（I2）；graceful 标志注入单测（I5） | `tests/` + I7 脚本 |
| PTY 冒烟 | 挂起/恢复、Ctrl-Z 按键、resize、USR2 色变、会话错误路径 | `docs/scripts/tui-smoke.sh` |
| 合规 | `cargo clippy --all-targets --all-features`；`cargo fmt` | CI |

---

## 六、与既有文档关系

- 本文档是复核文档 §8.4（运行时未覆盖项）的执行收尾记录与 issue 归档：I1/I2/I3/I7 对应 §8.4 五项中"受阻/部分"项及实测发现，I4/I5/I6 对应 §5/§8.1 遗留开放点，I8 对应 §七 文档同步要求。
- 遵循 `AGENTS.md`：代码注释仅英文、不引用文档结构标识符；本文件为中文规划，只给自然语言描述、精确锚点与最小示意，不含完整代码片段；模块保持 `lib.rs` 扁平声明、无 `mod.rs`。
- I4/I5 涉及跨 crate 修改，本文档仅给方案与落点建议，**不代为修改 wf-runtime 与存储层**。
