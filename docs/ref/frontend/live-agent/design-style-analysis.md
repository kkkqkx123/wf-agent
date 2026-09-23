# 前端设计风格参考 — LiveAgent-ui 分析与建议

> 基于对 ref/LiveAgent-ui 的全面分析，提炼其设计语言体系，为当前项目 Web 前端提供设计风格建议。

---

## 一、LiveAgent-ui 设计风格概述

LiveAgent-ui 的整体设计风格可以概括为：**现代桌面级应用美学 + macOS 毛玻璃质感 + 高度精致的细节打磨**。它并非简单的"功能堆砌"，而是在视觉上追求专业、克制、细腻的体验。

### 1.1 风格关键词

| 关键词 | 说明 |
|--------|------|
| **Frosted Glass（毛玻璃）** | 大量使用 `backdrop-filter: blur()` + 半透明背景，模拟 macOS 毛玻璃效果 |
| **Minimal & Clean（极简洁净）** | 页面留白充足，信息密度控制得当，无冗余装饰 |
| **Subtle Animation（微妙动画）** | 所有交互都有克制的动效回应，统一使用 `cubic-bezier(0.16, 1, 0.3, 1)` 缓动曲线 |
| **Monochromatic Harmony（单色和谐）** | 色彩以 HSL 色相 220°（蓝灰）为主轴，明度和纯度变化产生层次 |
| **Frosted Glass Loading** | 加载态不是简单 spinner，而是精心设计的毛玻璃骨架 + 光泽流动动画 |

---

## 二、设计语言体系详解

### 2.1 色彩系统

#### 2.1.1 基础色板（HSL）

LiveAgent-ui 的色彩体系以 **HSL 220°（蓝灰色相）** 为核心主轴：

```
背景 (light)   : 0 0% 100%      (纯白)
背景 (dark)    : 224 22% 9%     (深灰蓝)
前景 (light)   : 222.2 84% 4.9% (近黑)
前景 (dark)    : 210 30% 96%    (近白)

次要色 (light) : 210 40% 96.1%  (浅灰)
次要色 (dark)  : 220 18% 17%    (深灰)

边框 (light)   : 214.3 31.8% 91.4%
边框 (dark)    : 220 14% 26%

静音前景 (light): 215.4 16.3% 46.9%
静音前景 (dark) : 215 18% 76%
```

**核心结论**：不依赖品牌色（无亮蓝/紫色主色调），而是用**中性蓝灰**贯穿始终，营造专业工具感。

#### 2.1.2 语义色彩

| 语义 | 浅色主题 | 深色主题 | 用途 |
|------|---------|---------|------|
| Success | 152 60% 42% | 152 56% 52% | 操作成功、文件变更新增 |
| Error | 0 72% 51% | 0 75% 65% | 错误状态、文件变更删除 |
| Running | 252 56% 57% | 252 60% 70% | 执行中状态 |
| Bash 工具 | 142 70% 45% | 142 60% 62% | Shell 工具调用标识色 |
| 文件工具 | 220 70% 55% | 220 70% 72% | 文件操作工具标识色 |
| 搜索工具 | 38 92% 50% | 38 90% 66% | 搜索工具标识色 |

#### 2.1.3 毛玻璃色彩变量

```css
/* 浅色主题 */
--tool-card-bg: 0 0% 100% / 0.72;       /* 半透明白 */
--tool-card-border: 0 0% 0% / 0.06;      /* 极淡黑边框 */
--checkpoint-bg: 0 0% 100% / 0.72;
--checkpoint-border: 0 0% 0% / 0.06;
--hub-canvas: var(--background);           /* Hub 页画布 */

/* 深色主题 */
--tool-card-bg: 0 0% 100% / 0.08;         /* 半透明白（深色） */
--tool-card-border: 0 0% 100% / 0.14;     /* 半透明白边框 */
--checkpoint-bg: 0 0% 100% / 0.08;
--checkpoint-border: 0 0% 100% / 0.14;
--hub-canvas: 224 24% 6.5%;               /* 更深画布 */
```

### 2.2 字体系统

```
应用字体 (--app-font-family):
  ui-sans-serif, system-ui, "PingFang SC", "Microsoft YaHei", sans-serif

对话字体 (--chat-font-family):
  "OpenAI Sans Semibold", "PingFang SC", "Microsoft YaHei", sans-serif

代码字体 (--code-font-family):
  "SF Mono", SFMono-Regular, Menlo, Monaco, "Cascadia Code",
  Consolas, "Liberation Mono", monospace
```

