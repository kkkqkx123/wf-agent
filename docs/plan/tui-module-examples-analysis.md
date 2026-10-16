# TUI 模块功能分析与 Example 补充计划

> 目的：梳理 `crates/app/tui/` 下各 crate 的职责，并针对"人工审查渲染后结果是否符合预期"这一目标，规划每个 crate 需要补充的 examples。
> 现有 examples 位于 `crates/app/cli/wf-cli-demo/examples/`（作为所有 TUI 示例的统一落点）。

## 一、现有 Examples 覆盖情况

| Example | 演示内容 |
|---|---|
| `component_output.rs` | 共享 UI 组件的黄金输出生成器（写入 `outputs/` 供 diff 审查） |
| `tui_animation.rs` | LLM token 流式模拟 + 循环动画，可配合 diff-record |
| `tui_bench.rs` | 无头吞吐探针（prep cache / markdown 稳态行，性能回归） |
| `tui_diff_record.rs` | DiffRecorderBackend 集成演示，录制每帧 ANSI 字节 |
| `tui_key_receiver.rs` | 按键解析与 KeymapContext 手动测试工具 |
| `tui_management_showcase.rs` | 统一 Screen 导航壳层 smoke test（Dashboard/Workflow/.../Catalog） |

**缺口**：仅覆盖了 screens、按键、动画与性能；markdown 渲染、style/主题/动效、components 各弹窗/浮层、render 管线细节均缺少专门的渲染审查示例。

## 二、各 Crate 功能分析

### tui-clock（叶子）
| 模块 | 功能 |
|---|---|
| `clock` | 可注入的毫秒时钟，供 animation、expiry、pacing 共享 |

### tui-terminal
| 模块 | 功能 |
|---|---|
| `terminal` | inline / full-screen 两种形态共享的终端交互设施 |
| `capabilities` / `probe` | 终端能力探测：键盘增强、色彩深度、bracketed paste 等 |
| `sigint` | Ctrl+C 双击状态机（时间窗内两次按下） |
| `stderr` | stderr 抑制守卫（激活期间 fd 2 重定向到文件） |
| `editor` | 外部编辑器交接：挂起/恢复终端 |
| `liveness` | 终端存活检测，发现脱离的 UI 客户端 |

### tui-style
| 模块 | 功能 |
|---|---|
| `theme` | 主题检测：调色板派生、last-known-good 缓存、用户覆盖 |
| `theme_mode` | 缓冲区级 light/dark 适配（每帧一次） |
| `animation` | 组件动画系统 |
| `anim_core` | 确定性动画数值核心（无副作用，可测试） |
| `motion` | 运动与动画工具函数 |

### tui-markdown
| 模块 | 功能 |
|---|---|
| `blocks` | Markdown 块级结构分析 |
| `document` | 后端中立文档模型 |
| `plain` / `styled` | 整篇渲染：纯文本 / ratatui styled Lines |
| `stream` | 增量流式渲染 |
| `reasoning` | reasoning 段落共享契约 |

### tui-core（kernel）
| 模块 | 功能 |
|---|---|
| `events` / `event_dispatch` | 统一事件流与具体事件类型 |
| `reducer` | 事件归约 kernel（inline / full 共享） |
| `render_model` / `renderable` / `screen_data` | 只读渲染视图、统一渲染接口、屏幕显示模型 |
| `framer` / `redraw` | 帧调度（合并 draw 请求、限速）、重绘分级 |
| `stream_pacer` | 流式节奏控制（delta 到达 vs 可见文本） |
| `keymap` / `prep_keys` | 集中按键解析（上下文回退）、scrollback prep 缓存键 |
| `frame_metrics` / `perf` | 帧指标与预算断言、能力分级性能策略 |
| `anchor` | 滚动/流式锚点稳定性（视觉回归信号） |
| `width` | 宽度测量抽象 |
| `status_line` | footer 状态行状态 |
| `headless` | `ExecutionStreamEvent` → 纯文本无头汇总渲染 |

### tui-render
| 模块 | 功能 |
|---|---|
| `layout` | 统一帧布局：chat / diagram / management 分栏 |
| `ansi` | ANSI 转义 → ratatui Line 管线 |
| `prep_cache` / `screen_cache` | scrollback / 管理屏幕的增量准备缓存 |
| `screen_draw` | 各屏幕逐屏渲染实现 |
| `post_process` | 固定顺序的帧收尾管线 |
| `deferred` | 帧延迟重活：图片与图表注册 |

### tui-components
| 模块 | 功能 |
|---|---|
| `transcript` / `composer` / `footer` / `bottom_pane` | 会话记录原语、单行输入 composer、底部窗格与状态行组合 |
| `panels` | footer 内联视图的选择面板（`/` 命令面板等） |
| `mention` | `@` file/skill/workflow mention 解析 |
| `queue` | 提示词队列（inline 会话） |
| `select` | 全列表屏共用 SelectList 导航组件 |
| `modal` / `overlay` | 弹窗与弹窗栈、overlay 模式与事件反馈 |
| `confirm_modal` / `password_modal` / `help_modal` | 确认 / 秘密输入 / 帮助弹窗 |
| `approval_overlay` / `question_overlay` | 工具审批 / 追问视图 |
| `model_picker` / `file_selection` | 模型/会话选择器、目录浏览选择弹窗 |
| `file_viewer` | 只读文本查看与 diff 查看弹窗 |

### tui-debug
| 模块 | 功能 |
|---|---|
| `diff_recorder` | 后端装饰器：录制每帧 ANSI diff 字节 + FrameMetrics（`diff-record` feature 门控） |

## 三、当前结论（2026-10-16 更新）

在最近 TUI 统一期之后，demo 组织建议如下：

1. `tui_management_showcase` 作为**集成 smoke test** 保留，不拆分成每个 screen 单例 demo。它覆盖的内容包括 `Screens` 导航栈、合成 `ScreenData`、`ScreenContext.filter`、Catalog tab/filter 及详情选中，所有这些实际上都依赖 screen stack 联动，拆开会丢失高风险交互。
2. 不 recomend 为每个 `ScreenKind` 或每个 crate 新建独立 demo，除已经存在的 focused 演示：
   - `tui_components_gallery`：TUI 组件画廊；
   - `tui_style_gallery`：主题、动画、motion；
   - `tui_markdown_showcase`：markdown 整篇与流式渲染对照；
   - `tui_diff_record` / `tui_bench` / `tui_key_receiver` / `tui_scenario` / `component_output`：工具链与交互 parse/perf 回归。
3. 仅当同时满足以下 2 项以上时，才新增 demo：
   - 独占交互模式（例如 Catalog tab + filter、command palette completion、Markdown streaming）；
   - 集成 demo 难以稳定复现；
   - 新 demo 可以使用 synthetic data，低成本搭建；
   - 风险/审查收益明显。
4. 其余画廊补充优先在现有 demo 中扩展（`tab_bar`、`property_list`、`footer`、`history cell` 等 visual surface），而不是新增 example。

## 四、长期规范

- 每个新 demo 必须在 `examples/tui/README.md` 和 Cargo `[[example]]` 块中注册，避免 demo 孤岛化。
- 命名沿用 `tui_<surface>_showcase` / `tui_<tool>` 规则，不使用泛化 `screen`（screen 适合作为 Screen stack 的总览命名例外）。
- 所有 demo 使用合成数据；不要在 example 中接真实 server。
- 涉及 TUI 交互时必须更新 `docs/plan/tui-interface-reference-alignment-design.md` 里的人工验收 Checklist。
