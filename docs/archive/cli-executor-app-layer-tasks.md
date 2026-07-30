# CLI Executor — 应用层待实现任务

以下功能由应用层（CLI 初始化/运行时）负责，库层（wf-tools）不参与。

## 1. .wf/ 目录初始化

应用启动时：

- 创建 `.wf/tmp/outputs/`（递归）
- 创建 `.wf/tmp/sessions/`（预留）
- 写入 `.wf/.gitignore`，内容为 `*`，确保整个 `.wf/` 不被 Git 跟踪
- 清理残留：删除 `.wf/tmp/outputs/` 中超过 24h 的文件和空目录

## 2. session_id 与 call_id 生成

- session_id：UUID v7，每次 Agent 会话开始时生成
- call_id：格式 `{session_id}-{timestamp_ms}-{seq}`，每次 CLI tool 调用时递增 seq
- 传递给 `CliExecutionOptions::{output_dir, call_id}`
  - `output_dir` = `.wf/tmp/outputs/{session_id}/`
  - `call_id` = 上述格式字符串

## 3. 清理策略

| 时机 | 动作 | 备注 |
|------|------|------|
| 应用启动 | 删除 outputs 中 >=24h 的文件 | 防磁盘膨胀 |
| 应用启动 | 删除 outputs 下的空目录 | 清理残留 |
| 会话结束时 | 删除本会话的 outputs 目录 | `rm -rf .wf/tmp/outputs/{session_id}` |
| 运行时（可选） | 检查总磁盘占用 >500MB → LRU 删除 | 后台定期任务 |

## 4. 安全清理（可选）

- 文件可能包含敏感数据（Secrets, API Keys, 凭证）
- 可配置使用 `shred`（Unix）或 `sdelete`（Windows）覆盖删除
- 默认策略：普通 `rm` + 文件内容自然过期（清理周期 <=24h）

## 5. Config 配置项（建议）

应用层可在配置中暴露：

```yaml
cli_executor:
  max_output_bytes: 20971520        # 20MB
  max_output_lines: 1000            # 返回截断行数
  output_dir: ".wf/tmp/outputs"     # 输出目录
  max_disk_usage: 524288000         # 500MB
  cleanup_max_age_hours: 24         # 文件保留时间
```

## 6. 注意事项

- `output_dir` 和 `call_id` 均为 `Option`，任一个为 `None` 则库层跳过文件写入
- 文件写入失败时库层静默降级，不返回 Error
- 应用层无需关心文件写入细节，只需在启动时保证目录存在
