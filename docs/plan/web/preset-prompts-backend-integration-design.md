# 预设提示词后端化与配置模块设计

## 1. 背景与术语澄清

首页 Composer 当前使用前端硬编码的建议提示词（`ChatComposerBar.svelte` 中的
`SUGGESTION_KEYS` 经 `t()` 解析为三个 i18n 文案）。这套文本与后端能力脱节，
无法自定义、无法运营。

但在动手之前必须先澄清三个极易混淆的概念，本设计后续所有结论都建立在这组区分上：

- **Starter（会话起始建议 / 预设提示词）**：填充的是**用户消息正文**
 （`OutgoingMessage.text`），即用户点一下 chip，草稿箱里出现一段话，
  再由用户发送。当前 `SUGGESTION_KEYS` 属于这一类。
- **System Prompt Template（系统提示词模板）**：`wf-resource` 注册表中的
  `Template`（`system.default` / `system.code` / `system.agent` 等），控制的是
  **Agent 的系统提示词**，经 `system_prompt` /
  `system_prompt_template_id` 参与 `agent_prompt` 组装（含 `{{fragments}}`
  伪变量渲染）。用户在输入框里看不到它。
- **Library Template（模板库模板）**：节点 / 触发器 / Agent / 工作流四种可复用
  定义，走 `/api/v1/templates/library` 等通道，受精选 / 热门 / 使用计数 /
  克隆治理。

原版设计把第一类直接等同于第二类（“预设提示词可直接作为
`system_prompt_template_id` 的候选”），这是类型错误：前者是 user 文本种子，
后者是 system 文本模板，两者正交，不可互换。本设计纠正这一点。

## 2. 原版设计的问题

1. **类型混淆**：Starter 填充用户输入，`system_prompt_template_id` 决定系统
   行为。把 Starter 候选塞进 `system_prompt_template_id` 下拉，既不能工作
   （语义不对），也会误导 Agent / Workflow 配置页。
2. **`category = "preset"` 复用 `Template` 不可行**：`wf-types::Template`
   的 `category` 白名单（`TEMPLATE_CATEGORIES`）表示**渲染用途**
  （`system` / `rules` / `user-command` / `tools` / `composite` /
   `fragments` / `dynamic` / `user` / `assistant` / `tool-visibility`），
   没有 `preset`，校验在 `wf-config::processor::prompt` 中执行。
   `category` 也不是“是否预设”的维度——真正的区分维度是**来源与治理**
  （内置只读 baseline vs 用户自定义），对应既有 `is_system` /
   `enabled` 模式。硬塞一个新类别会污染校验、嵌入资产
  （`configs/prompts.json`）、自定义加载器与注册表。
3. **API 与现有约定冲突**：原版提出 `GET /api/v1/prompts/presets` 全套 CRUD
   加 `POST .../enable|disable` 单端点加 `POST .../usage`。问题：
   - 路径落在 `prompts` 命名空间，易与系统提示词模板混淆；Starter 是聊天
     交互域资源，应归属 chat 域。
   - 启停合并为一个端点不符合现有惯例（skills / tools 均为
     `POST /{id}/enable` 与 `POST /{id}/disable` 分开）。
   - 缺少分页包络（`PageView` + `limit` / `offset`）、`IdPath` 提取、
     `ok` / `error_response` 包络、utoipa 注解与
     `routes_match_utoipa_annotations` 对等测试、以及 `schema.d.ts` 再生成。
   - 系统提示词模板本身尚无 REST CRUD，却先为 Starter 建全套 CRUD，
     治理层级倒置；Starter 的 `variables` / `fragments` 在 v1 并不需要，
     强行复用只会增加渲染失败面。
4. **前端低估**：原版“移除硬编码、按需显示 chips”遗漏了：
   locale（后端 Starter 是何语言、切换语言是否重拉）、preview 模式
  （`client.preview.ts` + fixtures，无后端时 chips 从哪来）、空会话才显示
   （`showSuggestions = bubbles.length === 0`）、草稿键隔离、queue / busy
   下的点击语义、`ModelPicker` 类比不当（`ModelPicker` 是下拉选择器，
   Starter 是 chips 行，两者交互完全不同）。
