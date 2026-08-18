# CLI Executor 输出管理 — 文件重定向设计

## 背景

当前 `crates/wf-tools/src/executor/cli.rs` 的 `CliExecutor::execute` 使用内存截断：

```rust
fn truncate_lines(output: &[u8], max_lines: usize) -> String {
    // 超出 max_lines 时丢弃后续行，追加 "... (truncated N lines)"
}
```

问题：

1. **截断后原始输出丢失**：LLM 想看截断后的后续内容只能重跑命令
2. **不可追溯**：相同命令在不同上下文的输出无法对比审计
3. **无法按需检索**：LLM 无法对全部输出做 `grep`/`head`/`tail`

## 方案：文件重定向 + 硬上限

```
命令执行
  │
  ├─ stdout ──→ [tokio::process::ChildStdout]
  │                  │
  │                  ├─→ 截断版（返回给调用方）
  │                  │
  │                  └─→ .wf/tmp/outputs/{session_id}/{call_id}-stdout.txt
  │                        （完整输出，直到硬上限）
  │
  └─ stderr ──→ [tokio::process::ChildStderr]
                     │
                     ├─→ 截断版（返回给调用方）
                     │
                     └─→ .wf/tmp/outputs/{session_id}/{call_id}-stderr.txt
```

### 核心变更

#### CliExecutionOptions 扩展

```rust
pub struct CliExecutionOptions {
    pub args: Vec<String>,
    pub max_lines: Option<usize>,          // 行数截断（默认 1000）
    pub max_output_bytes: Option<u64>,     // 硬上限字节数（默认 20MB）
    pub output_dir: Option<PathBuf>,       // .wf/tmp/outputs/ 路径
    pub call_id: Option<String>,           // 本次调用的唯一 id
    pub timeout_ms: Option<u64>,
    pub cwd: Option<String>,
    pub env: Option<Vec<(String, String)>>,
}
```

#### CliExecutionResult 扩展

```rust
pub struct CliExecutionResult {
    pub stdout: String,             // 截断后的文本
    pub stderr: String,             // 截断后的文本
    pub exit_code: i32,
    pub success: bool,
    pub stdout_path: Option<PathBuf>,  // 完整输出文件路径（若存在）
    pub stderr_path: Option<PathBuf>,
    pub truncated: bool,               // 是否有截断发生
    pub truncated_reason: Option<String>,  // "lines" / "size" / "size+lines"
    pub total_lines: Option<usize>,    // 原始总行数
    pub total_bytes: Option<u64>,      // 原始总字节数
}
```

### 输出策略矩阵

| 输出大小 | 返回文本 | 文件写入 | 备注 |
|----------|---------|---------|------|
| ≤ max_lines ∧ ≤ max_bytes | 完整 | 写入 | 文件用于审计 |
| > max_lines ∧ ≤ max_bytes | 截断行 | 写入 | 注文件路径，LLM 可读文件 |
| ≤ max_lines ∧ > max_bytes | 截断字节 | 写入到 max_bytes 止 | 注文件路径 + 截断原因 |
| > max_lines ∧ > max_bytes | 截断行+字节 | 写入到 max_bytes 止 | 注文件路径 + 截断原因 |

### 截断行为细节

- **行数截断**：保留前 N 行，末尾附加 `... (truncated X lines, full output: {path})`
- **字节硬上限**：以 tokio 的 `read_to_end` 配合 `take(max_bytes)` 实现，读取超限后终止子进程 stdout pipe 读取。文件写入也在同一循环中，超出 max_bytes 后关闭文件句柄
- **两种截断共存时**：先读满 max_bytes 字节，再在此基础上按行截断

```
读取循环：
  loop {
    read chunk from stdout pipe (max 64KB)
    if total_bytes_read + chunk.len > max_output_bytes {
        truncate chunk to fit
        write truncated chunk to file
        write remaining to file (if still under limit)
        break
    }
    write chunk to file
    append to ring buffer (for truncation)
  }
```

