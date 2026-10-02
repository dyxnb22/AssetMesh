# 全项目 Code Clean 与代码坏味道审查

日期：2026-10-03。审查基于本轮开始时的完整工作区，包括已有未提交修改，而不是仅比较 HEAD。已有修改已保留；下面的“已修复”只指本轮新增的改动。

## 审查结论

项目的 domain / application / ports / SQLite / desktop adapter 分层总体成立。主要问题集中在几个入口模块：资产详情页拥有多个业务模块的编辑状态，应用壳拥有启动恢复和各类弹窗状态，portable 模块同时拥有格式、文件操作和导入策略，本地服务 runtime 同时拥有探测、日志和进程生命周期。

这些模块的风险不仅是文件长：修改某个业务字段、失败处理或资源策略，会触及与它无关的状态和实现细节。本轮优先拆开这些职责，同时修复实际发现的异步状态错误、字段清空错误和导入校验复杂度问题。

两批修改已落地。第一批处理详情编辑会话、应用壳、portable 与 runtime；续轮处理可空字段更新、传输契约、旧媒体导入、备份适配器、无用依赖及列表和导入导出展示。下面分别记录原始问题、处理结果与保留的边界。

## 范围与判断方法

- 扫描 desktop 前端、Tauri commands / DTO / runtime、core domain / application / ports、SQLite repositories / migrations / backup、providers、CLI，以及相关测试和项目架构文档。
- 对高复杂度模块深入阅读职责、状态所有权、调用边界、事务边界和错误分支；从 UI 命令跟踪到应用服务及持久化，再结合既有测试验证契约。
- 检查重复逻辑、过长函数、布尔状态组合、无用依赖、弱类型边界、隐式失败、重复线性扫描和异步结果串线。文件行数只用于定位候选，不单独作为上帝类的判据。
- 审查包括全项目层级扫描与重点模块深读，并非每行代码都经过人工逐行检查。既有 `docs/15-design-and-growth-audit.md` 中已落地的预算、备份、发现命令和维护改进属于审查基线，不重复算作本轮成果。

优先级：P1 表示可能造成明显的可用性问题；P2 表示用户可见的行为缺陷或显著的维护风险；P3 表示局部清洁度问题。结构问题的优先级不等同于安全漏洞等级。

## 发现总表

| 编号 | 优先级 | 问题 | 本轮状态 |
| --- | --- | --- | --- |
| R01 | P2 | 资产详情组件拥有媒体、软件、订阅、续费的草稿、提交和展示职责 | 已拆分 |
| R02 | P2 | 切换资产时，前一个资产的迟到保存结果可覆盖当前详情 | 已修复并加回归测试 |
| R03 | P2 | 冲突重新加载没有一致地清除订阅编辑和续费草稿 | 已修复并加回归测试 |
| R04 | P2 | 归档后的详情仍可能保留可编辑草稿；同一表单连续提交缺少同步锁 | 已修复并加回归测试 |
| R05 | P2 | portable 同时承担 V1 编码、文件锁与替换、预检、目标冲突和事务导入 | 已按职责拆分 |
| R06 | P1 | portable 详情查找反复扫描资产；重定向链反复遍历且每跳线性查找 | 已改为索引和共享路径检查 |
| R07 | P2 | runtime 注册表直接操作进程状态和日志缓冲内部字段 | 已拆分并封装 |
| R08 | P2 | App 混合启动恢复、偏好同步、页面组合和多个独立弹窗布尔状态 | 已拆分并统一弹窗状态 |
| R09 | P2 | 软件可选文本更新混淆“未提供”和“明确清空” | 已修复前端与应用服务 |
| R10 | P3 | 详情表单样式、媒体 mutation 流程和搜索关键词投影重复 | 已提取共同逻辑 |
| R11 | P2 | Rust / TypeScript 传输类型重复维护，已知模块在桥接层退化为动态 JSON | 已改为 typed DTO、入口校验及共享契约样例；类型仍手工声明 |
| R12 | P2 | 媒体及部分订阅字段缺少完整的 preserve / clear / set 更新语义 | 已统一三态 Patch，并验证 UI / JSON / core / SQLite |
| R13 | P2 | 旧媒体导入的匹配、规划、应用和规范化集中在一个模块 | 已按职责拆分 |
| R14 | P2 | SQLite 备份模块混合清单验证、快照、恢复选择、保留和偏好职责 | 已拆分，写入锁由 facade 持有 |
| R15 | P3 | AssetService / SearchService 注入并持有未使用依赖 | 已删除字段、构造参数和调用方假依赖 |
| R16 | P3 | 列表和导入导出视图存在大量重复展示代码与内联样式 | 已提取列表行、独立工作流、统计表和重复样式 |