5. **配置模块错位**：资源页（模型 / 工具 / 脚本 / 技能）与模板页
   （节点 / 触发器 / Agent / 工作流）已有明确分工，Starter 两边都不属于；
   “Agent / Workflow 配置可引用预设 id”延续了第 1 条的类型错误，不应实施。

## 3. 修改方向（结论）

1. **Starter 作为独立的聊天域资源，不复用 `Template`**：v1 字段只保留
   聊天种子真正需要的部分，不带 `variables` / `fragments` / 渲染管线。
   与系统提示词模板彻底解耦，`system_prompt_template_id` 的候选集不变。
2. **路径归属 chat 域**：`GET /api/v1/chat/starters` 系列，而不是
   `/api/v1/prompts/presets`。理由：生命周期是聊天交互（空态建议、点击填充、
   发送计数），不是模板治理；避免与 `Template` 命名空间碰撞。
3. **治理沿用模板库既有模式**：内置只读 baseline（`is_system = true`，
   不可改删）+ 用户自定义（CRUD + 启用停用 + 导入导出），`usage` 只在
   **发送成功后**记录，点击填充不计数。
4. **前端分阶段**：第一阶段只做聊天空态 chips 的后端化（含加载 / 空 / 错误
   三态与 i18n 回退）；管理 UI（启用停用、编辑、导入导出）作为第二阶段，
   落在设置页独立节或模板库新增分页，不进资源页，不碰 Agent / Workflow
   配置引用。

## 4. 后端资源模型

### 4.1 Starter 定义（v1）

```text
id          string   全局唯一，内置以 `starter.` 开头
title       string   chip 上显示的短标题（多 locale 下按请求 locale 返回）
content     string   点击后填入草稿箱的正文（纯文本，可含换行）
description string?  悬停提示，不上 chip
locale      string   `en` / `zh` 等，列表接口按请求 locale 过滤并回退
tags        string[] 分类展示与搜索，可空
enabled     bool     禁用即不在聊天接口返回
is_default  bool     是否纳入默认推荐集（空态显示的子集）
is_system   bool     内置只读标记，前端据此禁用编辑与删除入口
usage_count u64      发送成功累计
created_at / updated_at int64
```

刻意**没有** `variables` / `fragments` / `content` 渲染：v1 的 `content`
是纯文本，前端原样填入，不经过 `render_template` 管线。需要变量插值是
后续增强，不阻塞后端化。

### 4.2 内置 baseline

- 内置 Starter 随服务端静态配置下发（与 `configs/prompts.json` 互相独立，
  不混入 `prompt_templates()` 资产），服务启动时注册为只读。
- 当前前端三个 i18n 建议（总结 / 工具 / 计划）迁移为默认内置 Starter 的
  初始内容，但**语言不再写死在前端**：后端按 `locale` 存多份，前端请求时
  带当前语言，后端回退策略为 `请求语言 → en → 全部`。
- 自定义 Starter 存入存储层（与节点 / 触发器模板同等持久化语义），
  重启不丢失；内置与自定义 `id` 冲突时自定义胜出并打标（沿用
  `skip_if_exists` 注册语义的反向：启动期内置先装，用户数据覆盖）。

### 4.3 与现有能力的关系

- 与 `Template` / `SystemPromptFragment` / `FragmentCompositionConfig`
  **无关联**：不共享注册表，不共享校验，不共享渲染。
- 与 `system_prompt` / `system_prompt_template_id` **不互通**：Agent 运行链
  （`agent_prompt.rs` 的 inline 优先、模板渲染、skill / tool 元数据注入）
  保持现状；Starter 永远不出现在 `system_prompt_template_id` 候选里。
- 与模板库**治理模式对齐但不合表**：复用 `is_system` / `enabled` /
  `usage_count` / 克隆导入导出的交互心智，但独立存储、独立路由，
  不污染 `kind = workflow | agent | node | trigger` 四元组。

## 5. API 设计（修订）

基路径 ` /api/v1/chat/starters`，全部走标准包络（`ok` / `error_response` /
`ok_page`）、`IdPath` 提取、utoipa 注解，并补齐
`routes_match_utoipa_annotations` 对等覆盖与 `schema.d.ts` 再生成。

- `GET /api/v1/chat/starters?locale=&q=&tags=&enabled=&limit=&offset=`：
  聊天空态使用的列表。返回分页 `PageView<StarterSummary>`，
  默认只返回 `enabled = true` 且匹配请求 locale 的条目。
  `q` 做标题与内容子串匹配，`tags` 逗号分隔。
