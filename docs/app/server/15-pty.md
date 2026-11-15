# pty 域 HTTP 端点
> 操作数：7
| Method | Path | Handler (operationId) | Parameters / Body |
|--------|------|------------------------|-------------------|
| GET | `/api/v1/pty` | `get_pty` | persistent_only (q) |
| POST | `/api/v1/pty` | `post_pty` | body |
| DELETE | `/api/v1/pty/{id}` | `delete_pty` | id (p) |
| GET | `/api/v1/pty/{id}` | `get_pty_id` | id (p) |
| PUT | `/api/v1/pty/{id}` | `put_pty_id` | id (p), body |
| GET | `/api/v1/pty/{id}/connect` | `get_pty_connect` | id (p), cursor (q), ticket (q) |
| POST | `/api/v1/pty/{id}/connect-token` | `post_pty_connect_token` | id (p) |