**关键特性**：
- 三套可独立配置的字体族（界面 / 对话 / 代码），通过 CSS 变量注入
- 支持**字号缩放**（`--zone-font-scale` 变量），对话区、侧边栏、工具面板可独立缩放
- 中文场景优先使用 `PingFang SC` / `Microsoft YaHei`，确保 CJK 渲染质量

### 2.3 圆角

| 层级 | 值 | 用途 |
|------|-----|------|
| 基础 | `0.5rem (8px)` | 按钮、输入框、卡片 |
| 中圆角 | `12px` | 确认对话框面板 |
| 大圆角 | `14px` | 毛玻璃英雄区、骨架屏 |
| 全圆角 | `9999px` | 徽标、圆形图标、滑块 |

### 2.4 阴影

```css
/* 对话框阴影 (浅色) */
shadow-2xl

/* 毛玻璃英雄区 (浅色) */
0 1px 0 hsl(0 0% 100% / 0.6) inset,
0 8px 28px -18px hsl(0 0% 0% / 0.18)

/* 毛玻璃英雄区 (深色) */
0 1px 0 hsl(0 0% 100% / 0.05) inset,
0 8px 28px -18px hsl(0 0% 0% / 0.6)
```

### 2.5 动画系统

#### 2.5.1 统一缓动曲线

整个应用几乎统一使用一个缓动曲线，形成强烈的一致性：

```css
cubic-bezier(0.16, 1, 0.3, 1)
```

这个曲线特性：**快速起始 + 弹性结束**，给人"果断、自然"的感受。

#### 2.5.2 动画类型清单

| 类别 | 动画 | 时长 | 用途 |
|------|------|------|------|
| 页面入场 | `hubPageIn` | 260ms | Hub 页面整体淡入上移 |
| 面板入场 | `hubPanelIn` | 360-420ms | 面板淡入上移+微缩放 |
| Chip 入场 | `hubChipIn` | 320ms | 标签/徽章逐个弹入，带弹性 |
| Card 入场 | `skillCardIn` | 350ms | Skill 卡片淡入上移 |
| 抽屉面板 | `skillsDrawerPanelIn` | 340ms | 右侧抽屉滑入 |
| 下拉菜单 | `editorContextMenuIn` | 150ms | 右键菜单微缩放淡入 |
| Git Tab | `gitReviewTabIn` | 220ms | Git 标签页淡入上移 |
| Git 面板 | `gitReviewPaneForward` | 200ms | Git 面板水平滑入 |
| Toast | `notifySlideIn` | 300ms | 通知右侧滑入 |
| 文件拖放 | `fileDropOverlayIn` | 160ms | 文件拖放覆盖层淡入 |

#### 2.5.3 Shimmer 动效

自有 `@utility shimmer` 系统，用于文字/品牌标识的流光效果：

```css
/* 使用方式 */
shimmer                    /* 基础流光文本 */
shimmer-once               /* 只播放一次 */
shimmer-duration-3000      /* 自定义时长 3000ms */
shimmer-color-blue-500     /* 自定义流光颜色 */
```

#### 2.5.4 无障碍降级

所有动画在 `prefers-reduced-motion: reduce` 下全部禁用，体现了高度的无障碍意识。

### 2.6 组件设计风格

#### 2.6.1 UI 组件库风格

组件基于 **shadcn/ui 模式** + **Base UI (Radix 替代品)**，但设计上更"桌面原生"：

| 组件 | 风格特点 |
|------|----------|
| **Button** | 5 种变体 (default/secondary/destructive/outline/ghost)，4 种尺寸 |
| **Input** | 极简边框，focus 时仅变化边框色，无 ring 动画 |
| **Select** | 无 focus ring，border 变色作为反馈 |
| **DropdownMenu** | 毛玻璃弹出层，带缩放+淡入动画，max-h 限制 66vh |
| **ConfirmDialog** | 半透明模糊 backdrop，磨砂面板，两次确认避免误操作 |
| **ScrollArea** | 自定义 6px 超薄滚动条，hover 时渐显 |

#### 2.6.2 聊天 UI 风格

**气泡设计**：
```css
/* 用户气泡 */
--chat-user-bg: 220 9% 91%       (浅色) / 220 14% 28% (深色)

/* 助手气泡 */
--chat-assistant-bg: 0 0% 100%   (浅色) / 224 22% 9% (深色)

/* 工具调用卡片 */
--tool-card-bg: 0 0% 100% / 0.72  (毛玻璃半透明)
--tool-card-border: 0 0% 0% / 0.06
```

