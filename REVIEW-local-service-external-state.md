# Review:本地服务"外部运行中"状态(external)

> 供 review 的自包含汇总:问题、根因、设计决策、逐文件改动、测试证据、风险点。
> 第 1–6 节保留前两轮的实现与修复记录；当前最终行为和验证以第 7 节为准。
> 本仓库工作区还有大量其他未提交改动,**本次改动只涉及下文"改动清单"列出的 9 个文件**(另有 3 个 CLI 文件属于此前独立的改动,见文末备注)。
>
> **2026-10-02 更新**:首轮 review 提出 3 个 P2 + 1 个 P3,已全部修正,见文末"第二轮修复记录"。

## 1. 问题

本地服务(如 SillyTavern、gcli2api)在 AssetMesh 之外手动启动、正在运行时,服务页显示"**已停止**"。

**根因**:运行时状态只描述"AssetMesh 自己拉起的托管进程"(内存注册表,按资产 ID 索引)。注册表无条目时,`status()` 一律返回默认值 `Stopped`——把"**未被 AssetMesh 托管**"与"**已停止**"两个语义混为一谈,对用户构成了错误陈述("服务没在跑")。

**为什么此前被判定为有意设计**:运行时的安全边界是"只 signal 自己 spawn 的进程组,绝无端口/名字/PID 扫描去杀外部进程"([runtime.rs](apps/desktop/src-tauri/src/runtime.rs) 模块注释)。这个边界本身是对的,本次改动**不触碰**它。

**判定为缺陷的依据**:同一个信号(端口探活)已被信任到足以**拒绝一次用户操作**(启动冲突:`local port X is already in use — the service appears to be running outside AssetMesh`),却不用来**显示一个状态**——设计内部不一致。且状态机缺第三态:控制轴(托管生命周期)与观察轴(端点探活)被混在了一起。

## 2. 设计决策

**六状态:五个托管生命周期状态不变,新增一个 `external`。**

| 状态 | 含义 | 来源 | UI 控制 |
|---|---|---|---|
| starting / stopping | 托管进程过渡态 | 托管生命周期 | 按钮禁用 |
| running | 托管进程存活 | 托管生命周期 | 停止 / 打开 |
| stopped | 无托管进程且端口无响应 | 托管生命周期 + 探活 | 启动 |
| failed | 上次托管运行异常退出 | 托管生命周期 | 启动 + 退出原因 |
| **external(新)** | 无托管进程,但 endpoint 端口有响应 | **仅探活,只读** | **仅打开,无启停** |

关键不变量:

1. **优先级**:注册表有条目(含已结束的运行)→ 永远报告托管状态;只有无条目才探活。因此"上次托管运行的结果"不会被外部实例顶掉。
2. **`external` 只读**:每次读取重新计算、不落盘、无 pid、无停止按钮。探活从不 signal 任何进程——安全边界原样保留。
3. **探测范围收窄**:仅 active 的 `local` 服务 + 填了 loopback `endpoint_url` 才探测;saas 等其他类型即使填了 127.0.0.1 地址也不探测。读失败降级为"未监听"(stopped)——"AssetMesh 无托管进程"这一事实始终为真,探活只是对显示状态的精化。
4. **措辞诚实**:UI 徽章叫"外部运行中",详情页注释"该实例由 AssetMesh 外部启动,无法在此停止",沿用启动冲突报错里 "appears to be" 的留有余地语气(端口有响应 ≠ 严格等于就是这个服务在跑)。

明确**不加**的状态(已讨论并否决):`未知`态(没填 endpoint 的服务显示 stopped 是可接受近似,引导填地址而非加状态)、`不健康`态(健康检查兔子洞,代码注释明确 "never a health check of the service itself")、持久化运行状态(运行事实不落盘是既有架构原则)。

## 3. 改动清单(9 个文件)

### 后端(Tauri/Rust)

1. **[apps/desktop/src-tauri/src/runtime.rs](apps/desktop/src-tauri/src/runtime.rs)**
   - `RuntimeState` 新增 `External` 变体(`as_str() = "external"`;`is_live()` 天然为 false——它不是托管态)。
   - `RuntimeStatus::unmanaged(asset_id, listening)` 新公开构造器:`listening=true` → external,否则 → stopped。external 恒无 pid/started_at/error。
   - `LocalServiceRuntime::holds(asset_id)` 新增:区分"注册表有条目(含已结束)"与"从无条目"——前者必须原样报告托管状态。
   - `loopback_port_listening` 由私有改为 `pub(crate)`,文档注释更新:探活现在有两个只读用途(拒绝双开 + external 显示)。
   - `status_of` 的 pid 匹配补 `External => None`;模块头注释同步说明 External 是读时观察、非托管生命周期。