- `GET /api/v1/chat/starters/{id}`：详情（含完整 `content`）。
- `POST /api/v1/chat/starters`：创建自定义 Starter（`is_system` 恒为 false）。
- `PUT /api/v1/chat/starters/{id}`：更新自定义 Starter；内置返回 404
 （与节点 / 触发器模板“内置只读”语义一致）。
- `DELETE /api/v1/chat/starters/{id}`：删除自定义 Starter；内置返回 404。
- `POST /api/v1/chat/starters/{id}/enable` 与
  `POST /api/v1/chat/starters/{id}/disable`：分开的两个端点，
  与 skills / tools 一致，不合并为 `enable|disable`。
- `POST /api/v1/chat/starters/{id}/usage`：发送成功后计数，与模板库
  `POST /templates/library/{id}/usage` 语义一致。点击填充不调用。
- `POST /api/v1/chat/starters/import`（JSON 体）与
  `GET /api/v1/chat/starters/{id}/export`（`?download=`）：复用节点 /
  触发器模板的导入导出模式，前端自行组装下载。

读写接口在同一模块一次交付（读接口先被聊天空态消费，写接口随后的
管理 UI 直接复用），避免分两次改动存储模型与快照结构。

## 6. 前端集成

### 6.1 数据流

新增 `apps/web-app/src/lib/services/starters.ts`：

- `listStarters({ locale, q, tags })` 走 `client.GET('/api/v1/chat/starters')`，
  经 `call` / `extractPage` / `requireData` 拆包（沿用 `resources.ts` /
  `template-library.ts` 的 DTO 收敛模式，后端 `schema.d.ts` 多为 unknown
  时在本地收敛 interface）。
- `recordStarterUsage(id)` 在 `ChatSessionController.sendText` 返回 true
 （即消息已受理）后触发，fire-and-forget，失败不打扰主流程。
- locale 来源为当前 `paraglide` 语言，语言切换时重拉。

### 6.2 Composer 与 ChatComposerBar 改动

- `ChatComposerBar.svelte` 删除 `SUGGESTION_KEYS` 常量，改为挂载时拉取
  starters：`suggestions` 从 `string[]` 改为
  `{ id, title, content, description? }[]`（`Composer` 的 `suggestions` prop
  同步改类型，或在 Bar 层做薄映射，保持 `Composer` 只管渲染 chips）。
- `Composer.svelte` 的 chips 行位置与样式不变（输入框上方、
  `rounded-full border` 胶囊），只换数据源。点击语义保持**替换当前草稿**
  （`draft = starter.content`）并聚焦，与现行为一致；草稿非空时同样替换，
  不做追加拼接——追加会产生难以预料的混合文本，且与队列语义冲突。
  需要“追加而非覆盖”是后续增强，需另行设计分隔符与光标策略，v1 不做。
- `showSuggestions` 条件不变：仅空会话（`bubbles.length === 0`）显示；
  有消息、queue 非空、busy 状态下均不显示，避免打断正在进行的回路。

### 6.3 修改后的前端效果（验收口径）

1. **正常态**：新建会话（或无消息会话）进入时，输入框上方出现一排
   胶囊 chips，文案来自后端（中英文随系统语言切换而变化）。点击任一 chip，
   文本填入输入框（替换草稿），光标聚焦，用户可继续编辑后发送。
   发送成功后该 Starter 的 `usage_count` 加一（前端无感知）。
2. **加载态**：首屏拉取期间不展示骨架、不占位，直接不渲染 chips 行，
   输入框位置保持不动，避免布局跳动；数据到达后 chips 行淡入。
3. **空态**：后端返回空列表（全部禁用或无匹配 locale）时，chips 行整体隐藏，
   效果等同于今天 `suggestions.length === 0` 的情况，输入框上移贴边，
   无残留标题、无“暂无建议”占位。
4. **错误态**：请求失败（断网、鉴权、服务端错误，含尚无该端点的旧服务端）
   时回退到内置 i18n 三条文案，保证与今天一致可用，不弹 toast、不阻断输入；
   下次语言切换时静默重试一次。连续失败不做指数退避之外的额外 UI。