**关键特征**：
- 用户气泡有淡淡的灰色背景区分
- 工具调用卡片使用**毛玻璃效果**，营造"工具层"的视觉层次
- 每个工具类型有独立标识色（bash=绿色，file=蓝色，search=橙色）
- 消息内 Markdown 渲染精细：标题、列表、代码块、表格、Mermaid 图均有逐一定义

#### 2.6.3 设置页风格

设置页采用**左导航 + 右内容**的两栏布局：

```
┌─────────┬──────────────────────────────────────┐
│  导航    │  内容区域                              │
│  System  │  ┌─── 设置项卡片 ──────────────────┐  │
│  Shortcuts│  │  标签        控件                │  │
│  Providers│  │  描述                           │  │
│  Agents   │  └─────────────────────────────────┘  │
│  ...     │                                      │
└─────────┴──────────────────────────────────────┘
```

**特征**：
- 导航图标 + 文字组合，选中态高亮
- 设置项以卡片/行形式组织，hover 时显示操作按钮
- 保存状态指示器（保存中/已保存/错误）在页面顶部显示
- 响应式：小屏幕时标题行变为纵向排列

### 2.7 毛玻璃质感详解

毛玻璃（Frosted Glass）是 LiveAgent-ui 最具辨识度的视觉特征，主要出现在：

| 区域 | 实现 | 效果 |
|------|------|------|
| Hub 加载页 | `.hub-frost-hero` | 半透明背景 + blur(24px) + saturate(180%) + 光泽流动动画 |
| Hub 骨架屏 | `.hub-frost-skeleton` | 半透明背景 + blur(18px) + saturate(170%) + 倾斜光泽扫描 |
| 工具调用卡片 | `--tool-card-bg` | 72% 透明度白色 + 6% 黑边框 |
| 检查点卡片 | `--checkpoint-bg` | 72% 透明度白色 + 6% 黑边框 |
| 模态框背景 | `AlertDialog.Backdrop` | 55% 黑色 + blur-sm |

```css
/* 毛玻璃核心模式 */
.element {
  background: hsl(var(--background) / 0.82);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  backdrop-filter: blur(24px) saturate(180%);
  box-shadow:
    0 1px 0 hsl(0 0% 100% / 0.6) inset,   /* 顶部高光 */
    0 8px 28px -18px hsl(0 0% 0% / 0.18);  /* 底部阴影 */
}
```

### 2.8 Hub 页面风格

Hub 页（MCP Hub、Skills Hub）是一个独立的设计子系统：

**布局**：
```
┌─── Hub 页面 ──────────────────────────────┐
│  Header (标题 + 操作按钮)                   │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐    │
│  │ Card 1   │  │ Card 2  │  │ Card 3  │    │
│  └─────────┘  └─────────┘  └─────────┘    │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐    │
│  │ Card 4   │  │ Card 5  │  │         │    │
│  └─────────┘  └─────────┘  └─────────┘    │
└──────────────────────────────────────────────┘
```

**特征**：
- 毛玻璃加载英雄区（Hub Frost Hero）
- 卡片网格布局，带逐项延迟入场动画
- Chip 标签逐个弹入（阶跃延迟）
- 整体页面淡入上移
- 深色主题时画布更深（`--hub-canvas: 224 24% 6.5%`），卡片更通透

### 2.9 滚动条设计

整个应用对滚动条进行了精细的定制，不同区域有不同的策略：

| 区域 | 风格 | 说明 |
|------|------|------|
| 全局 | 6px 超薄半透明 | 始终显示，hover 加深 |
| Git 审查 | 浮动滚动条 | 默认透明，hover 显示 |
| 工具文本 | 薄滚动条 | 初始透明，hover 渐显 |
| 上传文件列表 | 极薄 4px | hover 显示，离焦隐藏 |
| 项目工具标签栏 | 隐藏原生显示自定义 | 极简风格 |
| 终端 | 8px 宽 | 半透明，hover 加深 |

---

## 三、对当前项目 Web 前端的风格建议

### 3.1 总体定位

当前项目（Modular Agent Framework）的 Web 前端，与 LiveAgent-ui 在**用户群体**（开发者）、**核心功能**（Agent/工作流管理）上高度重合，因此建议**继承其设计精神**，但做 Web 端适配：

