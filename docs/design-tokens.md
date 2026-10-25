# 设计令牌速查表

本文面向 Web、TUI、VS Code 等多端的界面实现，说明当前可用的语义令牌与密度档位。目标是让各端在不复制样式细节的前提下，对齐颜色、字号、圆角与间距。

## 颜色语义

- 背景与前景：`background`、`foreground` 用于页面底色与正文。
- 卡片与浮层：`card`、`popover` 用于内容卡与浮动面板，浮层可叠加 `shadow-popover`。
- 主次操作：`primary` 用于默认强调按钮，`secondary`、`accent` 用于弱强调与悬停。
- 状态色：`success`、`warning`、`destructive`、`info`、`running` 分别表达成功、警告、危险、信息与运行中。画布中的执行色统一经由展示模型的正规化函数映射，不直接写死色值。
- 品牌色：`brand` 仅用于品牌锚点，例如主 Run 按钮、空状态图标与关键入口，不用于大面积背景。
- 边界与输入：`border`、`input`、`ring` 用于分割线、输入边框与焦点环。
- 侧栏：`sidebar` 系列用于导航底色与激活态。
- 图表：`chart-1` 至 `chart-5` 用于指标与分析图表。
- 终端：`ansi-*` 系列用于渲染 shell 输出，亮色与暗色两套。
- 代码：`code-keyword`、`code-string`、`code-comment`、`code-number`、`code-function`、`code-type` 用于语法高亮。

明暗两套主题分别在 `:root` 与 `.dark` 下定义，token 均以 HSL 通道存储，便于叠加透明度。

## 字号与密度

- 字号档：`text-heading`、`text-title`、`text-body`、`text-caption`、`text-small`、`text-micro`，均随 `font-scale` 缩放。
- 密度档：`compact`、`default`、`comfortable` 三档，分别调整 `font-scale` 与 `density-scale`（圆角与间距）。偏好保存在本地并在首屏前应用，避免闪烁。
- 画布字号：`canvas-node-font-size`、`canvas-edge-font-size` 随界面字号流动，缩放补偿由 `canvas-zoom-compensation` 在运行时写入。
- 内容宽度：`content-max-width` 约束列表页居中宽度，画布路由保持全幅。

## 圆角与阴影

- 圆角：`radius-sm`、`radius-md`、`radius-lg`、`radius-xl` 由基础 `radius` 与密度派生。
- 阴影：`shadow-frost` 用于固定铬与毛玻璃，`shadow-popover` 用于浮层，`surface-float` 组合了浮层背景、边框与阴影。

## 使用原则

- 优先使用语义 token，禁止在组件内写死色值。
- 品牌色只做锚点，不做状态色替代。
- 新增多端复用组件时，先查本表，再考虑新增 token；新增 token 必须同时给出明暗两套值。
- 画布相关的颜色必须经由展示模型的正规化入口，保持服务端渲染与测试的回退一致。
