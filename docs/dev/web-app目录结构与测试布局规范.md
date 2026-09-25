# web-app 目录结构与测试布局规范

适用范围：`apps/web-app`（SvelteKit + TypeScript + Vitest）。本文固化现有已验证合理的设计，并约定后续新增测试（含集成测试）的放置规则，作为后续开发的参考基线。

## 1. 总体目录结构

```
apps/web-app/
├── src/
│   ├── app.html / app.css        # SvelteKit 入口与全局样式
│   ├── routes/                   # 文件式路由，只放路由文件（+page / +layout / +server 等）
│   └── lib/                      # 可复用代码，全部逻辑层在此
│       ├── api/                  # HTTP/SSE 客户端、OpenAPI schema、envelope 解析
│       ├── components/           # 组件按职责分组：chat / domain / icons / layout / ui
│       ├── config/               # 静态配置与命令定义
│       ├── services/             # 跨 store 的业务服务（如流式处理）
│       ├── stores/               # Svelte 5 runes store（*.svelte.ts）
│       ├── types/                # 共享类型
│       └── utils/                # 纯函数工具
├── tests/                        # 预留：集成测试目录（见第 3 节）
└── *.config.*                    # vite / vitest / svelte / eslint / tsconfig
```

分层原则：`routes` 只做路由编排，从 `lib` 导入；`lib` 内部依赖方向为 `routes → services/stores → api/utils/config → types`，不允许反向依赖。

## 2. 单元测试：共置（co-location）

单元测试采用与源码同目录的 `*.test.ts` 命名，**不使用** `__tests__/` 子目录。

理由：
- `__tests__/` 是 Jest 时代约定；Vitest 官方与 Svelte 社区主流做法是共置，vitest 默认 include 模式同样支持。
- 共置使 import 路径最短（`./route` 而非 `../route`），重命名/移动文件时测试自然同步，防止测试与源码脱节。
- 关键是一致性：当前全部单测均共置，无混用，维持现状。

现状示例（均满足就近原则）：

| 测试文件 | 被测源码 |
|---|---|
| `src/lib/utils/route.test.ts` | `route.ts` |
| `src/lib/utils/mentions.test.ts` | `mentions.ts` |
| `src/lib/utils/attachments.test.ts` | `attachments.ts` |
| `src/lib/stores/collection.test.ts` | `collection.svelte.ts` |
| `src/lib/services/streaming.test.ts` | 同目录服务 |
| `src/lib/config/commands.test.ts` | 同目录配置 |

### SvelteKit 特有硬约束

**禁止在 `src/routes/` 下放置任何 `.test.ts` / `.spec.ts` 文件。** SvelteKit 会把 routes 下的文件解析为路由，测试文件会凭空产生如 `/xxx.test` 的幻影路由。若某段逻辑需要路由级测试支持，应下沉到 `src/lib` 后共置测试。

## 3. 集成测试放置规则（后续新增时遵循）

集成测试（跨模块协作、依赖 mock server/多 store 联动、渲染完整组件树等）与单元测试分离，放在应用根目录的 `tests/` 下：

```
apps/web-app/tests/
├── api/            # 依赖 mock HTTP/SSE 服务的 client + store 联动
├── flows/          # 跨 services/stores 的业务流程（如 chat 流式全链路）
└── components/     # 组件树级渲染测试（@testing-library/svelte）
```

选择依据：
- `vite.config.ts`（已合并 Vitest 配置，无独立 `vitest.config.ts`）的 include 覆盖 `src` 与 `tests`，无需改配置即可被同一 runner 拾取；单测与集成测试通过目录边界自然区分，便于 `vitest run src` / `vitest run tests` 分别筛选。
- 与 Rust 侧约定对齐：本仓库各 crate 的集成测试统一放 `tests/`（见根 `AGENTS.md`），前端沿用同一心智模型。
- `tests/` 在 `src/` 之外，天然不受 SvelteKit 路由解析影响，也不会被 `vite build` 打进产物（build 只追踪从入口可达的 import）。
- 不使用模板残留的 `test/`（单数）别名，统一为 `tests/`；include 模式与 tsconfig 的 exclude 均已按此收窄。

边界判断：只验证单一函数/store 的行为 → 共置单测；需要装配两个及以上模块、或需要 DOM/网络桩 → `tests/` 集成测试。

### E2E（若未来引入 Playwright 等浏览器测试）

单独放 `e2e/` 目录，使用独立 runner 配置，与 `tests/` 的 Vitest 集成测试区分，避免 `.spec.ts` 命名被 vitest include 误拾取。

## 4. 配套约定

- 测试文件命名：`*.test.ts`（Vitest 惯例），与被测文件同名对应。
- 类型检查：`tsconfig.test.json` 覆盖测试代码，`pnpm typecheck` 同时检查源码与测试。
- 运行：`pnpm test`（一次性）/ `pnpm test:watch`。
- 每个测试目录内可按需再加子目录，但保持"目录名描述被测领域"而非"描述测试层级"（如 `tests/flows/`，而非 `tests/integration/unit/`）。
