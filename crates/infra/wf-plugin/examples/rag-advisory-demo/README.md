# rag-advisory-demo 示例插件

最小 Rust 原生插件，一次演示全部五种 RAG 检索扩展：元数据抽取、切分策略、
路由选择、后处理、评估裁判。全部实现确定、无副作用；宿主负责校验、合并与
失败回退，行为对照 `rag-core/src/plugin.rs` 的桥接语义。

## 构建

```sh
cargo build --release
```

产物为 `target/release/librag_advisory_demo.so`；改名去掉 `lib` 前缀后与
`plugin.toml` 的入口名一致。macOS 改名为 `rag_advisory_demo.dylib`，
Windows 改名为 `rag_advisory_demo.dll`。

## 安装试运行

原生后端默认关闭，仅用于完全受信的第一方扩展。开启后按以下方式试运行：

```sh
mkdir -p ~/.wf/plugins/rag-advisory-demo
cp target/release/librag_advisory_demo.so ~/.wf/plugins/rag-advisory-demo/rag_advisory_demo.so
cp plugin.toml ~/.wf/plugins/rag-advisory-demo/
```

## 五个贡献与宿主接线

| 贡献名 | 宿主配置项 | 行为 |
|---|---|---|
| `demo-topic` | 摄取配置 `metadata_extractor` | 文本含 `rust` 时建议 `topic=rust`，否则空对象；宿主只补全缺失键 |
| `demo-paragraph` | 摄取配置 `chunk_splitter` | 按空行切块；空建议时宿主回退内置切分 |
| `demo-keyword` | 路由配置 `route_selector` | 查询中出现目标名即选中；空建议时宿主回退关键词规则 |
| `demo-ordered` | 查询配置 `postprocessor` | 按 `chunk_id` 升序重排；未知标识宿主忽略 |
| `demo-cited` | 宿主直调裁判辅助，或合成配置 `eval_judge` | 答案带方括号引用得一分，否则得零点五分；越界分数宿主拒绝 |

## 实现要点

* 只实现 `WfNativePlugin` 的 `register` 与 `dispatch`，其余生命周期走默认。
* 分发均为纯函数：输入 JSON 只读，输出形状固定在零到一分、对象与数组契约内。
* 未注册的分发名直接报错，由宿主转为回退或显式失败，不静默通过。
* 检索扩展只走原生通道；Lua 与 Wasm 宿主不提供对应声明字段。