## 首批已落地修改

### 1. 资产详情：让业务模块拥有自己的编辑会话

入口 `apps/desktop/src/features/library/AssetDetailView.tsx` 从 1,915 行降至 596 行。剩余职责是读取详情、归档、反馈以及标签、关系、活动的组合。

新增 `features/library/detail/`：

- `SoftwareDetailEditor.tsx`、`MediaDetailEditor.tsx`、`ServiceDetailEditor.tsx` 分别拥有本模块草稿、字段和命令构造。
- `ServiceRenewalEditor.tsx` 单独拥有续费表单；订阅编辑器使用 `view / edit / renewal` 模式，消除互斥操作由多个布尔值组合表示的问题。
- `DetailEditorForm.tsx` 与 `DetailEditor.css` 复用表单外壳和样式，字段仍由各模块定义。
- `useDetailMutation.ts` 统一保存锁、错误反馈和“命令成功 → 读取 canonical 详情 → 通知刷新”的顺序。同步 busy ref 阻止同一事件周期的重复提交；保存或读回失败时保留草稿，不自动重试。

详情会话以资产 ID 为 React key。保存期间切换资产，旧回调只属于旧会话，不能替换新资产的详情；已完成的保存仍会通知父层刷新列表。冲突重新加载和归档生命周期变化会重建编辑器，避免继续使用旧草稿。未知未来模块的展示回退继续保留。

新增回归测试覆盖订阅编辑冲突、续费冲突、切换资产后的迟到保存、连续提交和归档时关闭草稿。既有保存失败、读回失败和列表刷新测试继续通过。

### 2. Portable：分离格式、文件机制与导入策略

`crates/core/src/application/portable.rs` 从 2,568 行变为 68 行的文档和公有入口，原来的公有类型、函数和服务仍从原路径导出。

内部按语义拆为：

| 模块 | 职责 |
| --- | --- |
| `format.rs` | V1 DTO、领域转换、分区词汇和导出过滤 |
| `budget.rs` | 包、文件、记录等读取和写入预算 |
| `bundle_io.rs` | 有界读取、私有文件、锁、暂存和可恢复替换 |
| `validation.rs` | manifest、解码、唯一性、引用和重定向预检 |
| `destination.rs` | 目标库快照、已有身份和冲突判定 |
| `import.rs` | 导入报告及事务编排 |
| `streaming.rs` | 逐条导出到内存或目录 |

原来单个大型预检函数拆为 manifest、读取分区、解码、身份和引用校验阶段。阶段接口使用内部类型，没有新增插件系统或通用导入框架。

性能修复针对两处真实重复扫描：

- 模块详情逐条执行 `assets.iter().find(...)`，最坏是平方级工作量。现在复用资产 ID 索引。
- 每个资产重新遍历它的完整重定向链，每跳又线性查找目标；长链最坏可达到立方级工作量。现在使用资产索引、当前路径集合和已检查集合，共享尾链只检查一次，预期线性复杂度且不依赖递归栈。

新增公开导入契约测试：4,000 个身份的重定向链可成功 dry-run；深层尾部循环在 dry-run 和正式提交中都被拒绝，目标库保持空。该测试验证正确性和深链处理，本轮未做旧版与新版的耗时基准比较。

V1 文件形状、旧模块未声明时的兼容规则、预算、私有权限、文件锁和事务提交边界保持原有契约。

### 3. 本地服务：封装进程状态与日志

