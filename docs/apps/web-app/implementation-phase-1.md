# Web App 实施方案 - 第一阶段

## 阶段目标

搭建前端应用壳，打通与 `wf-server` 的类型安全 REST 通路，建立样式、状态、加载态与质量门禁的基础体系，交付一个可运行的只读页面闭环。

## 一、范围与前提

### 1.1 前提（已具备）

- `apps/web-app` 为 SvelteKit 2 + Svelte 5 空壳，`check` / `typecheck` / `lint` / `test` 脚本可用。
- OpenAPI 快照 `apps/web-app/openapi.json` 与生成类型 `src/lib/api/schema.d.ts` 已提交；codegen 管线（`tools/openapi-codegen`）可用。
- 后端路由、统一分页包络、鉴权、静态托管、服务端缺口项均已落地（`docs/plan/web/server-gaps.md`）。

### 1.2 本阶段不包含

- 任何后端 / Node 中间层开发。
- 流式（SSE/WS）接入（阶段三）。
- 完整页面域铺开（阶段二起）。

## 二、基础工程

- 固化开发期接入方式：Vite 代理 `/api` 或显式 `VITE_API_BASE`，**二选一并写入集成文档**，保持 `baseUrl` 默认 `/api/v1` 语义单一。
- 样式底座：引入 TailwindCSS，按 `docs/spec/web/style-guide.md` 建立 Token 分层（底色、文字、边框、强调、域语义、形状阴影）与浅色/深色/跟随系统三模，首屏预注水防闪烁。
- 适配器目标：`@sveltejs/adapter-static`（`fallback: index.html`），与 `wf-server --static-dir` 同源托管对齐；若暂用 `adapter-auto`，在本阶段切换。
- 质量门禁接入 CI 或本地约定：`npm run check`、`npm run typecheck`、`npm run lint`、`npm run test` 全绿为合入条件。

## 三、API 客户端层

按 `docs/plan/web-app-integration.md` 第 2、3 节落地 `src/lib/api/`：

1. `client.ts`：`openapi-fetch` 以 `schema.d.ts` 的 `paths` 为唯一类型源；默认 `baseUrl = /api/v1`。
2. 鉴权注入：`onRequest` 挂 `x-api-key`（来源按集成方案第 5 节：生产同源不碰密钥，开发本地 `.env.local`）。
3. 信封拆包：`call()` 处理 `success=false` 与非 2xx，映射 `ErrorResponse.error.code` 为可分支的前端错误；`callPage()` 对应 `PageView{items, limit, offset, has_more}` 游标模型。
4. 错误归一：至少覆盖 `NOT_FOUND`、`INVALID_PARAMS`、`UNAUTHORIZED`、`FORBIDDEN`、`RATE_LIMITED`（429 读 `Retry-After`）。
5. 禁止手写响应模型：`data` 精度不足时按集成方案阶段推进替换，不双源定义。

## 四、应用壳与导航

- 三栏 Shell：左侧栏（可折叠为图标、悬停展开、尺寸记忆持久化）、中间主列（居中限宽）、右侧检查器（可调、记忆；窄屏先折叠检查器再折叠侧栏，移动端转抽屉）。
- 一级导航按 `docs/apps/web-app/web-app-feature-list.md` 第 2 节十个页面建立路由骨架（本阶段多数页面可为占位，但导航与面包屑完整）。
- 默认落地页为执行工作台。
- 布局与密度、窄屏阈值遵循样式规范第 4 节。

## 五、状态与加载态体系

- 状态分层：组件内状态 → 跨组件 store → 装配层组装；组件不直接持有流读取器（阶段三起生效，本阶段定型目录与约定）。
- 本地偏好（侧栏尺寸、主题、字号）走统一偏好通道封装；服务端 `/preferences`、`/favorites` 已存在，属阶段四接线，本阶段仅预留接口位置。
- 加载态四件套按样式规范第 6 节实现：骨架（波纹方向一致、无布局跳动）、流式占位（阶段三用）、空态（插画 + 一句话 + 主操作）、错误态（错误码 + 可读文案 + 重试）。
- 危险操作二次确认的通用弹窗组件在本阶段具备（阶段二起复用）。

## 六、打通的示范页面

选一个**只读列表页 + 详情页**（建议：执行列表 → 执行详情），验收以下链路全部类型化：

- 列表：游标翻页、过滤参数、`items`/`has_more` 消费、空态与错误态。
- 详情：按 id 拉取、404 分支、加载骨架。
- 全程无 `any` 泄漏、无手写响应接口。

## 七、任务清单

| # | 任务 | 产出 |
|---|---|---|
| 1 | 固化 dev 接入方式（代理或显式 base，二选一） | 配置 + 集成文档更新 |
| 2 | 引入 Tailwind 与 Token 三模主题 + 预注水 | 样式底座 |
| 3 | 切换 adapter-static（或确认等价静态产物路径） | 构建产物可被 `--static-dir` 托管 |
| 4 | `client.ts`：openapi-fetch + 鉴权 + `call`/`callPage` | `src/lib/api/client.ts` |
| 5 | 错误归一与错误码分支 | 错误类型与展示约定 |
| 6 | 三栏 Shell + 十页导航骨架 + 面包屑 | 装配层布局 |
| 7 | 原子组件最小集：按钮、输入、表格（含空态骨架）、弹窗、Toast | 原子层 |
| 8 | 示范列表页 + 详情页打通 | 端到端只读闭环 |
| 9 | check/typecheck/lint/test 门禁全绿 | 质量基线 |

## 八、验收标准

- `npm run check`、`npm run typecheck`、`npm run lint`、`npm run test` 全部通过。
- 示范页参数、`data`、`error` 全类型化，仓库内无重复声明的响应模型。
- 生产构建产物可由 `wf-server --static-dir` 同源托管并正确 SPA 回退（`/api/*` 未知路径保持 404）。
- 主题三模切换无首屏闪烁；骨架/空/错误三态在示范页可见。
- 侧栏折叠与尺寸记忆在刷新后保持。

## 九、风险与应对

- **类型精度停在 `unknown`**：示范页范围内按集成方案推进具体 DTO 替换，不整体阻塞。
- **dev 两套接入并存**：本阶段强制二选一固化，后续阶段不得再引入第二种方式。
- **Tailwind 与既有占位页冲突**：Token 先行，页面逐步迁移，禁止页面内硬编码色值（样式规范第 7 节）。
