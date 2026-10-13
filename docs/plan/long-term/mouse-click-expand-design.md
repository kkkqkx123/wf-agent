# 鼠标点击展开长期设计方案（点击展开为主，徽标点击为附带）

目标是以点击工具输出展开为主要交互，徽标点击回尾为附带能力。只改原有模块，不新增替代模块。本方案为后续可选参考，假设实现，不在本阶段落地。

本阶段已落地键盘基线：主会话 `Ctrl+End` 回尾、`Ctrl+Home` 回顶、空输入 `End` 回尾、发送重贴、悬浮徽标；历史浮层对等支持 `End` 回尾与 `Ctrl+Home` 回顶。本方案复用这套偏移语义，不引入新的滚动模型。

## 1. 目标与非目标

目标：

- 点击任一已渲染工具块的摘要行，在折叠与展开之间切换，效果等同现有 `Ctrl+O` 但可逐块寻址。
- 点击脱离态悬浮徽标回到尾部，效果等同 `Ctrl+End`。
- 鼠标能力运行时可切换，默认保持原生选区；开启后提供应用内选区补偿与 `Shift` 还原生选区的逃生通道。
- 历史浮层与主会话行为一致，只是作用于各自的偏移字段。

非目标：

- 首期不做悬停高亮常开、不做自由 motion 跟踪、不做触屏手势。
- 不改变现有分页、重绘分级、无头回放的窗口公式。
- 不默认开启鼠标捕获，不改变现有配置缺省值。

## 2. 现状基线

会话滚动收敛在控制器侧，`0` 表示尾随，大于 `0` 表示冻结；徽标是逐帧覆盖绘制的文本，不具备命中矩形；工具块只有最近一个与全部两种操作粒度，可见行到逻辑块的映射在克隆出缓存后即丢失。

## 3. 参考实现位置

### 3.1 本仓库现状锚点

鼠标分类与事件环：

- `crates/app/cli/wf-tui/src/tui/input_classify.rs:63-87`：滚轮与按键点击的分类，`Down/Up/Drag/Moved` 当前一律归为忽略以保留原生选区。
- `crates/app/cli/wf-tui/src/tui/input_classify.rs:133-145`：鼠标 opt-in 配置读取，缺省关闭。
- `crates/app/cli/wf-tui/src/tui/input_classify.rs:166-197`：crossterm 按键到内部键的映射，`Home/End` 在此收敛。
- `crates/app/cli/wf-tui/src/tui.rs:153-159`：输入门控组装，鼠标来自用户 opt-in，焦点与键盘来自探测。
- `crates/app/cli/wf-tui/src/tui.rs:406-420`：鼠标事件循环，过滤 `Moved`，其余标记焦点并路由到滚轮或忽略。
- `crates/app/cli/wf-tui/src/tui.rs:689-726`：滚轮处理与忽略分支，滚轮复用分页感知的滚动方法。
- `crates/app/cli/wf-tui/src/tui.rs:544-554`：终端模式组装，策略是唯一的模式调用方。
- `crates/app/tui/tui-core/src/perf.rs:101-123`：运行时策略，鼠标捕获默认关，其余输入能力默认开。
- `crates/app/tui/tui-core/src/perf.rs:171-180`：输入门控只做减法，修策略开关即回滚对应阶段。
- `crates/app/tui/tui-terminal/src/terminal/modes.rs:16-20`：鼠标捕获与替代滚动的语义区分，前者拿走选区，后者保留选区。
- `crates/app/tui/tui-terminal/src/terminal/modes.rs:52-64`：全屏基线默认关闭输入能力，由策略层叠加。
- `crates/app/tui/tui-terminal/src/terminal/guard.rs:57-58,92-93`：进入与恢复时的捕获开关成对操作。
- `crates/app/tui/tui-terminal/src/terminal/sequences.rs:33-34,56-57,84-85`：进入、恢复、重断言三处的捕获序列。
- `crates/app/tui/tui-terminal/src/terminal/control.rs:19-20`：捕获开关的控制面抽象与测试替身。

滚动与徽标绘制：

