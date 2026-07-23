# Claude Code TUI 按键体系完整介绍
> **说明**：Claude Code TUI = `claude /tui` 启动的全屏终端交互界面（Fullscreen Interactive TUI）。
整体设计思路：**模态分层 + Vim 风格导航 + 上下文隔离按键绑定**。按键按**上下文（Context）**区分：全局全局热键、对话输入模式（Chat/Insert）、浏览普通模式（Normal）、Vim 视觉选择模式、弹窗菜单模式。
可执行 `/keybindings` 打开 `~/.claude/keybindings.json` 自定义全部按键。

## 一、核心设计架构
1. **上下文隔离（Context Binding）**
    同一按键在不同区域行为不同：
    - `Global`：全局任何界面生效
    - `Chat`：底部输入框激活（插入模式）
    - `Normal`：焦点在对话历史区（浏览模式，Vim 键可用）
    - `Visual`：文本选区模式
    - `Modal`：弹窗/选择菜单（rewind、模型选择、权限弹窗）
2. **双导航体系兼容**
    - 原生方向键/PageUp/PageDown（常规终端用户）
    - Vim 风格 hjkl / g/G / Ctrl+u/Ctrl+d（重度开发者）
3. **模态切换逻辑**
    TUI 默认两种基础状态：
    - **插入模式（Chat）**：光标在底部输入框，可以打字；
    - **浏览模式（Normal）**：按 `Esc` 将焦点切向上方对话面板，使用 Vim 键滚动浏览历史。

---

## 二、全局热键（Global Context，任何界面可用）
| 按键 | 功能 |
|------|------|
| `Ctrl+C` | 中断当前 AI 生成 / 工具调用；连续两次强制退出会话 |
| `Ctrl+D` | 退出 Claude Code TUI（EOF，等价 `/exit`） |
| `Ctrl+L` | 重绘/刷新终端界面，修复花屏，不丢失对话历史 |
| `Ctrl+/` | 打开快捷键帮助面板 |
| `Shift+Tab` | 循环切换**权限模式**：自动接受 / 计划审批 / 手动确认 |
| `Ctrl+O` | 切换工具调用详情日志显示（简洁/完整输出） |
| `Ctrl+B` | 将正在执行的 bash 工具任务转入后台运行 |
| `Esc` | 通用退出：关闭弹窗、取消操作、切到 Normal 浏览模式 |

> Tmux 用户注意：`Ctrl+B` 会被 tmux 捕获，需要连续按两次 `Ctrl+B`。

## 三、插入模式 Chat（底部输入框激活，默认初始状态）
适合输入提示词，兼容 readline 标准编辑快捷键：
### 输入编辑
| 按键 | 作用 |
|------|------|
| `Enter` | 提交消息，发送给 Claude |
| `Shift+Enter` | 输入框内换行（不发送） |
| `Ctrl+A` | 光标移至行首 |
| `Ctrl+E` | 光标移至行尾 |
| `Ctrl+U` | 清空当前整行输入 |
| `Ctrl+K` | 删除光标到行尾文本 |
| `↑ / ↓` | 调取历史输入记录 |
| `Ctrl+R` | 历史命令搜索（反向查找） |
| `Ctrl+V / Cmd+V` | 粘贴文本/粘贴图片（多模态） |
| `Ctrl+G` | 使用外部编辑器（$EDITOR）编写提示词 |

### 特殊连续按键
- `Esc Esc`（输入框为空）：打开 **Rewind 对话回滚菜单**，跳转历史检查点；
- `Esc`（单次）：离开输入框，进入 **Normal 浏览模式**。

## 四、Normal 浏览模式（焦点在对话面板，Esc 切进来）
> 此模式**不能直接打字**，专注浏览上下对话、代码块、工具日志；完整支持 Vim 导航。
### 滚动导航
| 按键 | 功能 |
|------|------|
| `j / ↓` | 向下滚动1行 |
| `k / ↑` | 向上滚动1行 |
| `Ctrl+u` | 向上滚动半屏 |
| `Ctrl+d` | 向下滚动半屏 |
| `Ctrl+b / PageUp` | 向上整屏 |
| `Ctrl+f / PageDown` | 向下整屏 |
| `g / Ctrl+Home` | 跳转到对话最顶部 |
| `G / Ctrl+End` | 跳转到最新消息，恢复自动跟随滚动 |

### 文本选择（Visual Mode）
| 按键 | 功能 |
|------|------|
| `v` | 字符选择模式（Visual） |
| `V` | 整行选择模式（Visual Line） |
| `Esc` | 退出选区 |

选中后：`Shift+方向键` 扩展选区，可复制代码片段。

### 区块折叠
`Space`：展开/折叠工具调用块、长代码块、日志区域。

## 五、弹窗/菜单模态（Modal）
打开模型选择、rewind、权限配置、文件选择弹窗时：
- `↑ / ↓`：上下选择条目
- `Enter`：确认选中项
- `Esc`：关闭弹窗，放弃操作

## 六、常用斜杠命令（TUI 输入框内直接输入 `/xxx`）
不属于按键，但配合 TUI 使用高频：
```
/keybindings   打开快捷键配置文件
/help          帮助
/model          切换模型
/clear          清空对话
/vim            切换长期 Vim 编辑模式
/rewind         对话回滚（等价 Esc Esc）
/exit           退出
```

## 七、自定义按键示例 keybindings.json
```json
{
  "$schema": "https://www.schemastore.org/claude-code-keybindings.json",
  "bindings": [
    {
      "context": "Chat",
      "bindings": {
        "ctrl+enter": "chat:submit",
        "enter": "chat:newline"
      }
    },
    {
      "context": "Global",
      "bindings": {
        "ctrl+h": "global:toggleHelp"
      }
    }
  ]
}
```
修改文件自动热加载，无需重启 TUI。

## 八、新手最简工作流记忆
1. 启动：`claude /tui` → 默认【插入模式】直接写提示词；
2. 想翻看历史对话：按 **Esc** →【Normal模式】使用 `j/k/g/G` 浏览；
3. 浏览完毕：按任意字母键/Enter 切回底部输入框；
4. AI 回答跑偏：直接按 **Esc** 停止生成；
5. 需要回滚对话：输入框清空，连续按 **Esc Esc**；
6. 界面乱了：`Ctrl+L`；退出：`Ctrl+D`。
