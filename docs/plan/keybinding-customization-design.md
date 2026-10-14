# 快捷键（Keybinding）现状分析与自定义快捷键设计方案

> 结论速览：
>
> 1. 前端**建议引入** `@tanstack/hotkeys`（配套 svelte 适配层），用统一的键位注册中心替换当前散落的 `keydown` 监听；现有代码均为单键和弦，迁移成本低。
> 2. 前端目前**不支持**自定义快捷键；按键处理分散在各组件内、且无持久化配置。
> 3. TUI **框架层已具备**完整的 keymap 抽象（`tui-core/keymap.rs`），但运行时 `wf-tui` 未接入（硬编码 match），`wf-config` 亦无键位字段——自定义能力**未打通**，方案核心是"让运行时走 Keymap + wf-config 提供覆盖配置"。

## 一、参考目录 ref/hotkeys 分析

`ref/hotkeys` 是 TanStack/hotkeys 的完整 monorepo，核心包为 `packages/hotkeys`（框架无关内核），并按框架提供适配层：`react-hotkeys` / `vue-hotkeys` / `svelte-hotkeys` / `solid-hotkeys` / `angular-hotkeys` / `alpine-hotkeys` / `ember-hotkeys` / `lit-hotkeys` / `preact-hotkeys`。

内核能力（`packages/hotkeys/src/`）：

- `parse.ts` / `match.ts`：按键描述解析与匹配（单键、序列 sequence、按住 held）；
- `hotkey-manager.ts`：全局注册表，统一 scope/enable/disable；
- `conflicts.ts` / `validate.ts`：键位冲突检测与合法性校验；
- `hotkey-recorder.ts` / `hotkey-sequence-recorder.ts`：**录制用户按键**作为绑定（自定义快捷键的关键能力）；
- `display-labels.ts` / `format.ts`：跨平台展示格式化（⌘ / Ctrl）；
- `platform.ts`：平台差异（macOS Meta 等）。

对本项目的适配性：前端是 Svelte 5（SvelteKit），对应适配包为 `@tanstack/svelte-hotkeys`（提供 `createHotkey(s)` / `createHotkeyRecorder` 等）。

### 是否引入依赖？

**建议引入**，理由：

- 当前前端按键处理分散在至少 8 处，各自手写 `keydown` + 修饰键判断 + `isTyping` 防误触，语义重复且无统一优先级/冲突处理（见下节）。
- `canvas-shortcuts.ts` 已经自发演化出一个"注册表 + 帮助表"的雏形，说明统一注册中心是自然需求；自己实现 recorder（录制自定义键）成本远高于复用。
- 内核包体积小、框架无关，适配层薄，不引入重运行时。

若希望零依赖，也可只借鉴其"描述字符串解析 + 注册表 + 冲突检测"三件套自研，但 recorder、sequence、跨平台展示需自行维护，性价比低。下文方案按"引入依赖"展开，核心结构对自研路线同样适用。

## 二、前端现状与差距

现有按键处理（均为一次性 `addEventListener('keydown')` / `svelte:window` / 组件 `onkeydown`，硬编码键位）：

| 位置 | 键位 | 说明 |
| --- | --- | --- |
| `AppShell.svelte` | F1 | 全局帮助开关 |
| `CommandPalette.svelte` | Ctrl/⌘+K | 命令面板 |
| `TranscriptScroller.svelte` | Ctrl+Home/End | 转录跳顶/底 |
| `Composer.svelte` | Enter / Shift+Enter | 发送/换行 |
| `lib/graph/canvas-shortcuts.ts` | F、R、+/-、C、E、Esc、Ctrl+S/Z/A、Del | 画布快捷键，含帮助表 `CANVAS_SHORTCUT_HELP` 与 `isTyping` 守卫 |
| `GraphCanvas.svelte` | Esc | 连线取消 |
| `AddNodeDrawer.svelte` / `CommandPalette.svelte` 内部 | ↑/↓/Enter | 列表导航 |
| `GitKeyInput.svelte` | Enter | 提交 |

差距：

1. **无统一注册中心**：无 scope 概念（输入框聚焦、抽屉打开等上下文互斥靠各处手写 `isTyping` / `if (!open)`）。
2. **不可配置**：键位全部硬编码，无持久化（localStorage / 服务端 settings），无冲突检测。
3. **无录制能力**：无法让用户"按下组合键"来重绑。
4. **帮助展示与实现脱节**：`CANVAS_SHORTCUT_HELP` 是手写副本，改键不会同步。

## 三、TUI 现状与差距

- `crates/app/tui/tui-core/src/keymap.rs` 已有完整抽象：
  - `CKey` / `Key`（chord，含 ctrl/alt/shift，含 macOS Option 字符归一化 `normalize_key`）；
  - `KeyAction`（抽象动作，约 30 项：Quit/Help/Approve/Submit/…）；
  - `KeymapContext`（Global/Composer/Approval/Question/Modal/Panel 等上下文）；
  - `Keymap`（支持 `set_global` / `bind(ctx, key, action)` / `resolve`），`builtin_keymap()` 提供默认绑定，context 覆盖优先于 global；
  - 组件层（`tui-components` 的 approval_overlay、panels、modal 等）均已按 `KeyAction` 编程。
