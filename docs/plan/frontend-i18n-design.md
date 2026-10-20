# 前端 i18n 引入设计方案

> 参考实现：`~/code/linkrs/frontend`（inlang Paraglide JS v2）。

## 一、目标

为 `apps/web-app` 引入类型安全的国际化能力，支持英文与中文两种界面语言，并保持以下约束：

- 翻译 key 拼写错误在编译期暴露（类型安全）；
- 未使用的翻译条目可被 tree-shake，不增大产物体积；
- 语言切换即时生效，本地持久化，首屏不闪烁；
- 与现有 `preferences.svelte.ts` 本地偏好体系（`wf-ui-preferences`）风格一致。

## 二、参考实现分析（linkrs）

linkrs 前端为 SvelteKit 项目，其 i18n 方案由四部分组成：

1. **消息源文件**：`messages/en.json`、`messages/zh.json`，扁平 key，按 `域.名称` 分组（如 `common.cancel`、`console.execStream`），支持 `{name}` 占位符插值。
2. **工程配置**：`project.inlang/settings.json` 声明 `baseLocale: en`、`locales: ["en", "zh"]` 与 message-format 插件的 `pathPattern`；`vite.config.ts` 中使用 `paraglideVitePlugin`，策略为 `['localStorage', 'preferredLanguage', 'baseLocale']`（纯客户端渲染，无服务端策略），产物输出到 `src/lib/paraglide` 并配置 `$paraglide` 路径别名。
3. **单一入口封装**：`src/lib/i18n/index.ts` 统一导出 `t(key, values?)`、`message(key)`、`getLocale`、`setLocale`、`SUPPORTED_LOCALES`、`Locale` 类型。组件只从这里导入，不直接接触生成的 `src/lib/paraglide` 目录，保证 key 类型定义集中、可替换底层实现。
4. **语言切换组件**：`LanguageSwitcher.svelte`，语言名用各自的母语硬编码（English / 中文，不翻译），点击 `setLocale(code)` 后刷新文档使全量消息生效。

## 三、本项目现状

- `apps/web-app` 同为 SvelteKit（`adapter-auto`，`src/routes` 页面路由，`src/lib` 逻辑层），与 linkrs 构建形态一致，Paraglide 可直接复用。
- 已有本地偏好模块 `src/lib/stores/preferences.svelte.ts`：theme / density 等在首绘前从 `localStorage` 恢复，语言偏好应并入同一轨道。
- `apps/web-app-preview` 是由 `scripts/sync-frontend-preview.sh` 生成的 fixture 镜像，i18n 资源需纳入同步范围。
- 全部文案目前为硬编码字符串，散布于约百余个 `.svelte` 组件与少量 `.ts` 文件，一次替换工作量较大，需分阶段推进。

## 四、选型结论

沿用 linkrs 的方案：**inlang Paraglide JS v2 + message-format 插件**。理由：

- 编译型生成强类型消息函数，key 拼错即编译错误，符合本项目的类型安全取向；
- Vite 插件集成简单，产物按需 tree-shake；
- 纯 key-value JSON，便于后续接入翻译流程与工具校验（en/zh key 集合一致性可写脚本比对）；
- 与 SvelteKit 2 / Svelte 5 无耦合，迁移风险低。

不采用的备选：`svelte-i18n`（运行时查找、无编译期类型检查）、`typesafe-i18n`（维护活跃度下降、模板语法偏重）。

## 五、落地方案

### 1. 新增文件与依赖

```
apps/web-app/
  project.inlang/settings.json      # locales: ["en","zh"], baseLocale: "en"
  messages/
    en.json                         # 扁平 key，域.名称 命名
    zh.json
  src/lib/i18n/index.ts             # 唯一导入入口：t / message / getLocale / setLocale / SUPPORTED_LOCALES
  src/lib/i18n/index.test.ts        # key 集合一致性、t() 行为测试
src/lib/paraglide/                  # 编译产物，加入 .gitignore（由 vite 插件生成）
```

