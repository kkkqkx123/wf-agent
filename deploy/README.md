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

## 规划中的布局（真文件补充时按此落位）

- `deploy/systemd/`：服务单元模板。`EnvironmentFile` 给密钥、
  `WorkingDirectory` 指向业务项目、显式传参，不写死任何路径。
- `deploy/scripts/render-config.sh`：以 `config-examples/` 为唯一来源做变量替换，
  渲染到目标路径；凭据相邻文件 `0600`；密钥只从环境变量来。
- `deploy/scripts/bootstrap-dev.sh`：开发自举渲染（开发环境默认不足时补齐）。

## 禁忌

- 无秘密、无渲染后的真实值进仓库。
- 不把某台机器的实际配置复制回仓库当"生产基线"。
- 服务单元按 principal 区分：系统服务指向托管目录与项目 `.wf/`，
  个人自用指向 XDG；绝不让共享服务读取 ambient 个人目录。
- 生产部署必须渲染全部网域文件或显式设置运行环境，
  否则回退到代码默认（开发环境默认指向 `./dev-storage` 等开发路径）。
