# 上下文压缩实现

> 范围：`wf-agent`（发射与回写）、`wf-execution-shared`（会话、决策、信号分发、统一发射谓词、统一等待、估计单源与预算安装点）、`wf-runtime/trigger_listener/compression`（适配与管线，含共享单次运行）、`wf-runtime/bootstrap/trigger`（装配，单一预算安装点）、`wf-resource`（摘要链模板）、`wf-workflow/handler/llm`（工作流侧发射与等待，单摘要形态）
> 定位：跨层链路，预算判定在引擎，摘要执行在运行时监听侧，写回在会话，靠版本锚点串起幂等。决策与上报的唯一稳定来源为热标记，预算估计按需重算投影，不再读取缓存估计。执行核唯一：内建计划批与用户模板单摘要共用同一单次运行语义；投递单次：直接执行成功时只发审计拷贝，快照路由副本仅为无管线时的回退。

## 触发

分常规预算与强制安全网两类，智能体侧与工作流侧对称，均为警告或发射语义，不阻塞请求。

实现：智能体侧位于 `crates/engine/wf-agent/src/coordinator/iteration/compression.rs`（决策与发射）与 `crates/engine/wf-agent/src/coordinator/iteration.rs`（预检与停车门）；工作流侧位于 `crates/engine/wf-workflow/src/handler/llm/token_budget.rs`（常规发射与等待）与 `crates/engine/wf-workflow/src/handler/llm/events.rs`（强制发射）；决策纯函数位于 `crates/engine/wf-execution-shared/src/compaction/`（`decision.rs` 决策、`plan.rs` 计划、`budget.rs` 阈值、`strategy.rs` 策略、`blocks.rs` 块同步）。

### 智能体常规触发

每轮用量结算后进入压实决策，需同时满足用量跟踪开关打开、模型窗口上下文限额已知、压实窗口已知，否则直接返回，兜底交给强制安全网。

结算只负责用量落账、任务预算事件与前置门限检查，不再预估上报值。决策内刷新热估计后，以同一热值同时做阈值比较与信号上报，上报与判定同一样本。

热估计单源为请求热函数：输入已刷新的热标记、组装后请求估计与校准偏置，内部先算动态开销再叠加并校准。动态开销不落账本。决策为纯函数，按热值与阈值比较决定保持、丢最老活跃块、进入交接或发射压实。单摘要策略跳过块分支。激进计划释放后仍超硬线直接进交接，空计划在非单摘要模式下直接返回。

实现：热函数位于 `crates/engine/wf-execution-shared/src/context_store.rs`（请求热、共享动态开销、偏置合成）；会话侧刷新与计划装配位于 `crates/engine/wf-execution-shared/src/conversation_session/compression.rs`，跟踪器飞行与 streak 位于 `crates/engine/wf-execution-shared/src/token_tracker.rs` 与 `conversation_session/usage.rs`。

### 智能体强制触发

语言模型调用被服务商以上下文超长拒绝时发射强制信号。特征为强制标记为真、走无状态计划、不查规划建议、无请求级指令。同一版本已发射则跳过。发射前写入待定计划使强制结果同样可匹配，否决与无接管时清除。强制计划走共享无状态计划函数，智能体侧与工作流侧同源，永不咨询规划建议。

实现：共享计划函数位于 `context_store.rs` 的强制计划函数；智能体侧发射位于 `iteration/compression.rs` 的强制路径，调用点位于 `iteration/llm_call.rs` 与 `iteration/streaming.rs`；工作流侧发射位于 `handler/llm/events.rs`，调用点位于 `handler/llm/stream.rs` 与 `handler/llm/tool_exec.rs`。

### 工作流触发

语言模型节点在发射前按声明数组逐个检查。共享动态开销只算一次：组装后请求估计减去读数组估计，逐数组叠加各自投影估计后再走部件热函数统一校准，保持多目标共享动态语义不变。工作流压缩按构造为单摘要形态：无块表、永不走分层规划器，预算超限走常规发射，上下文超长错误走强制发射，嵌套压缩运行时保持静默。