2. **[apps/desktop/src-tauri/src/commands/service_runtime.rs](apps/desktop/src-tauri/src/commands/service_runtime.rs)**
   - `service_runtime_status_impl`:先查 `holds()` → 有条目走原托管路径;无条目走合成。
   - `service_runtime_statuses_impl`:在托管条目之外,列出 DB 中 active 的 local 服务(`ServiceFilter{ service_type: Local }`),排除已持有者,逐条合成。轮询路径的额外开销 = 每条一次 loopback `TcpStream::connect`(微秒级)。
   - `probe_unmanaged_endpoint` / `unmanaged_endpoint_listening`:active + local + loopback endpoint 三重门;任何读错误降级为"未监听"。

### 前端(React/TS)

3. **[apps/desktop/src/features/library/types.ts](apps/desktop/src/features/library/types.ts)**:`ServiceRuntimeState` 联合类型加 `'external'`(带语义注释)。
4. **[apps/desktop/src/features/library/ServicesWorkspace.tsx](apps/desktop/src/features/library/ServicesWorkspace.tsx)**
   - `stateTint.external` 用 attention 色(信息性提示,区别于 running 的品牌色)。
   - "打开页面"按钮条件从 `running` 扩为 `running || external`。
   - external **不显示**启动按钮(后端必拒,不给注定失败的按钮)、**不显示**停止按钮。
   - 详情面板新增 `service-external-note`:"该实例由 AssetMesh 外部启动,无法在此停止。"
5. **[apps/desktop/src/i18n/zh.ts](apps/desktop/src/i18n/zh.ts)**:新增 `'external': '外部运行中'` 与注释文案两个键。

### 测试

6. **[apps/desktop/src-tauri/tests/desktop_service_runtime_contracts.rs](apps/desktop/src-tauri/tests/desktop_service_runtime_contracts.rs)**
   - 新增 `unmanaged_services_report_external_from_the_port_probe_only`:覆盖 ①未托管+端口应答→external(单条+列表,无 pid)②释放端口→stopped ③saas 带应答的 loopback 地址→仍 stopped(类型门)④托管条目优先:托管运行结束后外部实例再占端口,仍报托管 stopped。
   - 更新两处旧断言(原为 `statuses.is_empty()`):语义改为"该记录以未托管(stopped、无 pid)出现"。见 `missing_directory...`( refusal 后无托管条目)与 `launch_configuration_persists...`(重开后无自动启动)。
7. **[apps/desktop/src/features/library/ServiceWorkflow.test.tsx](apps/desktop/src/features/library/ServiceWorkflow.test.tsx)**:新增用例——fake transport 种 `external` → 行与详情均显 external 徽章、无启停按钮、有打开按钮、详情含边界说明。

### 文档(与代码同步纠正)

8. **[docs/12-desktop-contract.md](docs/12-desktop-contract.md)** "Local-service runtime" 章节:
   - 原句 "there is no port/name/PID scanning" 已不准确,改为"探活仅用于两个只读用途(拒双开 + external 显示),probe 从不 signal"。
   - States 列表补 `external` 及其不变量(读时重算/不落盘/无 pid/无停止/托管条目优先)。
9. **[docs/10-services-subscriptions-v1.md](docs/10-services-subscriptions-v1.md)** "temporary runtime information" 段落补 `external` 观察的定义与边界。

## 4. 测试证据

- `cargo test -p assetmesh-desktop`:**14 个套件全部通过**(含 runtime contracts 18 个用例,fmt 后复跑仍绿)。
- `npx vitest run`(apps/desktop):**137 个用例全部通过**(含新增 external UI 用例与 i18n 字典校验)。

## 5. Review 时建议重点看的风险点