## 注入提示词

当截断发生时，返回文本末尾追加以下说明行（仅当 `output_dir` 和 `call_id` 已配置时）：

```
... (truncated 1342 lines, full output: .wf/tmp/outputs/{session_id}/{call_id}-stdout.txt)
You can use `grep`, `head`, `tail`, `sed` on that file to search or paginate.
```

该提示纯文本注入，不增加结构化字段，调用方（LLM）直接可见。

## .wf 目录管理

### 目录结构

```
.wf/
├── .gitignore          # 自动生成，忽略整个 .wf/
├── tmp/
│   ├── outputs/        # CLI executor 输出文件
│   │   └── {session_id}/
│   │       ├── {call_id}-stdout.txt
│   │       ├── {call_id}-stderr.txt
│   │       └── ...
│   └── sessions/       # session 状态文件（预留）
│       └── {session_id}.json
```

### 文件命名

```
{session_id}-{timestamp}-{seq}-{tool_name}-stdout.txt
{session_id}-{timestamp}-{seq}-{tool_name}-stderr.txt
```

- `session_id`：Agent 会话 ID（UUID v7）
- `timestamp`：调用时间（Unix ms，保证排序）
- `seq`：会话内递增序号（防碰撞）
- `tool_name`：工具名（如 `bash`, `rg`），可读性

### 管理职责

**wf-tools 库层（本次实现）**：

- 接收 `output_dir` 参数，写文件
- 不负责创建/清理 `.wf/` 目录
- 文件写入使用 `tokio::fs::write` + `create_dir_all`
- 文件写入失败时静默降级（仅返回截断文本，不设 path）

**应用层（CLI 初始化时）**：

- 启动时创建 `.wf/tmp/outputs/`、`.wf/tmp/sessions/`
- 写入 `.wf/.gitignore`（内容：`*`）
- 清理过期文件（删除超过 24h 的文件）
- 可配置最大磁盘占用（默认 500MB）
- 可选：启动时清理全部现有输出（`rm -rf .wf/tmp/outputs/*`）

### 清理策略

| 时机 | 动作 | 实现方 |
|------|------|--------|
| 应用启动 | 删除 >24h 的 outputs 文件 | 应用层 |
| 应用启动 | 删除 outputs 空目录 | 应用层 |
| 会话结束 | 删除本会话 outputs 目录 | 应用层 |
| 运行时 | 检查总磁盘占用 >500MB → LRU 删除 | 可选（后台任务） |

## 实现步骤

1. `CliExecutionOptions` 追加 `max_output_bytes`, `output_dir`, `call_id` 字段
2. `CliExecutionResult` 追加 `stdout_path`, `stderr_path`, `truncated`, `truncated_reason`, `total_lines`, `total_bytes` 字段
3. `execute()` 内部新增 `tokio::io::AsyncRead` 循环：同时写入文件 + 构建截断文本
4. `truncate_lines()` 改为接收原始总行数/总字节数，在末尾注入文件路径提示
5. 文件写入失败时静默降级，不阻塞命令执行
6. 应用层适配：启动时创建 `.wf/tmp/outputs/` + 写入 `.gitignore`

## 边界情况

1. **目录不存在**：`create_dir_all` 自动创建，失败则降级（不中断命令）
2. **磁盘满**：文件写入失败 → 降级为纯内存截断，不抛出 Error
3. **并发写入**：single-writer per file（`call_id` 唯一），无需锁
4. **大输出性能**：千兆级输出时 IO 可能成为瓶颈，但命令执行本身更慢，可忽略
5. **敏感数据**：输出文件可能含 Secrets，清理策略需确保及时删除；由应用层负责安全清理（`shred` 可选）
6. **Windows 路径**：`output_dir` 使用 `PathBuf` 跨平台，文件名不含非法字符