| 继承 | 调整 |
|------|------|
| ✅ 极简克制的色彩体系 | 🔄 毛玻璃质感选择性使用（Web 端性能敏感） |
| ✅ 统一缓动曲线动画 | 🔄 降低动画复杂度（Web 端非原生环境） |
| ✅ 精细的字体系统 | 🔄 去除桌面端特殊字体依赖 |
| ✅ 一致的基础组件风格 | 🔄 使用 shadcn-svelte 替代 shadcn/react |
| ✅ 深色/浅色主题双支持 | 🔄 增加系统主题自动跟随 |

### 3.2 色彩体系建议

基于 LiveAgent-ui 的色彩体系，为 Web 前端建议：

```css
/* 建议的基础色板 */
:root {
  /* 主色轴：从 220° 调整到更适合 Web 的 210°-220° */
  --background: 0 0% 100%;
  --foreground: 210 20% 12%;
  --muted: 210 20% 96%;
  --muted-foreground: 210 10% 50%;
  --border: 210 15% 92%;
  --card: 0 0% 100%;
  --primary: 210 20% 12%;
  --primary-foreground: 0 0% 100%;
  --accent: 210 20% 96%;
  --secondary: 210 20% 92%;
}

.dark {
  --background: 220 15% 10%;
  --foreground: 210 10% 90%;
  --muted: 215 15% 18%;
  --muted-foreground: 210 10% 60%;
  --border: 215 12% 22%;
  --card: 220 14% 12%;
  --primary: 210 10% 90%;
  --primary-foreground: 210 20% 12%;
  --accent: 215 15% 20%;
  --secondary: 215 12% 22%;
}
```

**说明**：
- 保持 LiveAgent-ui 的**蓝灰中性主轴**，不引入品牌亮色
- 减少 HSL 色相纯度，更适合 Web 长时间阅读
- 深色主题从 `224 22% 9%` 微调到 `220 15% 10%`，更柔和

### 3.3 字体系统建议

```css
/* 去掉 OpenAI Sans 等桌面特有字体 */
--app-font-family: 
  "Inter", ui-sans-serif, system-ui, 
  "PingFang SC", "Microsoft YaHei", 
  sans-serif;
  
--code-font-family:
  "JetBrains Mono", "Fira Code", "SF Mono", 
  Consolas, monospace;
```

**说明**：
- 引入 `Inter` 作为西文字体（Google Fonts 可加载，适合 Web）
- 保留 `JetBrains Mono` / `Fira Code` 作为代码字体（Web 场景常用）
- 保留中文 fallback 链 `PingFang SC` → `Microsoft YaHei`
- 保留 `--zone-font-scale` 缩放机制

### 3.4 动画系统建议

#### 3.4.1 保留的动画模式

| 动画 | 建议 | 说明 |
|------|------|------|
| 统一缓动 `0.16, 1, 0.3, 1` | ✅ 保留 | 形成品牌一致性 |
| 面板入场淡入上移 | ✅ 保留 | 给页面呼吸感 |
| Card 逐项入场 | ✅ 保留 | 适合网格列表页 |
| 右侧抽屉滑入 | ✅ 保留 | 适合详情面板 |
| 下拉菜单缩放淡入 | ✅ 保留 | 标准交互模式 |
| 通知滑入 | ✅ 保留 | Toast 通知 |

#### 3.4.2 简化/移除的动画

| 动画 | 建议 | 原因 |
|------|------|------|
| 毛玻璃光泽流动 | 🔄 简化 | Web 端 backdrop-filter 性能开销大 |
| Shimmer 文字流光 | 🔄 可选 | 仅用于品牌展示场景 |
| 12 芒星 spinner | 🔄 替换 | 改用更轻量的 CSS spinner |

#### 3.4.3 新增的 Web 适配动画

| 动画 | 说明 |
|------|------|
| 页面路由过渡 | SvelteKit 页面间过渡动画 |
| 数据加载骨架屏 | 列表/卡片加载时的脉冲骨架 |
| WebSocket 重连提示 | 连接状态变化的视觉反馈 |
| 侧边栏收起/展开 | 导航栏的滑入滑出 |

### 3.5 毛玻璃质感使用策略

在 Web 前端中，**不能无节制使用毛玻璃**，建议：

| 使用场景 | 建议 | 理由 |
|----------|------|------|
| 模态框背景 | ✅ 保留 | 性能消耗小，视觉提升大 |
| 导航栏/顶栏 | ✅ 适度使用 | 固定元素，blur 范围有限 |
| Hub 页面加载屏 | ⚠️ 简化 | 使用纯 CSS 模拟光泽感 |
| 工具调用卡片 | 🔄 改用浅色卡片 | Web 端避免大量 blur 区域 |
| 列表项 | ❌ 不使用 | 性能开销大，收益低 |