- `crates/app/cli/wf-tui/src/interactive.rs:145-151`：`view_scroll` 与 `scroll_at_top` 的偏移语义。
- `crates/app/cli/wf-tui/src/interactive.rs:431-468`：更早分页的后台获取，结果以前插方式落地。
- `crates/app/cli/wf-tui/src/interactive/keys.rs:45-67`：`Ctrl+End/Ctrl+Home` 全局跳转与翻页步进。
- `crates/app/cli/wf-tui/src/interactive/keys.rs:104-138`：上翻触顶分页、下移步进、回尾回顶动作。
- `crates/app/cli/wf-tui/src/interactive/keys.rs:359`：无修饰 `Home` 走光标行首，`Ctrl+Home` 在此前被拦截。
- `crates/app/cli/wf-tui/src/interactive/render.rs:161-168`：视口钳制与顶部标记刷新，超顶哨兵在此收敛。
- `crates/app/cli/wf-tui/src/interactive/render.rs:395-427`：脱离徽标的可见性判断与右对齐悬浮绘制，不进缓存。
- `crates/app/tui/tui-render/src/screen_draw.rs:544-623`：历史浮层视图结构与尾锚绘制，偏移在此钳制并回报告知是否到顶。
- `crates/app/tui/tui-render/src/screen_draw.rs:625-690`：浮层状态行与悬浮徽标绘制，窄屏降级短版或隐藏。
- `crates/app/cli/wf-tui/src/history_overlay.rs:88-121`：浮层翻页、回尾回顶、视口回写与渲染快照。
- `crates/app/cli/wf-tui/src/tui/key_routes.rs:315-343`：浮层翻页与回尾回顶路由，触顶时优先分页。

工具块现状：

- `crates/app/cli/wf-tui/src/interactive/tool_blocks.rs:24-43`：块结构与摘要定位用的序列号、体行数记账。
- `crates/app/cli/wf-tui/src/interactive/tool_blocks.rs:114-149`：折叠与展开的体行生成，失败时错误优先。
- `crates/app/cli/wf-tui/src/interactive/tool_blocks.rs:340-378`：最近可展开块的查看器请求装配。
- `crates/app/cli/wf-tui/src/interactive/tool_blocks.rs:380-439`：最近块切换、全部切换与单块目标切换。
- `crates/app/cli/wf-tui/src/interactive/tool_blocks.rs:444-467`：按摘要序列号的体行原位拼接，丢失摘要则丢记账。
- `crates/app/cli/wf-tui/src/interactive.rs:200-201,263-268`：展开版本变更的缓存失效契约。

### 3.2 opencode 参考

- `ref/opencode/packages/tui/src/routes/session/index.tsx:521`：偏离底部判定与防抖后的状态提交，流式与导航期间保持窗口。
- `ref/opencode/packages/tui/src/routes/session/index.tsx:751`：回尾动作，清理导航、清锚点、恢复粘底并滚到内容高度。
- `ref/opencode/packages/tui/src/routes/session/index.tsx:1408`：滚动容器装配，滚动条变化与内容 resize 时更新偏离态。
- `ref/opencode/packages/tui/src/routes/session/index.tsx:1426`：滚轮路由，消息导航、旧行揭示、新行揭示优先于偏离更新。
- `ref/opencode/packages/tui/src/routes/session/index.tsx:1455-1475`：独占提示行上的跳转徽标，悬停变色与抬起回尾。
- `ref/opencode/packages/tui/src/routes/session/group-view.tsx:129`：块切换动作定义，悬停态与切换共享同一披露对象。
- `ref/opencode/packages/tui/src/routes/session/group-view.tsx:150-152`：工具组标题行的悬停与抬起切换。
- `ref/opencode/packages/tui/src/routes/session/group-view.tsx:181-183`：第二类分组行的同式悬停切换。
- `ref/opencode/packages/tui/src/routes/session/group-view.tsx:217-219`：锚点图标行的悬停与抬起切换，事件目标参与切换。
- `ref/opencode/packages/tui/src/routes/session/group-view.tsx:254-256`：底部工作表列表行的悬停与抬起切换。

### 3.3 atomcode 参考

指针与表面：

