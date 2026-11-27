# CLI 的 RAG 命令面

> 范围：`crates/app/cli/wf-cli-shared` 的 RAG 参数、命令处理与命中渲染，以及 `wf-client` 生成客户端中的类型化方法
> 定位：全部命令都是无头管理命令，经由 HTTP 访问运行中的服务，不内嵌检索引擎

CLI 在 `wf rag` 命令组中暴露知识库管理能力，并把通用工具执行中的检索命中渲染成可读列表。命令定义、请求构造与渲染分别落在 `args_rag.rs`、`cmd/rag.rs` 和 `rag_render.rs`，保持参数、传输与呈现三层分离；请求构造全部委托生成客户端，命令层不含 URL 拼接。

## 1. 命令树

| 命令 | 作用 | 主要参数 |
|---|---|---|
| `wf rag stats` | 查看集合统计 | `--collection`、`--namespace` |
| `wf rag preview QUERY` | 预览检索命中 | `--top-k`、`--excerpt-chars`、范围参数 |
| `wf rag diagnose QUERY` | 输出空结果可能原因 | `--top-k`、`--filter-fields`、`--score-threshold`、可选范围 |
| `wf rag sources list` | 列举 source 状态 | 范围参数 |
| `wf rag sources add SOURCE URI KIND` | 读取并索引 source | `--converter`、`--options`、`--force`、范围参数 |
| `wf rag sources reindex SOURCE` | 按已保存的读取设置重建 source | `--collection`、`--namespace` |
| `wf rag sources delete SOURCE` | 删除 source | 范围参数 |

collection 与 namespace 默认取后端常用值 `kb`，调用时显式发给服务端，避免客户端缓存默认值造成范围歧义。`--options` 接收 JSON 对象字符串，省略时不下发；JSON 非法或不是对象时在参数层给出明确错误。

重建命令不接受 URI 与 KIND：服务端按三元组重放首次摄取时保存的读取器、转换器与读取选项，避免重建与首次摄取不一致。

## 2. 请求与响应

命令处理复用共享连接探测，再由 `WfClient` 的生成客户端发起类型化请求：

- 读取类命令调用 `handle_rag_*`，查询参数由生成客户端编码，解包统一信封后输出原始 DTO；
- 摄取使用 `RagSourceWrite`，重建与删除使用 `RagSourceRef` 三元组，二者都经 POST/DELETE 的类型化方法。

输出统一经过 `OutputEnvelope`：文本模式给出简短状态行，JSON 与按行 JSON 保留完整 DTO，静默模式只保留退出码。命令成功时 entity 字段记录 collection 或 source 标识，便于日志检索。

## 3. 命中渲染

`rag_render` 是文本模式下命中数组的共享格式化器，服务两个入口：

- `wf rag preview` 直接格式化预览命中；
- `wf tool execute rag_search` 在工具 id 以 `rag_` 开头时格式化工具结果。

格式化输出每行包含序号、分数、chunk 标识、source 和截断摘录；空数组输出明确的无命中提示。格式化只在文本模式启用，结构化格式仍保持单信封契约，避免破坏管道消费。

## 4. 错误与边界

- 服务不可用统一显示为服务端错误码与提示，不降级为空列表；
- 参数非法在发起请求前拒绝，特别是 source options 的 JSON 形态；
- 删除返回受影响数量，0 表示范围内没有该 source，不视为失败；
- CLI 不维护 source 状态缓存，每次列举都从服务端读取。

## 5. 文档与验证

命令帮助由 clap 文档注释提供；参数解析测试覆盖 `rag sources add` 与 `rag preview` 的必填项、默认值和 force 标志；命中格式化测试覆盖带分数命中、空数组与非数组输入。命令行为依赖的后端契约见 [../api/10-rag-domain.md](../api/10-rag-domain.md)，客户端同步细节见 [../client/03-rag-clients.md](../client/03-rag-clients.md)。