实现：检查与发射位于 `handler/llm/token_budget.rs` 的常规路径，嵌套深度守卫位于 `crates/engine/wf-workflow/src/message_context.rs`，数组版本与读写经由该文件的变量表账本；工作流单摘要断言位于 `context_store.rs` 的工作流形态函数。

### 发射守卫

同版本只发一次，新消息推进版本后才重 arm。已有机上飞行则本轮合并等待，过期飞行自释放。触发监听侧另有执行加目标加版本为键的认领表，重复投递直接丢弃，靠版本检查释放。锚定判定收敛到同一谓词：只有分发结果为已接管才锚定飞行，否决清待定但不计无接管，无接管保留审计事件且不锚定，智能体侧与工作流侧同谓词。

实现：版本守卫位于 `wf-types/src/llm/ledger.rs` 的账本守卫经由 `context_store.rs` 的数组守卫函数；飞行锚定谓词为 `context_store.rs` 的锚定谓词；管线认领表位于 `crates/app/wf-runtime/src/trigger_listener/compression/claim_table.rs`。

## 发射

发射器用当前视图快照构造压缩请求，携带目标、用量、限额、消息数、版本、强制标记、计划条目与请求级指令。先写压缩前检查点，再同步分发钩子信号，最后发布审计事件。

分发语义分三种：观察者否决则在接管前取消；观察者替换则以供给摘要直接落地为完成事件并跳过摘要链，空供给视为否决，无总线时替换直接取消；无否决无替换时运行具名接管。智能体侧附带循环标识，工作流侧不带。静态定义按信号点过滤后参与门控，不可解析的处理器失败开放。常规与强制共用同一发射入口与审计标注，差异只在强制标记、计划来源与指令为空。

实现：统一发射入口与审计标注位于 `context_store.rs` 的统一发射结构与分发函数；信号分发内核位于 `context_store.rs` 的分发函数与钩子注册表 `crates/engine/wf-execution-shared/src/hooks/`。

## 适配

压缩服务为薄适配器，只做解析与空快照跳过。单次投递：直接生成管线执行成功时发布无快照审计拷贝；仅当无管线接管时才发布带快照的路由副本作为回退，由内建模板匹配执行。路由副本带路由标记与嵌套深度，只有内建模板能匹配。用户模板禁止携带保留动作与指向该信号，加载期与匹配期双重拦截。

实现：适配器位于 `crates/app/wf-runtime/src/trigger_listener/compression/service.rs`，信号解析位于同目录 `signal.rs`，内建模板与注册表位于同目录 `registry.rs`，路由分发位于 `crates/app/wf-runtime/src/trigger_listener/router.rs`。

## 管线

管线独占执行策略：认领、重试退避、降级回退、写回编排、终态事件、触发器状态与账本审计。

快照先按限额裁剪到输入上限并扣除指令开销，零限额不裁剪，裁剪返回丢弃数。计划索引基于未裁剪快照，整体前移丢弃数并过滤空区间，块标识与深度不变。全被裁掉则回退单摘要。切片时每片头插请求级指令，计划索引合法性不受插值影响。空计划为单摘要模式，一片覆盖全快照。深度大于零的条目走重压链，未配置时回退主链。

逐片运行摘要子工作流，单片超时、最大尝试次数与退避共享整批预算，关闭信号可中止。单次运行语义单点：内建计划批与用户模板单摘要走同一单次运行函数，超时措辞、空输出判定与退避一致，用户模板侧以零重试保持单摘要形态。

实现：管线编排位于 `crates/app/wf-runtime/src/trigger_listener/compression/pipeline.rs`，逐片执行位于同目录 `chain_run.rs`（含共享单次运行函数），输入裁剪与切片位于同目录 `input_window.rs`，终态结算位于同目录 `terminal_report.rs`，重试退避与超时嵌套位于同目录 `policy.rs`；用户模板单摘要执行位于 `crates/app/wf-runtime/src/trigger_listener/workflow_runner.rs`，经由共享单次运行函数执行；摘要链模板位于 `crates/engine/wf-resource/src/predefined/workflow.rs` 的折叠摘要链。全部成功后一次聚合写回，版本锚点只推进一次，整批只落一个完成事件。单片失败耗尽重试后按终态结算：过期为放弃且已在写回内发布丢弃事件，不发失败事件；中止记放弃；失败按摘要工作流资源声明的回退策略处理，降级模式把本地裁剪窗口加显式降级声明写回并记成功，失败模式发失败事件并释放工作流侧飞行锚点。账本中中止与过期记放弃，降级成功记完成。

