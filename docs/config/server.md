# 服务配置（server/）

服务传输与中间件配置位于 `{layer}/server/`，共四个文件。由 `wf-server` 文件层加载，优先级：环境变量 > 项目 `.wf/server/` > settings `server/` > 内置默认。

| 文件 | 作用 |
|---|---|
| `server.toml` | 监听地址 |
| `auth.toml` | API Key 鉴权中间件 |
| `cors.toml` | CORS 中间件 |
| `rate-limit.toml` | 按 IP 滑动窗口限流 |

## server.toml

- `bind_addr`：监听地址，默认 `127.0.0.1:3000`
- 环境覆盖：`WF_SERVER_BIND_ADDR`（冻结旧名也接受 `--addr` flag）

## auth.toml

- `enabled`：是否启用 API Key 鉴权
- `header_name`：请求头名（默认 `x-api-key`）
- `allow_query_param`：是否允许 query 参数传 key
- `query_param_name`：query 参数名（默认 `api_key`）
- `excluded_paths`：免鉴权路径（默认健康探针）
- 密钥只来自环境变量 `API_KEYS`（逗号分隔），从不写入文件
- 环境覆盖：`AUTH_ENABLED`、`API_KEYS`

## cors.toml

- `allowed_origins`：允许来源（默认 `["*"]`）
- `allowed_methods`：允许方法
- `allowed_headers`：允许请求头
- 无环境变量（来源是部署数据，不是密钥）

## rate-limit.toml

- `enabled`：是否启用限流
- `window_ms`：滑动窗口（默认 60000）
- `max_requests`：窗口内最大请求数（默认 100）
- `excluded_paths`：免限流路径
- 环境覆盖：`RATE_LIMIT_ENABLED`、`RATE_LIMIT_WINDOW_MS`、`RATE_LIMIT_MAX_REQUESTS`
