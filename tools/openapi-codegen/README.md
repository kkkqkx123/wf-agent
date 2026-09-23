# openapi-codegen

从 `wf-server` 的 OpenAPI 快照离线生成 TypeScript 类型的独立工具包。  
**不进入 `apps` workspace**，与 web-app 依赖树隔离。

## 为何独立

- `openapi-typescript` 的 peer 为 `typescript ^5.x`，在 TypeScript 7 下运行时失败。
- web-app 使用 TS7（`svelte-check`）；本包锁定 TS5，仅在需要重新生成时安装/运行。
- 生成物在本目录为中间产物，**不进 git**；复制到 web-app 后的 `schema.d.ts` 才是正式文件。

## 依赖

| 包 | 版本 |
|----|------|
| `openapi-typescript` | `^7.4.4` |
| `typescript` | `^5.9.3`（满足 peer） |

Node.js `>=22`。

## 使用

```bash
# 1. 若注解/文档结构有变更，先刷新快照（在仓库根目录）
WF_REFRESH_OPENAPI=1 cargo test -p wf-server committed_snapshot_matches_document

# 2. 安装（仅首次或依赖变更时）
cd tools/openapi-codegen
npm install

# 3. 从快照生成（读 web-app 快照，写本目录 schema.d.ts）
npm run gen

# 4. 复制为正式类型文件
cp schema.d.ts ../../apps/web-app/src/lib/api/schema.d.ts
```

- **输入**：`../../apps/web-app/openapi.json`（仓库提交的 golden-file 快照）
- **输出**：本目录 `schema.d.ts`（已被 `.gitignore` 忽略）
- **正式产物**：`apps/web-app/src/lib/api/schema.d.ts`（提交入库）

## 日常校验（不重新生成时）

```bash
cargo test -p wf-server committed_snapshot_matches_document
```

快照漂移会使测试失败；需要更新类型时再走上面 1→4。

## 约定

- 本目录生成的 `openapi.json`、`schema.d.ts` 等中间文件不提交。
- 不在 web-app 内声明 `openapi-typescript` / `gen` 脚本，避免双源。
- 流式（SSE/WS）类型不由本工具生成，在 web-app 侧独立维护。
