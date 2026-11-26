# Server HTTP API 清单

> 范围：`crates/app/wf-server` HTTP 传输层暴露的全部端点。
> 来源：`crates/app/wf-server/openapi.json`（OpenAPI 3.0 快照，658 个操作 / 572 个路径）。
> 基础前缀：业务端点挂载在 `/api/v1` 下；`/`、`/health`、`/metrics`、`/api/v1/ws`、`/api/v1/info` 等系统端点按各自路径。
> 版本：`v1`（`API_VERSION`）。

## 通用约定

- **鉴权**：除公开健康检查外，所有端点接受 `x-api-key` 请求头或 `api_key` 查询参数；WebSocket 通过 `x-api-key` 头或 `api_key` 查询参数。
- **信封**：JSON 响应统一为 `{ "success": bool, "data": ..., "error": { "code", "message" } | null }`。
- **错误码映射**：NotFound/ExecutionNotFound → 404 `NOT_FOUND`；Validation → 400 `INVALID_PARAMS`；AlreadyExists → 409 `ALREADY_EXISTS`；Conflict → 409 `CONFLICT`；Timeout → 504 `TIMEOUT`；Storage → 500 `STORAGE_ERROR`；Execution → 500 `INTERNAL_ERROR`；未授权 401/无权限 403/限流 429（带 `Retry-After`）/SSE 限额 503。
- **SSE**：5 个端点返回 `text/event-stream`（`/events/stream`、`/workflows/{id}/execute/stream`、`/executions/{id}/error-analysis/stream`、`/agent-loops/{id}/stream`、`/llm/generate-stream`）。
- **WebSocket**：`GET /api/v1/ws`，支持 subscribe/unsubscribe/ping。
- **目录作用域**：请求可通过 `x-wf-directory` 头或 `directory` 查询参数指定目录；无默认根目录。

## 文档索引

| 文件 | 域 | 操作数 |
|------|----|--------|
| [02-system-and-health.md](02-system-and-health.md) | system | 58 |
| [03-workflow.md](03-workflow.md) | workflow | 132 |
| [04-agent.md](04-agent.md) | agent | 121 |
| [05-checkpoint-and-file.md](05-checkpoint-and-file.md) | checkpoint | 56 |
| [06-template.md](06-template.md) | template | 58 |
| [07-llm.md](07-llm.md) | llm | 57 |
| [08-observation-query-analysis.md](08-observation-query-analysis.md) | observation | 51 |
| [09-entity.md](09-entity.md) | entity | 45 |
| [10-git.md](10-git.md) | git | 19 |
| [11-chat.md](11-chat.md) | chat | 14 |
| [12-web.md](12-web.md) | web | 13 |
| [13-fs.md](13-fs.md) | fs | 12 |
| [14-trigger.md](14-trigger.md) | trigger | 11 |
| [15-pty.md](15-pty.md) | pty | 7 |
| [16-rag.md](16-rag.md) | rag | 7 |
| [17-cli.md](17-cli.md) | cli | 1 |

## 生成说明

本目录由 `crates/app/wf-server/openapi.json` 自动生成；如端点变更，请重新生成。
