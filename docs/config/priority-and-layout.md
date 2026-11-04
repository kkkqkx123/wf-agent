# 优先级与布局

## 优先级总则

显式覆盖 > 环境变量（`WF_*`） > 项目层（`.wf/`） > 用户 settings 层 > 代码默认值。

补充：

- CLI flags / SDK `ConfigOverrides` / 程序化覆盖最高。
- 显式托管目录（ops 渲染）在有传入时替代 settings 与 project 层的基础设施文件。
- 服务进程默认不读 ambient 个人目录；必须显式传 `--settings-dir`。

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
- MCP / Skill / Keymap：项目文件优先，全局文件补充；缺失跳过，坏文件告警跳过。

## Example 跳过规则

所有加载器跳过文件名含 `.example.` 的文件；索引展开与家族 glob 也跳过。因此样例不可能泄漏进运行配置。