## 回写

智能体循环启动时为存活会话拉起消费者，循环结束时中止。消费者订阅完成、失败、丢弃与嵌套写回四类事件，只处理本循环标识且目标为会话上下文的事件。

完成事件先做版本锚点检查，失配则释锚并发布丢弃事件，命中则按是否带块结果走聚合应用或单摘要应用。聚合应用取出待定计划与丢弃并迁移块，空块结果同样清理待定，与计划模式对称。压完仍超预算时计划模式在正常态直接请求交接，其余计停车 streak，达阈值记错等待新消息，否则清 streak。成功后走策略门控的压缩后检查点。失败与丢弃事件释飞行并清待定，发射守卫保留，版本靠新消息重 arm。嵌套写回为追加语义，同样版本检查，新消息胜出。

工作流变量映射目标经注册表直接写回，版本失配发布丢弃事件并按过期处理。落地后重 arm 预检警告并清理飞行锚点，仍超预算计 streak 供下一轮统一等待前检查是否直接停车。飞行归属：工作流目标的持久化锚点在此释放，智能体目标的飞行永不在此触碰，由智能体引擎消费者统一释放。

实现：写回编排位于 `crates/app/wf-runtime/src/trigger_listener/write_back.rs`，预检重 arm 与 streak 持久化适配位于同目录 `budget_tracker.rs`；智能体消费者位于 `crates/engine/wf-agent/src/conversation_compression.rs`，聚合应用与视图切换位于 `crates/engine/wf-execution-shared/src/conversation_session/compression.rs`，版本检查位于 `context_store.rs` 的锚点函数。

## 状态

会话历史只追加，压缩只追加摘要并切换读投影，撤销压缩为零成本视图切回。活跃视图为分段结构，由摘要段与序号区间段组成。

增量视图估计已删除，无快速读数旁路，无检查点字段。热标记与按需投影重算为决策唯一来源，缓存估计只做观测。账本只保留版本与发射守卫，计数与估计在写入时刷新、读取时纯读，决策永远重算投影。

实现：投影估计位于 `context_store.rs` 的稳定投影函数，按需重算；会话视图与历史位于 `conversation_session/history.rs`，压实应用位于 `conversation_session/compression.rs`，跟踪器透传位于 `conversation_session/usage.rs` 与 `token_tracker.rs`；变量表读写与账本位于 `message_context.rs`，账本结构位于 `wf-types/src/llm/ledger.rs`。压实状态随会话与检查点持久化，承载模型窗口、推导阈值、热标记、块表、待定计划与丢弃、交接请求标记、交接产物与模式。块有原始、已压缩、密封三态与重压计数，丢弃块直接清表。用量跟踪器存飞行记录、无接管 streak、仍超预算 streak、预检警告单次标记与校准偏置，压缩成功后重 arm 预检警告。

阈值由模型窗口按比例推导。块同步只在分层策略下开启，单摘要策略只保留阈值门控。校准偏置为实际减估计的指数平均，偏置合成只有一处实现，跟踪器方法委托该实现。快速读数、校准包装与简单版本等待等仅测试使用的旁路已删除。

## 等待

所有发射器共用同一结算原语与终态枚举：版本前移、失败、飞行释放、超时、取消。循环内先判版本再判失败最后判释放，超时与取消经由选择分支返回，成功优先于失败。轮询加取消选择是唯一语义，无订阅唤醒快路，最多增加一个轮询间隔延迟。

智能体侧映射为继续迭代、压缩失败停车、取消转调用取消错误。工作流侧按目标逐个等待，版本前移继续下一目标，失败停车加路由错误，释放与超时按超时暂停处理，取消转取消中断错误。仍超预算停车措辞两侧共享同一函数。通用仅判版本移动的等待已删除，不再保留双实现。

