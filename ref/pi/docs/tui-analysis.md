# TUI 模块功能分析

## 概述

`@mariozechner/pi-tui` 是一个极简终端 UI 框架，专为构建无闪烁的交互式 CLI 应用程序而设计。它采用差分渲染技术和同步输出机制，提供流畅的用户体验。

## 核心特性

### 1. 差分渲染 (Differential Rendering)
- 三策略渲染系统，仅更新发生变化的内容
- 大幅减少终端重绘，提升性能

### 2. 同步输出 (Synchronized Output)
- 使用 CSI 2026 转义序列实现原子屏幕更新
- 消除屏幕闪烁问题

### 3. 括号粘贴模式 (Bracketed Paste Mode)
- 正确处理大块文本粘贴
- 对超过 10 行的粘贴显示标记

### 4. 组件化架构
- 简单的 Component 接口，包含 render() 方法
- 支持主题定制

### 5. 图像支持
- 支持 Kitty 和 iTerm2 图形协议
- 在终端中内联显示图像

## 模块结构

```
tui/
├── src/
│   ├── components/          # UI 组件集合
│   │   ├── box.ts          # 带内边距的容器
│   │   ├── cancellable-loader.ts  # 可取消的加载器
│   │   ├── editor.ts       # 多行文本编辑器
│   │   ├── image.ts        # 图像渲染组件
│   │   ├── input.ts        # 单行输入框
│   │   ├── loader.ts       # 加载动画
│   │   ├── markdown.ts     # Markdown 渲染器
│   │   ├── select-list.ts  # 选择列表
│   │   ├── settings-list.ts # 设置面板
│   │   ├── spacer.ts       # 间距组件
│   │   ├── text.ts         # 多行文本
│   │   └── truncated-text.ts # 截断文本
│   ├── autocomplete.ts     # 自动补全功能
│   ├── editor-component.ts # 编辑器组件接口
│   ├── fuzzy.ts            # 模糊匹配算法
│   ├── index.ts            # 模块导出入口
│   ├── keybindings.ts      # 键盘绑定管理
│   ├── keys.ts             # 键盘输入解析
│   ├── kill-ring.ts        # Emacs 风格 kill/yank 缓冲区
│   ├── stdin-buffer.ts     # 标准输入缓冲
│   ├── terminal-image.ts   # 终端图像渲染
│   ├── terminal.ts         # 终端接口实现
│   ├── tui.ts              # 核心 TUI 类
│   ├── undo-stack.ts       # 撤销栈
│   └── utils.ts            # 文本工具函数
├── README.md
└── package.json
```

## 核心模块详解

### 1. tui.ts - 核心 TUI 引擎

**主要功能：**
- 差分渲染引擎
- 覆盖层 (Overlay) 系统
- 焦点管理
- 光标定位（支持 IME）

**关键类：**
- `Component` - 所有 UI 组件的基础接口
- `Container` - 可包含子组件的容器
- `TUI` - 主渲染引擎

**覆盖层功能：**
- 9 种锚点位置（center, top-left, bottom-right 等）
- 支持百分比和绝对定位
- 边距控制
- 可见性回调

### 2. terminal.ts - 终端接口

**Terminal 接口：**
- 输入/输出管理
- 光标控制
- 屏幕清除
- 标题和进度指示器

**ProcessTerminal 实现：**
- 原始模式标准输入
- 括号粘贴模式
- Kitty 键盘协议协商
- Windows VT 输入支持（通过 koffi）

### 3. keys.ts - 键盘输入处理

**支持功能：**
- 传统终端序列和 Kitty 键盘协议
- 按键匹配和解析
- 可打印字符解码

**支持的按键标识符：**
- 单键：escape, tab, enter, backspace, delete, home, end, space, 方向键
- 修饰符：ctrl, shift, alt, super
- 组合键：ctrl+c, shift+enter, ctrl+alt+x 等

### 4. components/editor.ts - 多行编辑器

**核心功能：**
- 光标移动（方向键、Home/End、Ctrl+方向键）
- 文本选择和删除（单词、行）
- 撤销/重做（支持合并）
- Kill ring（Emacs 风格 yank/yank-pop）
- 括号粘贴支持
- 大粘贴处理（带标记）
- 使用 Intl.Segmenter 的自动换行
- 自动补全集成（斜杠命令、@/# 提及）
- 历史导航（上下方向键）
- 字符跳转模式
- 垂直移动的粘性列

### 5. components/select-list.ts - 选择列表

**功能特性：**
- 上下导航（支持循环）
- 前缀匹配过滤
- 滚动指示器
- 双列布局（标签 + 描述）
- 自定义截断

### 6. components/markdown.ts - Markdown 渲染器

**支持的元素：**
- 标题（H1 带下划线）
- 粗体、斜体、删除线、下划线
- 带语法高亮的代码块
- 带边框的引用块
- 列表（有序/无序，嵌套）
- 宽度感知的表格
- 带 OSC 8 超链接的链接
- 水平分隔线
- 图像行检测

### 7. terminal-image.ts - 终端图像

**功能：**
- Kitty IAL 和 iTerm2 协议支持
- 能力检测（Kitty, Ghostty, WezTerm, iTerm2, VSCode, Alacritty）
- 图像尺寸提取（PNG/JPEG/GIF/WebP）
- OSC 8 超链接

### 8. utils.ts - 文本工具

**主要函数：**
- `visibleWidth(str)` - 终端列宽计算（考虑 ANSI、宽字符、表情符号）
- `truncateToWidth()` - 按宽度截断文本
- `wrapTextWithAnsi()` - 保留 ANSI 样式的文本换行
- 使用 Intl.Segmenter 进行字素簇处理

### 9. autocomplete.ts - 自动补全

**功能：**
- 斜杠命令补全
- 文件路径补全（使用 fd 进行快速模糊搜索）
- 路径前缀解析（@、引号路径、~/ 展开）
- 引号处理和光标定位

### 10. keybindings.ts - 键盘绑定

**功能：**
- 用户绑定覆盖
- 冲突检测
- 编辑器、输入和选择操作的默认绑定

## 依赖项

**运行时依赖：**
- `chalk` ^5.5.0 - 终端样式
- `marked` ^15.0.12 - Markdown 解析
- `get-east-asian-width` ^1.3.0 - 东亚字符宽度
- `mime-types` ^3.0.1 - MIME 类型检测

**可选依赖：**
- `koffi` ^2.9.0 - Windows VT 输入支持

## 使用场景

该 TUI 框架适用于：
1. 交互式 CLI 应用程序
2. 终端聊天界面
3. 配置文件编辑器
4. 设置面板
5. 需要富文本渲染的终端工具

## 设计亮点

1. **模块化设计** - 清晰的组件接口，易于扩展
2. **性能优化** - 差分渲染减少不必要的重绘
3. **跨平台** - 支持 Windows、macOS、Linux
4. **协议兼容** - 支持 Kitty 和传统终端
5. **IME 支持** - 完整的输入法编辑器支持
6. **图像渲染** - 现代终端图像协议支持