1. **轮询路径变重**:`service_runtime_statuses` 现在每次轮询做一次 DB 读 + N 次 loopback connect。N = active local 服务数,本机场景个位数;但若未来服务数大,可加探活结果短 TTL 缓存(未做,避免引入陈旧显示)。
2. **读错误降级**:`probe_unmanaged_endpoint` 出错时静默降级为 stopped(见代码注释)。理由:显示观察、"无托管进程"恒真;但如果你认为 DB 错误应上抛,这里是需要拍板的点。
3. **端口语义**:端口有响应只证明"有东西在听",不严格等于"就是这个服务"。~~UI 文案已留余地~~(首轮此说法不准确:提示文案曾确定地声称实例由外部启动;第二轮已修正为只陈述两个可证事实,见下)。
4. **既有断言的语义更新**:两个旧测试的 `statuses.is_empty()` 改成了显式未托管断言,请确认新语义表述符合你的意图。

## 6. 第二轮修复记录(2026-10-02)

首轮 review 结论:3 个 P2 + 1 个 P3。逐条修正如下:

### P2-1 探活无超时 → 每次连接限时 + 整轮预算

- `loopback_port_listening`(阻塞式 `TcpStream::connect`)替换为 `loopback_target_listening`:每次连接用 `TcpStream::connect_timeout`(单次 300ms,常量 `PROBE_ATTEMPT_TIMEOUT`),监听队列满导致的 SYN 丢弃不再能拖住状态读取。
- `service_runtime_statuses_impl` 增加整轮预算 `STATUS_PROBE_ROUND_BUDGET`(1500ms):预算花完后,剩余未托管记录一律按"未监听"报告(探活只是精化显示,"AssetMesh 无托管进程"恒真)。单条 status 读用 `deadline=None`,由单次超时兜底。
- 测试:`a_spent_round_budget_counts_as_not_answering`(预算过期的探活对真实监听者也返回 false)。满队列导致的真实挂起无法在单测中稳定复现,以超时常量 + 预算门测试覆盖。

### P2-2 手工截取 URL 解析失败 → 改用真 URL 解析器

- `parse_local_endpoint_port` 删除,替换为 `parse_loopback_target`,内部用 `tauri::Url`(即 `url::Url`)解析。查询串(`?x=1`)、fragment、大写 `LOCALHOST`、带路径的地址全部正确解析;非 HTTP(S) scheme、无显式端口仍返回 `None`(与原行为一致)。
- 测试:`parses_loopback_probe_targets_from_access_addresses` 覆盖 query/fragment/大小写/默认端口缺失/ftp 等。

### P2-3 无条件探双栈,探测地址可能非配置地址 → 显式 IP 只探该地址

- 新类型 `LoopbackTarget`(`Localhost(u16)` / `Address(IpAddr, u16)`):解析出的显式 loopback IP 把探活钉在**恰好那个地址**上;只有 `localhost` 允许尝试 127.0.0.1 与 ::1 两栈。
- 启动冲突拒绝路径同样受益:`load_launch_config` 返回 `LoopbackTarget`,`start` 签名随之调整;拒绝报错按目标显示 `local address {target} is already in use`(Display 实现)。
- 测试:`a_probe_answers_only_for_the_configured_address`——IPv6-only 监听者对显式 IPv4 目标不可见(反之亦然),`localhost` 两栈均可命中。这正是 review 中"页面显示 external 但打开页面连不上"的根因测试。

### P3 单条/列表探活逻辑重复 → 共用 helper

- 提取 `unmanaged_read(asset_id, lifecycle, service_type, endpoint_url, deadline)`:`service_runtime_status_impl`(单条,deadline=None)与 `service_runtime_statuses_impl`(列表,deadline=整轮预算)都经由它,资格判断(active + local + loopback endpoint)只写一遍,后续语义不会漂移。

### 附带修正:review 反馈的文案问题

review 指出首轮文档"UI 文案已留余地"的说法与实际不符——原提示"该实例由 AssetMesh 外部启动,无法在此停止"确实下了确定结论。已改为只陈述两个可证事实:

> Something is answering at this address, but AssetMesh did not start it and cannot stop it here.
> (该地址有服务在响应,但不是 AssetMesh 启动的,也无法在此停止。)

"AssetMesh 没启动它"来自注册表事实(确定);"有服务在响应"来自探活(观察事实);不再断言响应者就是该服务的实例。徽章"外部运行中"保留(短标签),细节由提示承载。

### 第二轮测试证据

- `cargo test -p assetmesh-desktop`:通过(runtime contracts 18 用例；新增 3 个探活用例属于 `runtime.rs` 单元测试，不能计入这 18 个契约用例)。
- `npx vitest run`:137/137 通过(含更新后的文案断言;i18n 字典校验同步)。

