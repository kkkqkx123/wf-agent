# 优先级与布局

## 优先级总则

显式覆盖 > 环境变量（`WF_*`） > 项目层（`.wf/`） > 用户 settings 层 > 代码默认值。

补充：

- CLI flags / SDK `ConfigOverrides` / 程序化覆盖最高。
- `--settings-dir` 提供 settings 层，`--config` 提供项目层；未传入的根完全跳过，不会读取进程工作目录下的 `.wf`。
- 服务进程默认不读 ambient 个人目录；需要共享默认配置时显式传 `--settings-dir`。
- 服务端固定使用 Production 环境默认值；缺失基础设施域文件时会回退到该环境的默认值。

## 布局

- 项目层根目录：`{project_root}/.wf`
- 易变目录：`.wf/meta`（会话缓存等）、`.wf/tmp`（超长输出溢写）；自动维护内部 `.gitignore`（内容 `*\n!.gitignore`），已有自定义内容不覆盖。
- 配置家族目录：`infrastructure`、`server`、`llm-auth`、`llm-profiles`、`llm-providers`。
- 索引文件统一名：`index.json`。
- 样例目录：`config-examples/`，永不加载。

## 领域文件加载语义

- 基础设施九域文件：缺失回退环境优化默认值；存在但解析失败报错。
- Sandbox：任一来源存在时 fail-fast 校验；无来源时为 `None`。
- 用户层 `config.toml`：宽松加载，坏文件告警跳过。
- MCP / Skill / Keymap：项目文件优先，全局文件补充；只有 settings 目录时仍能加载全局文件；缺失跳过，坏文件告警跳过。
- LLM 文件族：非法索引或记录直接失败；程序化记录与文件记录按 `id` 合并，程序化优先。

## Example 跳过规则

所有加载器跳过文件名含 `.example.` 的文件；索引展开与家族 glob 也跳过。因此样例不可能泄漏进运行配置。