`apps/desktop/src-tauri/src/runtime.rs` 从 1,350 行降至 535 行。注册表保留启动、停止、状态查询和退出协调，内部拆为：

- `runtime/probe.rs`：loopback 地址解析及探测。
- `runtime/logs.rs`：流解码、单行截断、增量游标和缓冲预算。
- `runtime/process.rs`：托管进程状态、信号、观察、回收以及结束进程保留。

`SharedProcess` 字段变为私有，由 `state / status / logs / request_stop / set_configured_stop` 等行为接口管理；注册表不再直接读写它的 Mutex 字段。流读取只依赖日志缓冲，不需要访问完整进程对象。日志预算和快照由 LogBuffer 自己维护。

保留既有公有 runtime 类型和地址解析入口，以及原来的退出、信号、日志保留行为。原有单元测试随职责一起移动，桌面 runtime 契约测试通过。

### 4. 应用壳：启动会话、恢复界面与页面组合分开

`apps/desktop/src/app/App.tsx` 从 838 行降至 526 行：

- `useDesktopSession.ts` 拥有启动状态、能力读取、初始化重试、恢复标记、偏好恢复和同步。
- `StartupScreen.tsx` 拥有加载、初始化失败、损坏库和恢复入口的展示。
- App 保留导航、工作区、列表和弹窗组合；七个创建弹窗布尔值变为一个 union 状态，表示一次打开哪一个弹窗。

发现并修复系统主题只在 effect 执行时采样的问题：选择 system 时监听系统外观变化，显式选择 light / dark 时移除监听。新增工作流测试验证两种行为。偏好延迟恢复不覆盖用户新选择的既有测试继续通过。

### 5. 统一小型重复逻辑，修复字段更新语义

`useMediaStatusActions.ts` 的状态转换和进度更新共用一个内部 mutation 流程，集中处理同步锁、canonical 读回、作用域变化、冲突刷新和反馈；保留原撤销条件。

`projection.rs:267` 的 `asset_keywords` 集中处理标签 trim、大小写去重和外部引用别名。媒体、软件、服务及信息投影继续拥有各自的正文和模块关键词。

`software_service.rs` 的六个可选文本字段原来通过“规范化后是 Some 才赋值”更新，明确传入空字符串也会被忽略。首批修复区分未提供和明确清空，续轮进一步统一为下文的 `Patch<T>`，删除临时的单模块 helper：

- 不提供字段：保留原值。
- 提供空串或纯空白：清除可选文本。
- 提供非空字符串：按已有验证规则更新。

涉及 version、install_location、executable_path、purpose、notes、architecture。软件编辑器现在明确发送 `null` 清空 purpose / notes。新增前端及 core 用例验证清空成功、未编辑的版本和路径保留、revision 推进以及删除的用途不再进入搜索投影。

## 续轮已落地修改

### R11：传输层的弱类型边界与重复契约

位置：`apps/desktop/src-tauri/src/dto/details.rs`、`dto/mod.rs`，`apps/desktop/src/features/library/wire.ts`、`types.ts`、`transport.ts`。

原始问题：core 的模块详情是 typed union，但桌面详情和列表 DTO 使用动态 JSON，序列化失败被回退为正常 unknown / null 数据；TypeScript 手工声明另一套类型，字段演进缺少两端验证。审查没有复现现存的序列化失败，不把这些 fallback 表述为已发生的数据损坏。

处理结果：

- 新增 `AssetDetailsDto` tagged enum，媒体、软件、服务、信息和合并重定向均使用具体类型。详情、列表及重复项 evidence 移除吞掉序列化错误的动态 fallback，JSON 的既有模块标签保持兼容。
- TypeScript 的重复项 evidence 改为具体 union；媒体进度字段的 nullability 与 Rust 统一，软件 install_source 收窄为可选文本。
- `getAsset`、`listAssets`、`searchAssets`、`duplicateCandidates` 在 native bridge 接收 `unknown`，验证字段形状后再进入 UI 状态。已知模块畸形数据报 unavailable；未来未知模块保留原 payload 与原模块名，继续只读展示。
- Rust 和 TypeScript 共用 `src/test/desktop-wire-fixtures.json`。Rust 反序列化、领域验证并原样序列化六种详情样例；前端读取同一组样例，并验证错误类型和未来模块。字段 guard 使用 mapped type，新增 TS 字段时必须补齐校验规则。