- `ref/atomcode/crates/atomcode-tui/src/surface.rs:203-215`：运行时拿取或归还指针，重启才能选是不可接受的。
- `ref/atomcode/crates/atomcode-tui/src/surface.rs:217-234`：自由 motion 单独门控，只在有跟随指针的内容时开启。
- `ref/atomcode/crates/atomcode-tui/src/plugin.rs:9826-9833`：表面行上报指针的配置注释，代价是终端自带点拖选区，修饰键可取回。
- `ref/atomcode/crates/atomcode-config/src/config/mod.rs:612`：指针上报的面向用户配置注释，点折工具调用与复制的权衡在此交代。

命中与选择：

- `ref/atomcode/crates/atomcode-tuix/src/render/interaction.rs:40-87`：单元格矩形包含判定、命中目标枚举、命中帧的逆序命中。
- `ref/atomcode/crates/atomcode-tuix/src/event_loop/pointer_select.rs:25-70`：多击窗口与 press 记录，单击双击三击的纯函数分类。
- `ref/atomcode/crates/atomcode-tuix/src/event_loop/mod.rs:9347-9361`：转录选区与上次点击记录的事件环状态。
- `ref/atomcode/crates/atomcode-tuix/src/event_loop/mod.rs:13353-13392`：指针命中处理，多击分类、转录路由优先、编辑器路由其次。
- `ref/atomcode/crates/atomcode-tuix/src/event_loop/mod.rs:13957-13993`：滚动清多击记录、放手无命中时结束拖选、resize 中止拖拽防幻影延伸。
- `ref/atomcode/crates/atomcode-tuix/src/render/mod.rs:457-462`：滚动到顶到底的渲染 trait 缺省空实现。
- `ref/atomcode/crates/atomcode-tuix/src/render/retained.rs:10509-10524`：保留渲染器的滚动步进与到顶到底实现。
- `ref/atomcode/crates/atomcode-tuix/src/render/worker.rs:380-399`：工作渲染器的滚动委托。
- `ref/atomcode/crates/atomcode-tui/src/host.rs:6907`：按块标识的折叠切换入口，点击即按此块寻址。

滚动键与文案：

- `ref/atomcode/crates/atomcode-tuix/src/event_loop/mod.rs:15805-15880`：滚动键统一处理，非空输入时不抢占行首行尾。
- `ref/atomcode/crates/atomcode-tuix/src/event_loop/mod.rs:15872-15877`：空输入 `Home/End` 到顶到底的实现位置。
- `ref/atomcode/crates/atomcode-i18n/src/product/en.rs:318`：空输入跳转到顶到底的帮助文案。
- `ref/atomcode/crates/atomcode-i18n/src/product/en.rs:1303`：点击展开的盖子提示文案。
- `ref/atomcode/crates/atomcode-i18n/src/screen/en.rs:431`：点击折叠的图例文案。
- `ref/atomcode/crates/atomcode-i18n/src/screen/en.rs:1044`：底部更多行的点击回底文案。

## 4. 总体方案

保持现有偏移语义不变，以命中帧把点击坐标翻译为已有动作：徽标命中等价 `jump_to_latest`，摘要行命中等价按块标识的切换。鼠标能力走运行时策略开关，不改配置缺省；命中帧只读偏移与布局，不改变窗口计算。

分层保持：滚动状态不下沉到归约层；命中帧由渲染层在全帧时发布，事件层只消费快照；工具块拼接仍走现有版本失效链路。

## 5. 状态与命中帧

在渲染侧收敛一个逐帧发布的命中快照，内容为徽标矩形与可见摘要行矩形，附带块标识与是否可展开。事件侧持有上一帧快照用于命中判定，跨帧的 press 与 release 必须属于同一快照世代，否则按拖选处理，避免 resize 与流式追加导致的错位点击。

坐标系约定：点击坐标先减去滚动区原点，再按容量与当前偏移换算为可见窗口内的行号；超顶哨兵值只存在于按键与绘制之间，命中换算必须使用钳制后的值。弹窗、加载占位、审批问询遮罩下不发布任何命中区，避免点穿，与现有徽标抑制一致。

