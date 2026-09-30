# 设计令牌速查表

> 来源：`apps/ui/src/styles/tokens.css`（语义令牌唯一来源）与
> `apps/web-app/src/app.css` 的 `@theme inline` 映射（Tailwind 类名入口）。
> 本文档只做速查，新增令牌时以源码为准并同步此处。
> 对应 `docs/ref/frontend-viz/06-可借鉴Dify与n8n的设计.md` §4.3 的建议。

## 颜色（HSL 三通道存储，用 `hsl(var(--x) / <alpha>)` 拼透明度）

| 令牌                                                   | 浅色值                      | 深色值                        | 用途                                      |
| ------------------------------------------------------ | --------------------------- | ----------------------------- | ----------------------------------------- |
| `--background` / `--foreground`                        | `0 0% 100%` / `220 20% 12%` | `220 16% 10%` / `210 12% 90%` | 页面底与正文                              |
| `--card` / `--card-foreground`                         | `0 0% 100%` / 同上          | `220 15% 12%` / 同上          | 卡片面                                    |
| `--popover` / `--popover-foreground`                   | 同 card                     | `220 16% 13%` / 同上          | 浮层（Dialog/菜单）                       |
| `--primary` / `--primary-foreground`                   | `220 20% 14%` / 白          | `210 12% 88%` / `220 20% 10%` | 主按钮                                    |
| `--secondary` / `--muted` / `--accent`                 | `214 20% 95/96/94%`         | `217 14% 18/17/20%`           | 次级面、 hover                            |
| `--destructive` / `--success` / `--warning` / `--info` | 红/绿/ amber /蓝            | 同色系提亮                    | 状态语义                                  |
| `--running` / `--running-foreground`                   | `252 56% 54%` / 白          | `252 60% 70%` / 深            | 运行态                                    |
| `--brand` / `--brand-foreground`                       | `24 95% 53%` / 白           | `24 90% 62%` / 深             | 品牌锚点（Logo、主 Run 按钮、主列表空态） |
| `--border` / `--input` / `--ring`                      | `214 16% 91/88%` / ring 深  | `217 13% 22/26%` / ring 亮    | 边框与焦点环                              |
| `--sidebar*`（4 个）                                   | 浅灰蓝组                    | 深灰蓝组                      | 侧栏三件套                                |
| `--chart-1..5`                                         | 蓝/绿/橙/紫/红              | 同色系提亮                    | 图表                                      |
| `--overlay`                                            | `220 20% 10% / 0.55`        | `0 0% 0% / 0.68`              | 遮罩                                      |
| `--scrollbar-thumb*`                                   | 灰                          | 深灰                          | 滚动条                                    |

## 圆角与字号

| 令牌                                            | 值                                                        | 说明                   |
| ----------------------------------------------- | --------------------------------------------------------- | ---------------------- |
| `--radius`                                      | `calc(0.5rem * var(--density-scale))`                     | 基准圆角，随密度档浮动 |
| `--text-heading/title/body/caption/small/micro` | `1.125/1/0.875/0.8125/0.75/0.6875rem × var(--font-scale)` | 六档字号，随密度档浮动 |

Tailwind 侧（`app.css @theme inline`）：`--radius-sm/md/lg/xl` 由 `--radius`
派生；`--color-*`、`--text-*`、`--shadow-*` 全量映射令牌；另有
`--spacing-sidebar: 15rem`、`--spacing-inspector: 22.5rem` 两个布局常量。

## 密度双变量

| 变量              | compact | default | comfortable | 作用               |
| ----------------- | ------- | ------- | ----------- | ------------------ |
| `--font-scale`    | 0.94    | 1       | 1.06        | 六档字号           |
| `--density-scale` | 0.92    | 1       | 1.08        | `--radius`（圆角） |

写入路径：首屏 `app.html` 内联恢复 → `routes/+layout.svelte` 响应式应用
（`applyFontScale` + `applyDensityScale`，`stores/theme.svelte.ts`），
源头是 `stores/preferences.svelte.ts` 的 `fontScale` / `spacingScale`，
持久化在 `localStorage wf-ui-preferences`。

## 用法示例

- Tailwind 类：`bg-card text-card-foreground border-border`、`text-running`、
  `bg-brand text-brand-foreground`（仅主 CTA/Logo）、
  `rounded-lg`（即 `--radius-lg`）。
- 透明度：`bg-destructive/10`、`border-destructive/25` 由映射自动支持。
- 状态色：优先 `statusTone()`（`@wf-agent/ui/status`）而非手写色值。
- 阴影：`shadow-frost`（固定铬）/`shadow-popover`（浮层）；毛玻璃用
  `frost` / `frost-panel` 工具。
