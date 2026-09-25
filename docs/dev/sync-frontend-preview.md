# web-app → web-app-preview 同步机制
 
## 概述
 
`sync-web-app-preview.sh` 脚本将 `apps/web-app` 的源码完整同步到 `apps/web-app-preview`，确保预览项目始终与主项目路由、组件、store、服务层保持一致。Preview 端只需替换一个文件——`src/lib/api/client.ts`——把真实的 openapi-fetch 客户端换成基于 fixtures 的 mock 实现，即可在无后端的情况下完整跑通所有页面。
 
这种 "全量镜像 + 单文件覆盖" 的设计避免了两套代码分叉带来的维护负担：主项目只维护一份 UI/业务逻辑，预览项目只需维护 fixtures 和 mock client，其他一切由同步脚本保证一致。
 
## 位置
 
```
scripts/sync-web-app-preview.sh
```
 
## 使用方法
 
### 从项目根目录执行
 
```bash
./scripts/sync-web-app-preview.sh
```
 
脚本支持从任意目录执行，内部会自动定位仓库根目录并解析 `apps/web-app`（源）和 `apps/web-app-preview`（目标）。
 
## 同步内容
 
脚本使用 rsync 镜像整个源树，并通过 `--exclude` 保护 preview 专属文件不被覆盖，同时在同步后用 jq 合并 `package.json`。
 
| 路径模式 | 来源 | 说明 |
|---------|------|------|
| 顶层配置 | `apps/web-app/` → `apps/web-app-preview/` | `.gitignore`、`.prettierrc*`、`eslint.config.js`、`svelte.config.js`、`tsconfig.json`、`tsconfig.test.json`、`vite.config.ts`、`vitest.config.ts` |
| SvelteKit 入口 | `src/app.html`、`src/app.css` | HTML 模板 + Tailwind 全局样式 |
| API 模块 | `src/lib/api/envelope.ts` | 信封拆包逻辑（`schema.d.ts` 不再同步，见下方排除项） |
| 组件 | `src/lib/components/**/*` | `domain` / `layout` / `ui` / `icons` 全部 |
| 配置 | `src/lib/config/**/*` | 导航配置等 |
| 服务层 | `src/lib/services/**/*` | 所有业务服务（调用 client.ts） |
| Store | `src/lib/stores/**/*` | `theme`、`toast`、`preferences` 等 |
| 类型 | `src/lib/types/**/*` | 模型类型定义 |
| 工具 | `src/lib/utils/**/*` | `cn`、`format`、`route`、`status` |
| 路由 | `src/routes/**/*` | 所有页面和布局 |
 
### package.json 合并策略
 
package.json 不参与 rsync 镜像，而是由独立的 jq 合并步骤构建：
 
| 字段 | 合并规则 |
|------|---------|
| `name` / `version` / `description` | **保留 preview 身份**（首次运行时若不存在，脚本会初始化 `@wf-agent/web-app-preview` / `0.1.0`） |
| `scripts` / `engines` | 跟随 web-app（新增脚本自动同步） |
| `devDependencies` / `dependencies` | 完整采用 web-app 的（新增依赖自动同步） |
| `type` / `main` | 跟随 web-app |
| `keywords` / `author` / `license` | 优先 preview 自身，preview 未定义时回退到 web-app |
 
### Preview 专属文件（不被覆盖）
 
rsync 通过 `--exclude` 保护以下文件：
 
| 路径 | 用途 |
|------|------|
| `src/lib/api/client.ts` | **fixture-backed 客户端**，替代 openapi-fetch。首次同步时从源项目拷贝一次作为起点，后续同步脚本会跳过它 |
| `src/lib/fixtures/**/*` | 所有本地样例数据（clock、workflows、executions、agentLoops、checkpoints、resources、insights、triggers） |
| `.env` | preview 端环境变量覆盖（例如 `VITE_BACKEND_TARGET` 指向 preview 不需要的代理） |
 
额外排除项（防止构建产物和编辑器垃圾被同步）：`node_modules/`、`dist/`、`build/`、`.svelte-kit/`、`.env*`、`.DS_Store`、`.vscode/`、`.idea/`。

**为控制 git 历史体积而主动跳过的生成物**：`openapi.json` 与 `*.d.ts`（即 `src/lib/api/schema.d.ts`）。这两个文件是由后端契约生成的大体积产物，且其导出的类型仅用于类型标注、在构建时会被擦除，因此不同步到 preview 项目，避免每次契约更新都在 preview 的 git 历史里堆积大块 diff。preview 运行只依赖同步过来的运行时源码与 fixtures。
 
## 前置条件
 