实现：结算原语与停车措辞位于 `context_store.rs` 的结算函数、停车消息函数与仍超预算措辞函数；智能体等待位于 `iteration/compression.rs` 的飞行结算与 `iteration.rs` 的停车门；工作流等待位于 `handler/llm/token_budget.rs` 的逐目标等待与停车门。

生效预算优先显式注入，缺失时读装配期纠正后的进程全局并记调试日志，两侧下限一致。生命周期协调器透传显式值，不做二次计算。

## 预检

两处预检共用同一精算函数：阈值以下用本地估计，接近或超过预算时调用服务端计数精算，失败回退本地值。警告语义不阻塞请求，单次标记，压缩写回后重 arm。智能体侧在组装后请求上检查，工作流侧在声明数组维度上附加数组明细。

实现：精算函数位于 `context_store.rs` 的精算函数；智能体预检内联于 `iteration.rs` 的迭代骨架；工作流预检位于 `handler/llm/token_budget.rs` 的预检函数，重 arm 经由 `trigger_listener/budget_tracker.rs` 与会话写回。

## 装配与扩展

进程全局在装配期经单一安装点一次性注入：等待预算与压实限额同批安装。全局为 bootstrap-only，生产路径优先显式注入，缺失时读全局并记调试日志，测试经专用复位函数隔离。

实现：单一安装点位于 `context_store.rs` 的预算安装函数，装配调用位于 `crates/app/wf-runtime/src/bootstrap/trigger.rs`；等待预算全局位于 `wf-execution-shared/src/token_events.rs`，压实限额全局位于 `compaction/strategy.rs`，显式覆盖分别经由迭代协调器与执行器的 `with_*` 方法。缺失主链回退内建并记错，重压链缺失仅记调试日志。超时嵌套在装配期显式校验：等待预算须覆盖最坏声明成本，违例记错并修正到下限。各执行器经显式注入覆盖全局，缺失时读全局。

规划建议为命名策略的纯整形建议，引擎全量校验与钳制，缺失失败空建议回退内建。终态声明由摘要工作流的触发子流程配置声明失败或可见降级，缺失装配期告警一次。运行时路由配置携带摘要标识、重压标识与策略，支持策略、重压链、会话注册表与账本注入。

可观测事件为请求审计、路由副本、完成、失败、丢弃、任务用量警告与超限、触发器起止记录与耐久账本、压实指标与调试维度。审计字段与版本号保持不变。

## 实现位置总览与重复判定

按逻辑落点：决策纯函数在 `wf-execution-shared/src/compaction/`；会话与跟踪在 `conversation_session/` 与 `token_tracker.rs`；统一发射、等待、估计、安装点在 `context_store.rs`、`token_events.rs`、`compaction/strategy.rs`；智能体发射等待在 `wf-agent/src/coordinator/iteration/`，回写消费在 `wf-agent/src/conversation_compression.rs`；工作流发射等待在 `wf-workflow/src/handler/llm/`，变量表在 `message_context.rs`；适配管线在 `wf-runtime/src/trigger_listener/compression/`，用户模板执行在 `trigger_listener/workflow_runner.rs`，装配在 `bootstrap/trigger.rs`，摘要链在 `wf-resource/src/predefined/workflow.rs`。

重复判定：已消除的多套实现为直接投递与路由副本的双执行（现仅单次投递加回退）、内建批与用户模板的手写超时重试（现共用单次运行函数）、四处发射的锚定分支与强制计划构造（现共用锚定谓词与强制计划函数）、两侧仍超预算措辞（现共用措辞函数）、读时懒重算的缓存估计（现写入刷新读取纯读）。仍并存但非重复的：一是内建管线与用户模板执行的外壳差异，前者带认领重试降级聚合写回与账本，后者为单次单摘要，前者是后者的超集而非并列实现；二是智能体分层计划与工作流单摘要，系构造差异而非两套同能实现，工作流无块表是既定形态；三是调用点四处并存（智能体常规强制、工作流常规强制），内核已统一，仅触发时机与快照来源不同；四是认领表与版本守卫并存，前者防重复执行，后者防过期写回，分工不同。