**Web 端毛玻璃改良方案**：

```css
/* 轻量毛玻璃（Web 优化版） */
.web-frost {
  background: hsl(var(--background) / 0.85);
  backdrop-filter: blur(8px);  /* 降低 blur 值 */
  /* 移除 saturate，Web 端不必要 */
  border: 1px solid hsl(var(--border) / 0.6);
  box-shadow: 0 1px 3px hsl(0 0% 0% / 0.06);
}
```

### 3.6 组件库策略

#### 3.6.1 建议使用 shadcn-svelte

LiveAgent-ui 使用 **shadcn/ui (React)** + **Base UI**，Web 前端建议使用 **shadcn-svelte** 作为基础组件库：

| shadcn-svelte 组件 | 对应 LiveAgent-ui 组件 | 覆盖度 |
|-------------------|----------------------|--------|
| Button | ✅ button.tsx | 完全覆盖 |
| Input | ✅ input.tsx | 完全覆盖 |
| Select | ✅ select.tsx | 完全覆盖 |
| Dialog | ✅ confirm-dialog.tsx | 完全覆盖 |
| DropdownMenu | ✅ dropdown-menu.tsx | 完全覆盖 |
| ScrollArea | ✅ scroll-area.tsx | 完全覆盖 |
| Label | ✅ label.tsx | 完全覆盖 |
| Textarea | ✅ textarea.tsx | 完全覆盖 |
| Card | — | 需要新增（风格参考 tool-card） |
| Tabs | — | 需要新增（设置页导航） |
| Switch | — | 需要新增（开关控件） |
| Separator | — | 需要新增 |
| Sheet | — | 需要新增（抽屉面板） |
| Command | — | 需要新增（命令面板） |

#### 3.6.2 需要自建的业务组件

参考 LiveAgent-ui 的业务组件，Web 端需要自建：

| 组件 | 参考 LiveAgent-ui | 说明 |
|------|------------------|------|
| ChatTranscript | `pages/chat/transcript/` | 对话转录列表 |
| ChatBubble | `AssistantBubble.tsx` | 对话气泡 |
| ChatComposer | `ChatComposerBar.tsx` | 输入组合器 |
| ToolCallCard | `ToolCallItem.tsx` | 工具调用展示卡片 |
| WorkflowGraph | — | 工作流图谱可视化 |
| DiffView | `DiffView.tsx` | Git Diff 对比视图 |
| Timeline | — | 执行时间线 |
| StatusBadge | `StatusText.tsx` | 状态标识徽标 |

### 3.7 布局框架建议

参考 LiveAgent-ui 的布局，Web 前端建议的**响应式布局**：

```
┌─────────┬────────────────────────────┬──────────┐
│ 侧边栏   │    主内容区                  │ 属性面板  │
│ (可折叠) │                            │ (可折叠)  │
│ ─────── │                            │ ─────── │
│ 导航菜单  │   ┌──────────────────────┐ │ 详情/   │
│ 工作流   │   │   页面内容              │ │ 配置    │
│ 线程     │   │                       │ │        │
│ Agent   │   │                       │ │        │
│ 资源     │   └──────────────────────┘ │        │
│ 设置     │                            │        │
└─────────┴────────────────────────────┴──────────┘
   240px          1fr                      360px
```

**响应式断点**：

| 断点 | 布局调整 |
|------|----------|
| > 1280px | 三栏全开 |
| 1024-1280px | 右侧面板转为浮层/抽屉 |
| 768-1024px | 侧边栏折叠为图标栏 |
| < 768px | 单栏，导航和面板为浮层 |

### 3.8 加载状态设计

参考 LiveAgent-ui 的 Hub Frost 骨架屏，Web 前端建议：

#### 3.8.1 页面级加载

```css
/* 页面级脉冲骨架 */
.page-skeleton {
  background: linear-gradient(
    90deg,
    hsl(var(--muted) / 0.5) 25%,
    hsl(var(--muted) / 0.8) 50%,
    hsl(var(--muted) / 0.5) 75%
  );
  background-size: 200% 100%;
  animation: shimmer 2s ease-in-out infinite;
  border-radius: 8px;
}
```

#### 3.8.2 列表级加载