保留边界：TypeScript 类型仍由人维护，没有引入代码生成器；共享样例和字段校验降低漂移风险，不等于所有 DTO 都实现了跨语言自动生成或运行时验证。这里校验传输形状，业务合法性继续由 core 判断。DTO 注册文件不因长度较大就被判为上帝类。

### R12：统一可空字段的三态更新

位置：`crates/core/src/application/patch.rs`，媒体 / 软件 / 服务 command DTO、应用服务、CLI 和详情编辑器。

原始问题：媒体及部分订阅编辑器把删空的值转成 undefined，应用服务把缺省理解为保留，导致删除后保存原值又回来。普通 `Option<T>` 还会在 JSON 反序列化时合并缺省和 null。

现在共用 `Patch<T>`：

| Wire 输入 | Core 表达 | 行为 |
| --- | --- | --- |
| 不提供字段 | `Leave` | 保留现值 |
| `null` | `Clear` | 清除可选值 |
| 具体值 | `Set(value)` | 按既有领域规则更新 |

DTO 使用 default 与跳过 Leave 序列化，避免遗漏字段被意外输出成 null；错误地直接序列化 Leave 会返回错误。原服务模块的 Patch 导出路径保留。空字符串及纯空白仍支持清空可选文本，以兼容 CLI 和旧桌面调用者。

媒体、软件和服务的可空更新字段使用同一协议；金额与货币仍必须一起更新或清空，单边修改被拒绝且 revision 不推进。本地服务运行期间，明确 null 清空启动配置也必须经过原有冻结检查。

详情编辑器明确发送 null 清空可选字段；标题是必填字段，删空时发送空文本，让后端明确拒绝并保留草稿，不再静默忽略。CLI 增加媒体 `update --clear-year`，与 `--year` 互斥；省略其他字段仍保留原值。

新增测试覆盖真实 JSON command → core → SQLite 的缺省、null、具体值与 paired money 校验，前端媒体 / 软件 / 订阅清空及拒绝后草稿保留，以及真实 CLI 的清空和互斥参数。真实托管进程契约也验证运行时 null 清空 project_dir / start_command 被拒绝且 revision 不推进。

### R13：旧媒体导入只编排阶段

原 980 行的 `import_media.rs` 同时拥有规范化、匹配索引、规划和批次写入。现在入口为 196 行，保留公有报告类型和 `MediaImportService::import`：

- `import_media/normalize.rs`：输入校验与规范化，删除未使用的 now 参数。
- `matching.rs`：私有 MatchIndex，拥有身份优先级、已规划身份和冲突报告。
- `apply.rs`：一个批次的事务应用、create / update 和进度合并策略。

canonical ID / exact reference 优先级、启发式匹配仅报告冲突、同一批次后续行匹配、每 200 条一个事务以及部分提交报告均保留。它继续使用旧媒体导入契约，没有改为 portable 的整包事务语义。既有 core / CLI 导入测试全部通过。

### R14：备份适配器封装各自的文件机制

`crates/storage-sqlite/src/backup.rs` 从 943 行降为 117 行的 facade，保留 `SqliteBackupStore`、备份目录和选中数据库入口：

| 私有模块 | 职责 |
| --- | --- |
| `files.rs` | 有界 JSON、私有权限、原子文件、checksum、空间和文件锁 |
| `database.rs` | online copy、只读连接、数据库检查及 change token |
| `manifest.rs` | 旧清单兼容、header / payload 校验、fingerprint 与发布 |
| `inventory.rs` | 清单扫描、损坏项披露和稳定排序 |
| `create.rs` | 快照、portable 恢复点及迁移前备份 |
| `recovery.rs` | 源指纹复查、新库验证、恢复选择和复制 |
| `retention.rs` | 保留策略与暂存 / 恢复库清理 |