5. **回退态**：`vite --mode preview`（fixture 模式）与离线演示下，
   由 `client.preview.ts` + fixtures 提供三条与现 i18n 文案等价的 mock
   Starter，保证无后端时页面与今天一致。
6. **国际化**：中英文切换后 chips 文案即时更换（重拉对应 locale），
   草稿箱已有内容不受影响；后端缺对应 locale 时回退显示英文，
   不出现空白 chip。
7. **管理入口（第二阶段）**：第一阶段不在任何页面新增管理 UI；
   内置 Starter 不可编辑、不可删除（前端据 `is_system` 禁用按钮）。
   第二阶段在模板库页新增 Starter 分页（与节点 / 触发器 / Agent /
   Workflow 并列，不进资源页），提供搜索、语言筛选、启用停用、编辑、
   导入导出；Agent / Workflow 配置页**不出现** Starter 引用入口。

### 6.4 i18n 与测试

- 现有 `chat.chatComposerBar.suggestSummary / suggestTools / suggestPlan`
  三个 key 在第一阶段**保留**，仅用作 preview fixtures 与后端不可达时的
  回退文案；待管理 UI 上线且内置 Starter 稳定后，再移除并清理相关断言。
- 更新依赖硬编码建议的单测与截图断言，改为 mock `listStarters`；
  新增用例覆盖：空列表隐藏、请求失败隐藏、locale 切换重拉、
  发送成功才记 usage（点击不记）。

## 7. 配置模块（分阶段）

- **第一阶段（本设计必含）**：无管理 UI。后端提供列表 + 详情 + usage，
  前端只消费。设置页不新增开关，避免“默认预设选择与自动应用”这类
  尚未定义语义的选项（自动应用是指进会话自动填充？还是自动发送？
  两者体验差异极大，v1 不引入）。
- **第二阶段（管理 UI）**：模板库页新增 Starter 分页，与节点 / 触发器 /
  Agent / Workflow 并列为同级 Tab，自带搜索框、语言筛选与新建 / 导入入口；
  行内提供启用开关、编辑（仅自定义）与二次确认删除、单条导出。不进资源页
  （模型 / 工具 / 脚本 / 技能）以免职责混乱，也不进设置页——Starter 的治理
  形态（浏览、启用停用、导入导出、使用计数）与模板库一致，直接复用该页的
  卡片、对话框与反馈模式。删除仅限自定义。
- **明确不做**：Agent / Workflow 配置引用 Starter id；Starter 绑定
  agent 模板 / workflow / 模型 profile 的关联建模；权限与多用户隔离
  （沿用现有单用户存储语义，留待服务端多租户方案统一解决）。

## 8. 迁移步骤

1. 后端新增 Starter 存储模型与 `/chat/starters` 全套端点（列表 + 详情 +
   创建 / 更新 / 删除 / 启停 / usage / 导入导出，写接口与读接口同一模块
   一次交付），配套 utoipa 注解、包络、分页、对等测试；再生成 `schema.d.ts`。
2. 前端新增 `services/starters.ts`，`ChatComposerBar` 改为异步数据源，
   `Composer` 的 `suggestions` 改类型；preview mock 与 fixtures 补齐；
   i18n key 转为回退文案；更新受影响单测。
3. 灰度验证：空列表隐藏、错误回退、locale 切换、usage 只在发送后计数、
   preview 模式一致性。
4. 第二阶段交付模板库 Starter 分页管理 UI（搜索、语言筛选、启用停用、
   编辑、导入导出、二次确认删除）；移除前端常量与回退 key 的最终清理
   放在该阶段末尾，不放在第一阶段。

## 9. 风险与非目标

- 本设计第一阶段不含管理 UI、不含变量插值、不含片段组装；
  任何要求 Starter 支持 `{{variable}}` / `{{fragments}}` 的需求都进入
  后续增强，且届时应重新评估是否值得并入 `Template` 渲染管线，
  而不是在 v1 预埋。
- 内置 Starter 的只读语义与版本兼容由后端保证：内置 id 稳定、
  删除内置返回 404、升级时新增内置不覆盖用户同名自定义。
- 自定义 Starter 的权限与多用户隔离留待后续设计，v1 沿用现有存储语义。
- 不引入“自动应用预设”开关：语义未定（填充 vs 发送），误触代价高，
  必须经独立交互设计后再议。
