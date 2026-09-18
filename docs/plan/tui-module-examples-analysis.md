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
| `tui_screens.rs` | 全屏各页面（Dashboard/Workflow/...）带合成数据的展示 |

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

## 三、需要补充的 Examples（用于人工渲染审查）

原则：每个示例都以"合成数据 + 可交互/可退出 + 展示该 crate 全部视觉状态"为标准；优先放在 `wf-cli-demo/examples/`，命名沿用 `tui_*` 前缀。

### 1. tui-markdown：`tui_markdown_showcase.rs`（高优先级）
- 展示：标题/列表/代码块/表格/引用/分隔线的 `styled` 与 `plain` 两种整篇渲染；
- 同一文档分别以"一次性渲染"和"逐块流式喂入 `stream`"渲染，左右分屏对照，人工检查流式拼接结果与整篇渲染一致；
- 覆盖 reasoning 段落折叠/展开样式。

### 2. tui-style：`tui_style_gallery.rs`（高优先级）
- 展示：theme 调色板色样、`theme_mode` light/dark 双模式同屏切换（如按 `t` 切换）、动画/motion 各关键帧状态（提供暂停/步进键逐帧审查，而非仅实时播放）；
- `anim_core` 为纯数值核心无视觉，可在文档中说明不设示例。

### 3. tui-components：`tui_components_gallery.rs`（高优先级，可拆分）
- 每个弹窗/浮层一个按键入口：confirm / password / help / approval / question / model_picker / file_selection / file_viewer(文本+diff) / select / panels / mention / composer / footer / queue；
- 每个组件展示默认态 + 关键中间态（焦点、选中、空态、错误态）；
- 若单文件过大，按现有 `component_output.rs` 黄金输出机制补充对应 outputs 快照，交互示例仅演示操作路径。

### 4. tui-render：`tui_render_pipeline.rs`（中优先级）
- 展示：layout 三种分栏模式（chat / diagram / management）的合成帧；
- `ansi` 管线：喂入带 ANSI 转义的文本检查着色还原；
- `post_process` 收尾前后对照（可用分屏或按键切换）；
- prep_cache 增量命中/失效可通过 diff-record 日志观察（联动 tui-debug）。

### 5. tui-terminal：`tui_terminal_capabilities.rs`（中优先级）
- 展示：能力探测结果面板（键盘增强、色彩深度、bracketed paste 实测）；
- 演示 sigint 双击判定、stderr 守卫激活/解除、editor 挂起/恢复全程视觉状态。

### 6. tui-core：`tui_core_visual_signals.rs`（低优先级）
- core 多为逻辑 kernel；仅对有视觉后果的模块设示例：anchor 滚动稳定性（构造滚动+流式并发场景）、stream_pacer 节奏（快/慢到达对照）、redraw 分级（注入不同级别事件观察重绘范围）；
- framer/frame_metrics/perf/keymap 已有 `tui_bench` / `tui_key_receiver` 覆盖，无需重复。

### 7. tui-clock / tui-debug：无需新示例
- tui-clock 是纯叶子设施，其效果在 style 动画示例中体现；
- tui-debug 已有 `tui_diff_record` 示例覆盖。

## 四、优先级汇总

| 优先级 | Example | 覆盖 crate |
|---|---|---|
| P0 | `tui_markdown_showcase.rs` | tui-markdown |
| P0 | `tui_style_gallery.rs` | tui-style |
| P0 | `tui_components_gallery.rs` | tui-components |
| P1 | `tui_render_pipeline.rs` | tui-render |
| P1 | `tui_terminal_capabilities.rs` | tui-terminal |
| P2 | `tui_core_visual_signals.rs` | tui-core（仅视觉相关模块） |
| — | 无需补充 | tui-clock, tui-debug |