依赖（workspace 集中管理原则适用于 Cargo；npm 侧直接加在 `apps/web-app/package.json`）：

- `@inlang/paraglide-js`（含 vite 插件）、`@inlang/plugin-message-format`（devDependencies）。

`vite.config.ts` 增加：

```ts
paraglideVitePlugin({
  project: './project.inlang',
  outdir: './src/lib/paraglide',
  strategy: ['localStorage', 'preferredLanguage', 'baseLocale'],
}),
```

并在 `svelte.config.js`（或 vite 配置的 alias）中加入 `$paraglide` 别名。

### 2. 入口封装（与 linkrs 同构）

`src/lib/i18n/index.ts`：

- `import { m } from '$paraglide/messages.js'`，导出 `MessageKey = keyof typeof m`；
- `t(key, values?)`：按当前 locale 渲染一条消息，供组件直接调用；
- `message(key)`：返回消息函数，供导航表、选项列表等数据驱动的调用点使用（本项目 `lib/config/navigation.ts`、`commands.ts`、`session-tabs.ts` 等配置表属于此类）；
- 转发 `getLocale` / `setLocale`，导出 `SUPPORTED_LOCALES` 与 `Locale` 类型。

语言切换落地为 `src/lib/components/layout/LanguageSwitcher.svelte`（端名硬编码 English / 中文），放置于 `TopBar`，并可暴露到 `ShortcutsSettings` / `HelpModal` 的偏好区域。

### 3. 语言偏好接入 preferences

- 将 `locale` 加入 `preferences.svelte.ts` 的首绘恢复轨道（`wf-ui-preferences` key），避免刷新闪烁；
- `setLocale` 调用点同步写回本地偏好；
- Paraglide 的 `localStorage` 策略与其并存即可：策略链中 `localStorage` 优先，偏好模块只作为恢复与展示的统一来源。

### 4. 文案替换分阶段推进

| 阶段 | 范围 | 说明 |
| --- | --- | --- |
| 1 | 基建落地 | 插件配置、`messages/*.json` 骨架（`common.*`、`app.*`）、`i18n/index.ts`、LanguageSwitcher |
| 2 | 布局层 | `components/layout/*`（TopBar、Sidebar、CommandPalette、PageState、Toaster 等） |
| 3 | 领域面板 | `components/domain/*`（workflow、execution、checkpoints、git、templates 等面板与卡片） |
| 4 | 会话与聊天 | `components/chat/*`、`components/pty/*` |
| 5 | 配置表与数据驱动文案 | `lib/config/*.ts` 经 `message(key)` 化，以及 services / stores 中的 toast、错误文案 |

每个阶段独立可合入，en 为基准语言先行提取，zh 随阶段补齐；缺失 key 自动回落 baseLocale（Paraglide 默认行为），不会阻塞渲染。

### 5. preview 镜像与校验

- `messages/`、`project.inlang/`、`src/lib/i18n/` 随 `scripts/sync-frontend-preview.sh` 同步；若脚本按白名单同步，需将上述路径加入；
- 新增脚本校验（可挂 `npm run check` 前置或独立 `lint:i18n`）：比对 `en.json` 与 `zh.json` 的 key 集合一致，报告多出/缺失项；
- 验证命令：`npm run check`（svelte-check 覆盖 key 类型）、`npm run test`、`vite build` 后抽查产物中未用 key 已被 tree-shake。

## 六、风险与注意

- `src/lib/paraglide` 为生成目录：必须加入 `.gitignore`，并确保 CI 在构建前已执行 `vite build`/`dev`（插件按需生成），避免直接引用缺失产物；测试环境（vitest）需在 vitest 配置中同样挂载 paraglide 插件或预生成。
- 本项目接口返回的错误信息来自后端（英文），属后端 i18n 范畴，本方案不覆盖；前端仅对自身文案负责。
- Markdown 渲染（`StreamMarkdown` 等）与用户产出内容不翻译，仅翻译界面 chrome 文案。
- 遵循仓库约定：代码与注释全英文；文档（含本文件）用中文。