| 工具 | 用途 | 安装 |
|------|------|------|
| `rsync` | 增量文件镜像（比手写 cp 更健壮，支持 `--delete`） | 多数系统自带；`apt install rsync` / `brew install rsync` |
| `jq` | package.json 依赖合并 | `apt install jq` / `brew install jq` |
 
## 工作流程
 
```
apps/web-app (主项目)          apps/web-app-preview (预览)
├── package.json               ├── package.json (name 不同)
├── openapi.json               ├── (不同步：生成物，控制 git 体积)
├── src/app.html               ├── src/app.html ← 同步
├── src/app.css                ├── src/app.css ← 同步
├── src/lib/api/               ├── src/lib/api/
│   ├── client.ts  ← 真实 API  │   ├── client.ts ← mock (保留)
│   ├── envelope.ts            │   ├── envelope.ts ← 同步
│   └── schema.d.ts            │   └── (不同步：生成物，控制 git 体积)
├── src/lib/components/**/*    ├── src/lib/components/**/* ← 同步
├── src/lib/services/**/*      ├── src/lib/services/**/* ← 同步
├── src/lib/stores/**/*        ├── src/lib/stores/**/* ← 同步
├── src/lib/types/**/*         ├── src/lib/types/**/* ← 同步
├── src/lib/utils/**/*         ├── src/lib/utils/**/* ← 同步
├── src/routes/**/*            ├── src/routes/**/* ← 同步
└── (无 fixtures)             └── src/lib/fixtures/**/* ← 保留
```
 
### 日常使用
 
当 `apps/web-app` 新增或修改组件、路由、服务后：
 
1. 运行同步脚本：
```bash
./scripts/sync-web-app-preview.sh
```
 
2. 如果新增了数据字段（`src/lib/types/models.ts` 有变更），手动更新 preview 端的 fixtures：
```
apps/web-app-preview/src/lib/fixtures/*.ts
```
 
3. 如果 `openapi.json` 有新增端点且需要在 preview 中展示：
   - 在对应 fixture 文件里补充样例数据
   - 确保 services 里的 DTO 转换逻辑能适配新字段
 
4. 安装/更新依赖：
```bash
cd apps/web-app-preview && npm install
```
 
5. 启动预览服务验证：
```bash
npm run dev
```
 
### 关于 mock client 的说明
 
主项目的 services 层统一调用 `client.GET('/api/v1/...', ...)`，其中 `client` 来自 `$lib/api/client`。Preview 端只需让这个 `client` 返回符合 openapi-fetch 响应形状（`{ data, error, response }`）的对象，整个服务层无需任何改动。
 
**首次运行同步**时，preview 端没有自己的 client.ts，脚本会从 web-app 拷贝一份真实版本过去。此后 preview 开发者应把它替换为 fixture-backed 实现（一个返回 fixtures 数据的 shim）。脚本检测到 preview 已有 client.ts 后，后续同步会自动跳过它。
 
fixture-backed client 的骨架大致如下：
 
```typescript
// apps/web-app-preview/src/lib/api/client.ts
import { fixtures } from '$lib/fixtures/...';
 
const routeMap = new Map<string, unknown>([
['/api/v1/workflows', { items: fixtures.workflows, has_more: false, limit: 50, offset: 0 }],
['/api/v1/workflows/wf-release-train', { id: 'wf-release-train', /* ... */ }],
// ... 每个端点对应一个 fixture
]);
 
async function fake<T>(path: string): Promise<{ data?: T }> {
const found = routeMap.get(path);
return found !== undefined ? { data: found as T } : {};
}
 
export const client = {
GET: async <T>(path: string) => fake<T>(path),
POST: async <T>(path: string) => fake<T>(path),
// services 只用了 GET，但完整 mock 通常也提供 PUT/DELETE
};
```
 
`envelope.ts` 里的 `call()` 和 `extractPage()` 会自动拆解 `data` 并从 fixture 结果里抽取分页字段。
 
## 删除行为
 
脚本使用 `rsync --delete`，意味着**主项目里已删除的文件会被同步删除**（除了 `--exclude` 保护的 preview 专属文件）。这避免了 preview 因遗留文件与主项目产生偏离。
 
## 自动化（可选）
 
如需在 CI 中确保 preview 始终与主项目同步，可以在工作流里加上：
 
```yaml
# .github/workflows/sync-preview.yml
jobs:
sync-preview:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install dependencies
        run: sudo apt-get install -y rsync jq
      - name: Sync web-app-preview
        run: ./scripts/sync-web-app-preview.sh
      - name: Check for uncommitted changes
        run: git diff --exit-code apps/web-app-preview/
```
sync-web-app-preview.sh
/workspace/scripts
+191
-0