facade 为一次写操作持有同一把锁，子模块不重复加锁；恢复过程中复制、再次校验源、验证新库、写选择标记和旧库禁写的顺序保持。V1 清单、旧 fingerprint 兼容和保留规则保持原契约。16 项 SQLite 备份契约测试通过，覆盖 WAL 快照、旧清单、损坏备份、恢复切换和保留。该项解决职责和维护风险，没有宣称发现新的备份损坏案例。

### R15：删除无用构造依赖

`AssetService` 删除从未使用的 ID generator，构造函数只接收 factory 和 clock；`SearchService` 删除从未使用的 clock，只接收 factory。两处 `allow(dead_code)` 及 CLI 的中间无用参数同步删除，desktop、CLI 和测试构造点均已更新。公有 Rust 构造签名因此简化；这项修改不属于完全不变的接口承诺。

### R16：收敛实际重复的展示与状态职责

- `AssetLedger.tsx` 从 942 行降为 765 行，`LedgerRow.tsx` 拥有单行展示、键盘选择 / 打开和标签交互，稳定样式移到 `LedgerRow.css`；工具栏、查询、排序、分页和选中资产仍属于列表入口。
- `ImportExportView.tsx` 从 830 行降为 85 行，只组合标题、标签页和两种工作区。导出 / 导入分别拥有 `useExportBundle` / `useImportBundle` 和各自的展示组件。
- 两个工作流 hook 始终挂载，切换标签仍保留路径、导出选项、预检和收据；新增测试实际切换标签后继续应用已检查的 source / fingerprint。
- `ImportDispositions` 使用六行固定展示数据消除重复 JSX，反馈卡片、目录输入、统计布局和标签样式集中到 `PortableWorkspace.css`。

没有引入通用表单框架、可配置 UI 引擎或假想复用组件。部分一次性与动态样式继续就地表达；本项是对实际重复的定点清理，不承诺所有 JSX 或 inline style 都消失。

## 明确保留的结构

- UnitOfWork 同时暴露多个 repository 是跨资产、模块、活动和搜索投影的原子提交需要；QueryUnitOfWork 的只读快照接口有实际 SQLite 和内存实现。本轮没有把这些事务能力拆成独立提交。
- LibraryService 的跨模块分派属于统一查询边界，关系查询服务的遍历预算属于关系查询职责；没有仅因文件较长而把它们标为上帝类。
- CLI / Tauri 的命令适配层，以及 providers 与 core 的接口有真实边界和替代实现，不因“只是转发一次调用”就删除。
- Portable V1 DTO 与领域类型分开是兼容性边界；本轮重构保持这层隔离，也保留未知模块回退。
- 现有测试主要验证应用契约、事务、恢复和用户流程，本轮没有新增仅固定文件行数、组件拆分形状或 CSS 常量的测试。

## 验证结果与限制

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace` | 588 passed，0 failed，4 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `npm test` | 23 个测试文件、165 项测试通过 |
| `npm run typecheck` | 通过 |
| `npm run lint` | 通过 |
| `npm run build` | 通过 |
| `git diff --check` | 通过 |

新增测试覆盖真实状态与数据契约。Rust 的 4 个 ignored 项包含 3 个发布规模测试，以及由测试父进程按需调用的 listener 子进程入口；不把它们全部称为规模测试。真实 Tauri WebDriver E2E 需要 Linux，本轮 macOS 环境未运行；也未运行 50k 发布规模基准。

本轮没有修改数据库 schema 或操作真实资料库，也没有安装、发布或替换桌面应用。验证范围是当前源码、测试及前端生产构建。

## 后续演进边界

本次列出的结构拆分与行为修复已完成。跨语言类型仍手工声明，是明确保留的维护边界；增加新模块或扩展 DTO 时，应同步 typed DTO、前端 guard 和共享样例。如果未来需要覆盖所有命令与 DTO，再评估统一类型生成，不为当前几个稳定模块预先引入新的框架。

后续清理应以实际变更造成的职责耦合、重复逻辑或行为问题为依据，继续保留事务、文件格式和恢复生命周期的契约；不把文件行数或样式是否内联当作单独的重构目标。