窄终端下不绘制徽标时也不发布其命中区；摘要行在折叠态只有一行可点，展开态摘要行仍可点以折回，体行不可点以保留选区语义。

## 6. 鼠标捕获与事件路由

捕获开关从启动时一次性改为运行时可切换： конфи opt-in 仍是总闸，运行时开关在此之下做 minute-to-minute 切换，切换时经终端守卫成对启停捕获并请求全帧。关闭捕获即回到现有行为，原生选区无损。

事件路由顺序：滚轮保持现有行为；按下记录坐标与快照世代；抬起时若同一世代同格且落在命中区则执行对应动作并请求全帧，否则按现有忽略逻辑处理，不干扰原生选区。`Shift` 修饰的按下与拖拽一律走原生选区，不参与命中。motion 事件默认不启用，悬停高亮也不做，避开持续帧开销；若后续要做悬停，需单独门控并只在菜单类内容上开启。

历史浮层复用同一路由，只是命中快照来自浮层绘制，动作为浮层自己的回尾与分页。

## 7. 点击展开细节

摘要行到块的映射：渲染可见窗口时记录每行来源的块标识与行角色，摘要行可命中，体行与普通行不可命中。点击摘要行时按块标识走现有单块切换路径，原位拼接体行并 bump 版本，全帧重排。不可展开的块不发布命中区，避免无反馈点击。

分页 interplay：触顶且分页为部分态时，点击不触发分页，分页仍归 `PageUp` 与 `Ctrl+Home`；点击只作用于已加载行的折叠态。丢失摘要（被截断回收）时按现有逻辑丢记账，点击快照中也不应再包含该块。

大输出防护：展开仍走现有截断上限；检索类默认折叠策略不变；查看器入口保持 `Ctrl+V`，点击只做行内折叠，不开大输出弹窗，避免一次误触淹没转录。

## 8. 徽标点击细节

徽标命中是展开命中帧的附带项：脱离态且徽标已绘制时发布右下矩形，命中即清零偏移。状态行中的 `End for latest` 文案保留，鼠标用户同时看到可点击暗示。浮层徽标同理，命中即浮层回尾。徽标矩形不参与缓存键，每帧重算，与现有绘制一致。

## 9. 选区补偿与复制

开启捕获即失去终端原生点拖选区，必须同时提供应用内选区与复制补偿，否则为净回归。最小补偿为转录拖选加语义端点、双击选词三击选行的纯函数分类、选中内容经现有复制通道输出。`Shift` 按下时跳过应用内选区，把事件还给终端，保证用户总有一条路可以原生复制。复制失败与拒绝仍走现有 notice 队列，不新增通知通道。

## 10. 重绘与性能

复用现有分级，不新增分级。命中帧发布伴随全帧，不污染底部专用与动画专用路径；点击切换伴随内容版本变化，自然落在全帧内。motion 常开是最大的持续开销，首期不做。同步输出仍只包全帧与底部帧，动画帧不包。弱终端与 ssh 下帧预算本来就紧，捕获开启后的 Down/Up/Drag 事件增量是主要输入开销，路由层对非命中事件必须保持无帧或复用现有滚动帧，不为每次按下都申请全帧。

## 11. 测试

控制器单测覆盖按块切换的目标寻址、不可展开块无命中、丢失摘要丢记账；命中换算单测覆盖偏移平移、换行导致的逻辑行到显示行展开、窄宽度下徽标无命中、弹窗遮盖无命中；事件单测覆盖 press 与 release 跨世代按拖选处理、滚动与 resize 清多击记录、`Shift` 还原生；浮层单测覆盖尾页重置、回尾复位、触顶分页仍归键盘；无头回放用例覆盖窗口公式不受命中帧影响。

## 12. 落地顺序与验收

先做运行时鼠标开关与 `Shift` 还原生，达到可回退；再做命中帧与徽标点击试点，验证坐标、遮盖、点穿链路；最后做逐块点击展开与选区补偿，达到与键盘 parity。验收：开启后点击摘要行逐块折叠，点击徽标回尾，关闭后原生选区无损；`Shift` 点拖始终原生；弱终端与 ssh 下无持续帧率回归；现有滚动、分页、重绘与无头基线测试全过。