- **但运行时未接入**：`crates/app/cli/wf-tui/src/tui/key_routes.rs`、`input_classify.rs`、`interactive/keys.rs` 直接对 `Key`/`CKey` 硬编码 `match`，未构造 `Keymap` 实例做 `resolve`。也就是说 `Keymap` 目前只被 `wf-cli-demo` 示例使用。
- `wf-config` 中无 keybind/shortcut/keymap 任何字段——没有配置来源。
- 帮助视图（HelpModal / footer 提示）同样硬编码键位文案。

结论：TUI 的"自定义快捷键"缺的不是抽象，而是**三段接线**：运行时改走 `Keymap::resolve` → `wf-config` 增加键位配置 → 覆盖项合并进 `builtin_keymap()`。

## 四、设计方案

### 4.1 前端：统一键位注册中心（引入 @tanstack/hotkeys）

1. **依赖**：`apps/web-app` 增加 `@tanstack/hotkeys`（内核）+ `@tanstack/svelte-hotkeys`（适配层）。`web-app-preview` 由同步脚本镜像，无需单独处理。
2. **动作注册表** `lib/shortcuts/`：
   - `actions.ts`：声明式定义全部动作 `{ id, description, defaultKeys, scope, handler }`。scope 枚举：`global` / `canvas` / `palette` / `transcript` / `input`。现有 `CANVAS_SHORTCUT_HELP` 迁移为该表的一部分，帮助浮层（F1）从注册表生成，消除手写副本。
   - `registry.ts`：将动作表注册进 hotkey-manager，统一处理 scope 激活、`isTyping` 守卫（输入框内仅放行显式允许的键）与冲突上报。
   - 组件改造：`AppShell` / `CommandPalette` / `TranscriptScroller` / `GraphCanvas` 等逐步替换为从注册表订阅；输入框内行为（Enter 发送等）保留组件内 `onkeydown`，不纳入全局表。
3. **自定义与持久化**：
   - `settings.ts`：用户覆盖 `{ actionId: keys | null }`（null 表示禁用），存 localStorage（键名如 `wf.shortcuts.overrides.v1`）；若后续 wf-server 提供 per-user settings API，则迁移到服务端，localStorage 仅作离线缓存。
   - 生效方式：注册表初始化时 `defaultKeys` 先经过 override 合并，再交给 hotkey-manager。
4. **录制 UI**：设置页新增"快捷键"分区：动作列表（来自注册表）+ `createHotkeyRecorder` 录制新键 + 冲突检测（conflicts）提示 + 恢复默认。

### 4.2 TUI：打通 Keymap 与配置层

1. **运行时接入 Keymap**（`wf-tui`）：
   - 会话启动时构造 `Keymap`：`builtin_keymap()` + 用户覆盖合并，随 `ScreenState` 持有；
   - `tui/key_routes.rs`、`tui/input_classify.rs`、`interactive/keys.rs` 的硬编码 `match` 改为 `keymap.resolve(current_context, key)` 后按 `KeyAction` 分派。此步是重构主体，需按 `KeymapContext` 逐段迁移并保持行为等价（现有 keymap 单测 + `wf-cli-demo/examples/tui_key_receiver.rs` 可作对照）。
   - HelpModal / footer 键位提示改为从当前 `Keymap` 反查（遍历绑定生成文案），消除手写文案。
2. **wf-config 增加键位配置**：
   - `wf-config` 增加 `keymap` 配置节（TOML），按 context 分组，值为按键描述字符串，例如：

     ```toml
     [keymap.global]
     quit = "ctrl+q"
     help = "?"

     [keymap.composer]
     submit = "enter"
     ```

   - 新增字符串 ↔ `Key` 的解析/序列化（`"ctrl+shift+s"` 等，沿用 `CKey` 支持的键集合；不支持键直接报配置错误）。
   - 配置加载后生成 override 列表（`(KeymapContext, Key, KeyAction)`），在 `builtin_keymap()` 基础上 `bind` 合并；对"同一 Key 绑定到两个 Action"做冲突校验（后绑定覆盖 + 记录告警）。
3. **冲突与合法性**：在 `tui-core/keymap.rs` 补充 `validate`：检测重复 Key、保留键（如 ctrl+c 的进程中断语义）警告；单测覆盖。
4. **持久化位置**：沿用 `wf-config` 现有配置文件发现与读写路径；不做 TUI 内改键 UI（可作为后续项，用录制思路在 `/keys` 命令面板中实现）。

### 4.3 前后端一致性（可选后续项）

键位动作 ID 两端各自维护即可（动作集不同：前端面向页面导航，TUI 面向会话交互），不建议强行共享枚举；若未来桌面端出现，可共享"按键描述字符串格式"这一层约定。

## 五、实施步骤（建议顺序）

1. TUI：`wf-config` 增加 `keymap` 配置节与字符串解析 + 单测；
2. TUI：`wf-tui` 运行时迁移到 `Keymap::resolve`（分 context 分批），帮助文案改为反查；
3. 前端：建立 `lib/shortcuts/` 注册中心，先迁移 `canvas-shortcuts.ts`（已有雏形、收益最直接），再迁移全局键；
4. 前端：设置页自定义快捷键 UI + localStorage 持久化 + 冲突检测；
5. 文档：用户文档补充自定义快捷键配置说明。
