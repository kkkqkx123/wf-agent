# deploy/ — 部署机制（从不参与加载）

本目录回答"生产配置如何到达真实路径"，本身永不参与配置加载。
当前实际没有任何生产真文件，全部仍是演示；
真文件后续在目标机器上由本目录的机制渲染，绝不提交进仓库。

## 职责边界

- `config-examples/`：值长什么样（唯一的值来源）。
- `deploy/`：值如何到达真实路径（脚本、服务单元、渲染模板的占位目录）。
- 真实路径：`/etc/wf` 或显式设置目录（托管/共享服务）、`~/.config/wf`
  或 `~/.wf`（个人自用）、业务仓库 `.wf/`（项目增量）。
- 代码默认值：以上全缺席时的底线（开发 Development、生产 Production
  两套环境默认，见 `wf-config::processor::infrastructure`）。

## 落位布局

- `deploy/templates/`：由 `scripts/sync-deploy.sh` 从 `config-examples/`、
  `.wf/`、`.wf-user/` 单向同步生成，可评审、不可手改。模板只含加载器
  会读的 live 家族与可导入的种子家族，不含其他目录。
- `deploy/scripts/render-config.sh`：目标机按层渲染
 （`--to managed|settings|project`），部署期占位符用 `@@NAME@@`，
  `${VAR}` 一律透传给进程启动时展开；环境文件 `0600`。
  各层覆盖与加载器的对应关系：
  `managed` 渲染运维 live 层（`infrastructure/`、`server/`、
  `config.toml`、`mcp/skill-settings.json`、`llm-auth/`）到托管目录，
  该目录即服务的 `settings_dir`；`settings` 渲染同样的 live 家族另加
  `llm-profiles/llm-providers/` 种子到个人设置目录；`project` 整体渲染
  `templates/project/` 到 `.wf/`（含各资源索引家族的导入起点，加载器
  在此只自动读 `mcp.json/skills.json`），`infrastructure/server` 覆盖
  仅随 `--with-project-overrides` 进入项目。
- Live 与种子的界限：网域九文件、传输四文件、`config.toml`、
  `mcp/skill-settings`、`llm-auth` 由加载器自动读，是 live 配置；
  `llm-profiles/llm-providers` 与各资源索引家族（`workflows`、
  `agent-loops`、`node-templates`、`prompt-templates`、`scripts`）只做
  `import` 起点，网关与注册表的来源是 API 与存储，启动时不扫描它们。
- `deploy/scripts/bootstrap-dev.sh`：开发自举渲染（开发环境默认不足时补齐）。
- `deploy/systemd/`：系统服务与个人单元模板。`EnvironmentFile` 给密钥、
  `WorkingDirectory` 指向业务项目、显式传参，不写死任何路径。
- `deploy/env/wf-server.env.template`：环境文件模板，只留空键、不带值。

## 禁忌

- 无秘密、无渲染后的真实值进仓库。
- 不把某台机器的实际配置复制回仓库当"生产基线"。
- 服务单元按 principal 区分：系统服务指向托管目录与项目 `.wf/`，
  个人自用指向 XDG；绝不让共享服务读取 ambient 个人目录。
- 生产部署必须渲染全部网域文件或显式设置运行环境，
  否则回退到代码默认（开发环境默认指向 `./dev-storage` 等开发路径）。