### 第二轮改动文件

在首轮 9 个文件基础上,涉及:`apps/desktop/src-tauri/src/runtime.rs`(探活重构)、`apps/desktop/src-tauri/src/commands/service_runtime.rs`(签名适配 + helper + 预算)、`apps/desktop/src/i18n/zh.ts` 与 `apps/desktop/src/features/library/ServicesWorkspace.tsx`(文案)、`apps/desktop/src/features/library/ServiceWorkflow.test.tsx`(断言)、本 REVIEW 文档。

## 7. 第三轮修复记录(2026-10-02)

### 显式默认端口

URL 库会把显式 `http://127.0.0.1:80` / `https://localhost:443` 的端口归一化为 `None`，使第二轮状态探测和启动冲突检查漏掉它们。现在先用 URL 库验证并解析主机；只有归一化丢掉端口时，才从原始 authority 恢复显式默认端口标记。未填写端口仍不探测，query/fragment 中的端口文字不会被当作 authority 端口。回归覆盖 `:80`、`:443`、零填充端口、IPv6、大小写、query/fragment，以及隐式端口排除。

### 公平预算与未观察的处理

列表先完成 SQLite 读取，再开始 1500ms 探活预算。每条记录获得“剩余时间 / 剩余记录数”的时间份额；`localhost` 再将自身剩余份额分给剩余 IPv4/IPv6 目标。各次连接取 300ms 与剩余份额的较小值，使前面的阻塞监听者不能消耗后面的全部探测机会。

预算不足以开始一次连接时，探活返回 `unavailable`，单条/列表命令传播错误；不再把“没有探测”转换为 `stopped`。前端既有轮询逻辑保留上一轮成功快照，并在恢复后更新。数据库、无效 ID 等读取错误也传播。运行事实仍然只在内存中读取，不新增缓存、落盘状态或运行状态变体。

1500ms 限制的是连接等待预算，不能保证含数据库读取、线程调度和返回结果处理的整条命令精确在 1500ms 内完成。

### 真实回归证据

- 新测试先在修复前运行并失败：显式 `:80` 返回 `None`；剩余 40ms 的连接仍等待约 301ms；五个满队列监听者使后面的正常监听者在第一轮显示 `stopped`。
- 满队列通过真实本地监听 socket 和保留的连接构造。修复后覆盖剩余等待预算、IPv4 阻塞时 IPv6 仍有机会，以及 SQLite + 命令层连续两轮均正确报告最后的正常监听者为 `external`。
- 新增命令层测试确认过期预算返回错误；新增前端轮询测试确认失败时保留 `external`，恢复后的真实 `stopped` 可以正常替换它。
- 全量前端第一次运行还发现关系页测试在 lazy explorer 加载完成前同步断言。该断言改为等待 `relation-explorer` 出现，产品代码没有因此修改。

### 本轮改动文件

`runtime.rs`、`commands/service_runtime.rs`、`desktop_service_runtime_contracts.rs`、新增测试辅助文件 `tests/support/loopback.rs`、`ServicePolling.test.tsx`、`RelationWorkflow.test.tsx`、`docs/12-desktop-contract.md` 和本 REVIEW 文档。

### 第三轮最终验证

- `cargo test -p assetmesh-desktop`: **111 passed / 0 failed**。其中库单元测试 13 个(运行时单元测试 8 个)，运行时契约测试 19 个。
- `npm test`(apps/desktop): **21 个文件、138 passed / 0 failed**。
- `npm run typecheck`、`npm run lint`、`npm run build` 全部通过。
- `cargo clippy -p assetmesh-desktop --all-targets --all-features` 无警告通过；修改的 Rust 文件 `rustfmt --check` 通过；`git diff --check` 通过。
- 当前结论：本轮两项 P2 和预算上限 P3 已修复；源码和前端生产构建已验证。安装中的桌面应用尚未替换，需要按原有编译替换流程生效。

## 备注:工作区中的另一组独立改动

同一工作区还有本次会话早前完成的 **CLI 本地服务启动字段补齐**(与本改动无关,属于 CLI/桌面端能力对齐):
`crates/cli/src/commands/service.rs`(`service add|update` 新增 `--project-dir` / `--start-command`)、`crates/cli/src/format.rs`(详情打印两行)、`crates/cli/tests/cli_e2e.rs`(新增 `local_service_launch_metadata_round_trips`)。CLI 相关测试 5 个全通过。