```
┌─────────────────────────────┐
│ [████████████░░░░░░░░░░░░░] │  <-- 渐变进度条
└─────────────────────────────┘

┌─── 脉冲骨架 ─────────────────┐
│ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓              │  ← 标题占位
│ ▓▓▓▓▓▓  ▓▓▓▓▓▓▓▓▓           │  ← 两行描述
│                              │
│ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓              │
│ ▓▓▓▓▓▓  ▓▓▓▓▓▓▓▓▓           │
└────────────────────────────────┘
```

每个骨架项有逐项延迟（延迟 80ms 递增），形成从左到右的"波次"效果。

### 3.9 主题切换策略

LiveAgent-ui 支持系统主题自动跟随 + 手动切换（light/dark/system），Web 端建议：

```typescript
// theme store
type Theme = 'light' | 'dark' | 'system';

const theme = writable<Theme>('system');

// 自动应用
$: if ($theme === 'system') {
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  document.documentElement.classList.toggle('dark', mq.matches);
}
```

额外增加 **CSS 过渡动画**，主题切换时有平滑过渡：

```css
/* 主题切换过渡 */
* {
  transition: background-color 0.3s ease, border-color 0.3s ease, color 0.3s ease;
}
```

### 3.10 移动端适配建议

LiveAgent-ui 是纯桌面应用，Web 前端需要额外考虑移动端：

| 适配项 | 策略 |
|--------|------|
| 导航 | 底部 Tab bar 替代左侧导航 |
| 对话 | 全屏聊天界面，底部固定输入 |
| 工作流编辑 | 最低适配平板（768px+），手机仅查看 |
| 面板 | 底部 Sheet 替代右侧面板 |
| 触摸手势 | 增加滑动返回、长按操作菜单 |
| PWA | 支持安装到主屏幕 |

---

## 四、设计风格规范总结

### 4.1 设计 Token 建议

```css
/* 建议的 CSS 自定义属性体系 */

/* 颜色 */
--color-background
--color-foreground
--color-muted
--color-muted-foreground
--color-primary
--color-primary-foreground
--color-secondary
--color-secondary-foreground
--color-accent
--color-accent-foreground
--color-destructive
--color-destructive-foreground
--color-border
--color-input
--color-ring
--color-success      /* 新增：操作成功 */
--color-warning       /* 新增：操作警告 */
--color-info          /* 新增：信息提示 */

/* 字体 */
--font-sans
--font-mono
--font-chat

/* 字号缩放 */
--zone-font-scale

/* 圆角 */
--radius-sm: 0.375rem
--radius-md: 0.5rem
--radius-lg: 0.75rem
--radius-xl: 1rem

/* 动画 */
--ease-out-expo: cubic-bezier(0.16, 1, 0.3, 1)
--ease-in-out: cubic-bezier(0.4, 0, 0.2, 1)
--duration-fast: 150ms
--duration-normal: 250ms
--duration-slow: 350ms

/* 阴影 */
--shadow-sm
--shadow-md
--shadow-lg
--shadow-xl
--shadow-frost
```

### 4.2 风格检查清单

实现 Web 前端时，对照以下清单确保设计一致性：

- [ ] 色彩是否沿用了蓝灰中性主轴？
- [ ] 圆角是否统一使用了 0.375-1rem 的范围？
- [ ] 动画是否统一使用了 `cubic-bezier(0.16, 1, 0.3, 1)`？
- [ ] 深色/浅色主题是否都有完整的色彩映射？
- [ ] 滚动条是否经过了定制？
- [ ] 毛玻璃效果是否只在关键区域使用？
- [ ] 加载态是否使用了骨架屏或过渡动画？
- [ ] `prefers-reduced-motion` 是否被尊重？
- [ ] 字号缩放机制是否可用？
- [ ] 响应式布局是否覆盖了所有断点？

---

## 五、总结

LiveAgent-ui 的设计风格可以归纳为 **"安静而精致"（Quietly Polished）** —— 它不靠亮色或华丽装饰吸引眼球，而是通过细腻的毛玻璃质感、克制的动效、精心调校的间距和色彩，营造出一种专业工具应有的"质感"。

对于当前项目的 Web 前端，建议：

1. **继承设计精神**：极简、专业、精致，不引入不必要的装饰
2. **适配 Web 环境**：毛玻璃适度使用，动画降低复杂度，增加响应式支持
3. **使用 shadcn-svelte**：快速获得与 LiveAgent-ui 一致的组件基础
4. **关注性能**：Web 端不是原生环境，`backdrop-filter` 和动画需要权衡
5. **保持品牌一致性**：建立完整的设计 Token 系统，确保多页面视觉统一
