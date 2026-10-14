# 自定义快捷键配置说明

本项目支持在前端（Web）与 TUI（终端）两侧自定义快捷键。两侧配置相互独立：前端配置保存在浏览器本地，TUI 配置保存在配置文件中。

## 前端（Web）

### 查看与修改

进入 **Settings → Shortcuts** 页面：

- 每个快捷键显示当前生效的按键组合与所属作用域（`global` 全局 / `canvas` 画布）；
- 点击 **Record** 后按下任意组合键即可重新绑定（录制中按 Escape 取消，按 Backspace/Delete 清除绑定）；
- 点击 **Disable** 禁用某个快捷键，点击 **Reset** 恢复默认；
- 保存时自动检测与现有绑定的冲突，冲突时会给出提示。

### 存储位置

覆盖配置保存在浏览器 localStorage（键名 `wf.shortcuts.overrides.v1`），仅对当前浏览器生效，不上传服务端。内容形如：

```json
{ "canvas.saveDraft": "Mod+Shift+s", "canvas.zoomIn": null }
```

值为新按键字符串，或 `null` 表示禁用。未出现在其中的动作使用内置默认值。按键写法遵循 TanStack hotkeys 语法：`Mod` 在 macOS 上代表 ⌘，在 Windows/Linux 上代表 Ctrl。

### 可配置的快捷键

| 动作 | 默认按键 | 作用域 |
| --- | --- | --- |
| 切换帮助 | F1 | global |
| 命令面板 | Mod+K | global |
| 转录跳顶 / 跳底 | Mod+Home / Mod+End | global |
| 画布适配视图 / 重排布局 | F / R | canvas |
| 画布缩放 | + / - | canvas |
| 聚焦选中节点 / 编辑模式 | C / E | canvas |
| 退出编辑模式 | Escape | canvas |
| 保存草稿 / 撤销 / 重做 / 全选 | Mod+S / Mod+Z / Shift+Mod+Z / Mod+A | canvas |
| 删除选中 | Delete | canvas |

其余按键（输入框内的 Enter 发送、列表 ↑/↓ 导航等）不纳入全局注册表，保持固定行为。

## TUI（终端）

### 配置文件

快捷键覆盖从两个位置加载，项目级文件优先于全局文件：

- 全局：`$XDG_CONFIG_HOME/wf/keymap-settings.toml`（或 `$HOME/.config/wf/keymap-settings.toml`）
- 项目：`<项目根>/.wf/keymap-settings.toml`

文件缺失或解析失败时直接使用内置默认键位，不会阻断启动。

### 文件格式

按键按「上下文 → 动作 → 按键描述」三级组织：

```toml
[keymap.global]
quit = "ctrl+q"
help = "?"

[keymap.composer]
submit = "enter"
```

### 上下文与动作

上下文（section 名）：`global`、`list`、`detail`、`chat`、`input`、`modal`、`composer`、`panel`、`approval`、`question`。

可用的动作名与内置默认键位可在 TUI 中按 `?` 查看帮助浮层；浮层末尾会列出当前生效的所有用户覆盖。常用动作包括：

| 动作 | 说明 |
| --- | --- |
| `quit` | 退出（global，默认 `ctrl+q`） |
| `help` | 帮助浮层（global，默认 `?`） |
| `back` | 返回/关闭（默认 `q` / `Esc`） |
| `move_prev` / `move_next` | 列表导航（默认 `k`/`Up`、`j`/`Down`） |
| `select` / `submit` | 确认 / 提交（默认 `Enter`） |
| `refresh` | 刷新当前视图 |
| `approve` / `approve_all` / `deny` / `deny_once` | 工具审批 |
| `clear` | 清空输入 |

按键描述写法：修饰键（`ctrl` / `alt` / `shift`，大小写与顺序不敏感）+ 基础键，用 `+` 连接。基础键支持：

- 命名键：`enter`、`esc`、`tab`、`up`、`down`、`left`、`right`、`home`、`end`、`pageup`、`pagedown`、`backspace`、`delete`、`space`、`f1`–`f12`
- 单字符：字母（自动转小写）、数字、标点（如 `?`、`/`、`+`）

示例：`ctrl+shift+s`、`alt+x`、`pageup`、`?`。

### 校验与限制

- 未知上下文或动作名的条目会被跳过并在日志中告警；
- 按键描述非法（多基础键、未知键名等）会被跳过并告警；
- 同一上下文中两个动作绑定到同一按键属于冲突，配置校验会报告该问题，后加载的条目生效。

## 相关实现

- 前端注册中心：`apps/web-app/src/lib/shortcuts/`（动作目录 `actions.ts`、覆盖持久化 `settings.ts`、合并与冲突检测 `registry.ts`）；
- TUI 键位表：`crates/app/tui/tui-core/src/keymap.rs`（`Keymap` / `KeyAction` / `KeymapContext`）；
- TUI 配置解析与合并：`crates/infra/wf-config/src/keymap.rs`；
- 设计方案：`docs/plan/keybinding-customization-design.md`。
