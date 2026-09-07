# wf-cli 示例补充方案：mini 模式与 TUI 界面演示

> 状态：已完成（mini_panels.rs、tui_screens.rs 已交付）
> 上游方案：`docs/plan/cli/wf-cli-stage6-mini模式-实施方案.md`、`docs/plan/cli/wf-cli-stage7-全屏TUI完整模式-细化实施方案.md`
> 范围：补充 wf-cli 的 examples 目录，为 mini 模式面板和 TUI 各界面提供演示

## 一、现状分析

### 1.1 原有示例清单

| 示例文件 | 功能描述 | 覆盖范围 |
| :--- | :--- | :--- |
| `mini_demo.rs` | Mini 管线展示：合成事件驱动 reducer/footer/markdown/审批/问题管线 | mini 核心渲染管线 |
| `tui_animation.rs` | TUI 动画示例：模拟 LLM token 流式渲染 | TUI 渲染性能调试 |
| `tui_diff_record.rs` | Diff 记录示例：捕获每帧 ANSI diff 字节 | TUI 性能分析 |
| `tui_key_receiver.rs` | 键位接收示例：手动测试键位绑定 | 键位映射验证 |
| `component_output.rs` | 组件输出生成：生成 UI 组件的 golden output | 组件渲染验证 |

### 1.2 功能覆盖缺口

#### mini 模式缺口

| 功能模块 | 现有覆盖 | 缺口说明 |
| :--- | :--- | :--- |
| 命令面板 | 未覆盖 | `/` 命令触发、过滤、选择执行 |
| 模型选择面板 | 未覆盖 | `/model` 命令、模型列表、切换确认 |
| 技能面板 | 未覆盖 | `/skills` 命令、技能列表、选择插入 |
| 队列面板 | 未覆盖 | `/queued` 命令、队列管理、编辑删除 |
| 工作流面板 | 未覆盖 | `@workflow` 提及、工作流选择 |
| 提及面板 | 未覆盖 | `@` 触发、文件/技能/工作流补全 |

#### TUI 界面缺口

| 界面 | 现有覆盖 | 缺口说明 |
| :--- | :--- | :--- |
| Dashboard | 未覆盖 | 仪表板数据展示、导航入口 |
| Workflow | 未覆盖 | 工作流列表、详情查看 |
| Executions | 未覆盖 | 执行记录列表、状态过滤 |
| Session | 未覆盖 | 会话流式渲染、交互式对话 |
| Checkpoints | 未覆盖 | 检查点列表、恢复操作 |
| Search | 未覆盖 | 搜索输入、结果展示 |
| Settings | 未覆盖 | 设置展示、配置修改 |
| Help | 未覆盖 | 帮助文档展示 |

## 二、设计方案

### 2.1 新增示例清单（已实现）

| 示例文件 | 功能描述 | 类型 |
| :--- | :--- | :--- |
| `mini_panels.rs` | Mini 面板展示：命令面板、模型面板、技能面板、队列面板、工作流面板、提及面板 | 非交互（stdout 输出） |
| `tui_screens.rs` | TUI 界面演示：各界面的数据展示和键盘导航 | 交互式 TUI |

> 注：`mini_session.rs`（会话管理演示）因会话管理依赖真实 DomainAdapter 运行时，合成数据难以完整模拟，暂不实施。

### 2.2 示例设计详情

#### 2.2.1 `mini_panels.rs` - Mini 面板展示

**功能目标**：
- 展示命令面板的完整列表和过滤功能
- 展示模型面板的多配置文件列表和当前标记
- 展示技能面板的技能列表和导航
- 展示队列面板的待处理提示词列表
- 展示工作流面板的工作流列表
- 展示提及面板的组合列表和过滤功能
- 展示命令直接查找功能

**运行方式**：
```bash
cargo run -p wf-cli --example mini_panels
```

#### 2.2.2 `tui_screens.rs` - TUI 界面演示

**功能目标**：
- 演示 TUI 模式各界面的数据展示（Dashboard、Workflow、Executions、Checkpoints、Search、Settings、Help）
- 展示界面间的键盘导航（1-8 数字键、j/k 上下选择）
- 展示合成数据驱动的界面渲染
- 支持 Esc 返回仪表板、q 退出

**运行方式**：
```bash
cargo run -p wf-cli --example tui_screens
```

## 三、实现记录

### 3.1 已完成

| 阶段 | 任务 | 状态 |
| :--- | :--- | :--- |
| 阶段 1 | 创建 `mini_panels.rs` 示例 | 已完成 |
| 阶段 2 | 创建 `tui_screens.rs` 示例 | 已完成 |
| 阶段 3 | 编译验证 + clippy + fmt | 已完成 |

### 3.2 验证结果

- `cargo check -p wf-cli --examples` — 通过（无警告）
- `cargo clippy -p wf-cli --example mini_panels --example tui_screens` — 通过
- `cargo fmt -p wf-cli` — 通过
- `cargo run -p wf-cli --example mini_panels` — 运行正常，输出完整面板渲染

## 四、现有示例完整清单

| 示例文件 | 功能描述 | 运行方式 |
| :--- | :--- | :--- |
| `mini_demo.rs` | Mini 管线展示（核心渲染管线） | `cargo run -p wf-cli --example mini_demo` |
| `mini_panels.rs` | Mini 面板展示（命令/模型/技能/队列/工作流/提及面板） | `cargo run -p wf-cli --example mini_panels` |
| `tui_animation.rs` | TUI 动画示例（渲染性能调试） | `cargo run -p wf-cli --example tui_animation` |
| `tui_screens.rs` | TUI 界面演示（各界面展示 + 键盘导航） | `cargo run -p wf-cli --example tui_screens` |
| `tui_diff_record.rs` | Diff 记录示例（性能分析） | `cargo run -p wf-cli --example tui_diff_record --features diff-record` |
| `tui_key_receiver.rs` | 键位接收示例（键位验证） | `cargo run -p wf-cli --example tui_key_receiver` |
| `component_output.rs` | 组件输出生成（golden output） | `cargo run -p wf-cli --example component_output` |
