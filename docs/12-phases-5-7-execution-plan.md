# Phase 5–7 产品、架构与执行手册

状态：**Phase 5 完成（含 R1–R6 remediation 闭环），Phase 6 待启动**
适用范围：Phase 5 Desktop（已完成）、Phase 6 Runtime Enrichment（待启动）、Phase 7 Personal Modules
目标读者：开发者，以及上下文较短、推理能力有限但能可靠执行明确任务的编码模型

> 本文不是当前实现状态声明。Phase 状态仍以 `docs/07-roadmap.md` 为准；Phase 4 的完成条件仍以
> `docs/11-unified-library-core.md` 为准。Phase 4 未通过全部退出条件前，不得开始本文的实现任务。

---

## 1. 如何使用本文

本文把 Phase 5–7 拆成可独立验证的小任务。执行模型每次只领取一个任务 ID，不得同时实现后续任务。

每次开发按以下顺序进行：

1. 阅读本节、目标 Phase 的总则，以及当前任务卡。
2. 阅读任务卡列出的现有文件；通过 `rg` 确认真实接口和命名，不从本文猜测已经变化的代码。
3. 检查 `git status --short`，保留用户已有修改。
4. 先添加或修改能够证明任务行为的测试，使缺失行为可观察。
5. 实现满足测试所需的最小完整纵切，不顺带搭建后续 Phase。
6. 运行任务卡的局部验证，再运行仓库级质量门禁。
7. 对照任务卡的“完成条件”逐项报告；任何一项不成立，任务保持未完成。

### 1.1 固定阅读顺序

所有任务先读：

1. `README.md`
2. `docs/02-architecture.md`
3. `docs/03-domain-model.md`
4. `docs/04-foundation.md`
5. `docs/05-storage-and-portability.md`
6. `docs/06-module-system.md`
7. `docs/07-roadmap.md`
8. `docs/11-unified-library-core.md`
9. 本文中当前 Phase 和当前任务卡

改动 SQLite、portable export、module schema 或 canonical data 时，再读相关 ADR 和已完成模块规格。
改动桌面视觉时，先读本文第 6 节。改动 provider/runtime 时，先读第 7 节。新增领域模块时，先读第 8 节。

### 1.2 每次只允许一种主变化

一次任务只能有一个主要目的：

- 新增一个 application interface；或
- 新增一个 adapter；或
- 新增一个页面纵切；或
- 新增一个持久化版本；或
- 完成一个质量/迁移门禁。

当一个任务同时要求新的 canonical schema、新 provider、新页面和写操作时，任务拆分错误，应停止并重新切分。

### 1.3 完成报告模板

执行模型完成任务后，用以下格式交付：

```text
任务：P5-xx / P6-xx / P7-xx
结果：complete | incomplete

实现：
- <实际完成的行为>

验证：
- <命令> — PASS/FAIL

未完成：
- <没有完成的条件；没有则写“无”>

范围外观察：
- <只记录，不顺手修改>
```

“编译通过”“页面能打开”“写了代码”都不是完成条件。完成条件必须是可观察行为和对应测试同时成立。

---

## 2. Phase 5–7 的产品主线

AssetMesh 的中心仍是耐久资产、明确身份及资产之间的关系。Phase 5–7 不把它变成系统监控面板、GitHub
客户端、知识库数据库、Skill 市场或通用自动化平台。

```text
Phase 4: 一个稳定的 headless 资产库 interface
    ↓
Phase 5: 一个忠实使用该 interface 的桌面应用
    ↓
Phase 6: 对已有资产附加可丢弃、可解释的运行时观察
    ↓
Phase 7: 用真实个人工作流扩展 Project、Agent Capability、Knowledge 模块
```

### 2.1 三个 Phase 的唯一核心问题

| Phase | 必须回答的问题 | 不回答的问题 |
| --- | --- | --- |
| 5 | 用户能否通过桌面应用完整操作 Phase 4 已有能力？ | 新资产领域、实时监控、插件系统 |
| 6 | 哪些已有资产现在可用、异常或需要关注，证据是什么？ | 通用运维、自动修复、进程管理器 |
| 7 | Project、Skill 和 Knowledge Collection 如何成为耐久资产并参与统一关系？ | GitHub 全功能客户端、Skill 市场、知识原子数据库 |

### 2.2 从 Hermes 扩展吸收什么

迁移的是已经证明的领域语义、interface 和验收案例，不是整仓复制 Python/JavaScript 实现。

| 来源 | 吸收 | 保持外部或停止 |
| --- | --- | --- |
| Personal Command Center | 固定 Inspector、诚实空态、approval/receipt 交互经验 | CPU/内存/电池、Mihomo、AI 配额、Hermes session 控制 |
| Extension Workspace | Source/Deployment/Core 关系、Observation、Finding、owner adapter 纪律 | 独立 Desired State/Plan/Apply 控制平面 |
| GitHub Cloud Explorer | 仓库发现、稳定 locator、本地 checkout 匹配、安全 `gh` adapter | PR/Issue/Review/Checks/日志浏览器 |
| Skill Control Center | Skill identity、provenance、occurrence、activation、collision、`observe`/`explain` | 市场、通用安装器、未经证明的 orchestrator |
| Hermes Obsidian | collection 状态、稳定外部 locator、健康摘要 | Source Unit、Atom、Decision、Proposal、Vault 写入 |
| LeetCode Coach | 无直接代码迁移 | 独立 Desktop 插件保持归档 |

---

## 3. 不可破坏的架构合同

### 3.1 依赖方向

```text
React UI
   ↓ invoke / event DTO
Tauri interface adapter
   ↓
Application modules
   ↓
Domain modules + kernel
   ↓
Ports
   ↑
SQLite / OS / Git / gh / launchd / Docker adapters
```

必须满足：

- React 不读取 SQLite，不实现匹配、merge、relation、runtime 判定或 provider 规则。
- Tauri command 只验证 transport 输入、调用 application interface、映射 DTO。
- Application 协调 use case、快照和事务；外部 I/O 不在写事务中执行。
- Domain 不依赖 Tauri、React、SQL、操作系统命令或网络类型。
- Infrastructure adapter 实现 inward-defined port；provider 结果先成为 candidate/observation。
- CLI 和 Desktop 调用同一 application interface；不能各自拥有一套业务行为。

### 3.2 深模块规则

模块应在小 interface 后隐藏足够多的复杂度。评审每个新模块时做 deletion test：

- 删除后复杂性会在多个调用方重新出现：模块有价值。
- 删除后只少了一层透传：模块过浅，应合并。

一个新 seam 至少满足其一：

- 已有两个真实 adapter；
- 第二个 adapter 已在同一 Phase 的已批准任务中；
- seam 隔离不可测试或不可信的外部 I/O，并已有 fake adapter 测试。

### 3.3 Canonical 与 derived 状态

| 数据 | 类型 | 可否删除重建 | 是否 portable export |
| --- | --- | --- | --- |
| Asset、typed details、tags、relations、external refs | canonical | 否 | 是 |
| 用户确认的 purpose、notes、项目角色 | canonical | 否 | 是 |
| SearchDocument / FTS | derived | 是 | 否 |
| provider candidate | advisory | 是 | 否 |
| runtime observation / finding | observed | 是 | 默认否 |
| UI filter、展开状态、窗口尺寸 | presentation | 是 | 否 |
| 外部 Core 的私有实体 | owner-owned | 由 owner 决定 | AssetMesh 不复制 |

任何 derived/observed 数据进入 canonical 前，都必须经过明确 application command 和可审查规则。

### 3.4 事务与并发

- 一次 unified detail/query 在一个 `QueryUnitOfWork` 读快照内完成。
- provider、Git、`gh`、launchd、Docker、文件扫描在事务外执行。
- canonical commit 使用短写事务，并在事务内重新匹配可能变化的 identity。
- 所有桌面写命令携带已有 revision 时使用 optimistic concurrency；冲突返回 typed failure，不静默覆盖。
- Desktop 和 CLI 可以直接复用 SQLite adapter；只有真实 contention 或后台任务需求出现后才评估 daemon。

### 3.5 错误词汇

跨 adapter 使用稳定类别，不把原始 stderr、SQL 错误或秘密返回 UI：

```text
invalid_input
not_found
conflict
stale_revision
setup_required
unavailable
permission_denied
unsupported
timeout
rate_limited
corrupt_data
internal
```

每个错误包含安全的用户说明；可恢复错误包含明确下一步。UI 不从错误文本推断类别。

---

## 4. 目标目录和所有权

Phase 5 开始后建议形成以下结构；任务未开始前不要一次性创建空目录。

```text
AssetMesh/
├── apps/
│   └── desktop/
│       ├── package.json
│       ├── src/                  # React presentation only
│       │   ├── app/
│       │   ├── features/
│       │   ├── ui/
│       │   └── test/
│       └── src-tauri/            # Tauri interface adapter
│           └── src/
│               ├── commands/
│               ├── dto/
│               └── state.rs
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── domain/
│   │       ├── application/
│   │       └── ports/
│   ├── providers/
│   │   └── src/
│   │       ├── runtime/
│   │       ├── git/
│   │       ├── github/
│   │       ├── skills/
│   │       └── knowledge/
│   ├── storage-sqlite/
│   ├── cli/
│   └── desktop-contract/         # 仅在生成 DTO 真正带来收益后创建
├── migrations/
└── docs/
```

所有权规则：

- `apps/desktop/src/features/library` 只能使用 desktop transport interface。
- `src-tauri/commands` 不能访问 repository 或 SQL。
- `crates/providers` 不能写 canonical repository。
- 新领域的 domain、application、portable schema 首先放 `crates/core`；SQLite 实现在对应 adapter 中。
- 共享 UI primitive 放 `apps/desktop/src/ui`；只有两个已完成 feature 使用后才提升为共享 primitive。
- `desktop-contract` 只有在 Rust/TypeScript DTO 漂移被测试证明为实际问题后创建；不得为了目录美观提前建立。

---

## 5. 总路线图和进入门禁

### 5.1 路线图

```text
P5-00 Phase 4 exit audit
  └─ P5-01 Desktop skeleton + transport smoke
      └─ P5-02 Library read slice
          ├─ P5-03 Asset detail + typed domain panels
          ├─ P5-04 Search/filter/navigation
          └─ P5-05 Desktop mutation pattern
              ├─ P5-06 Media/Software/Service workflows
              ├─ P5-07 Relations + impact explorer
              ├─ P5-08 Activity + duplicate review
              ├─ P5-09 Import/export + settings
              └─ P5-10 Desktop hardening and release gate

P6-00 Runtime contract and threat model
  └─ P6-01 Observation + Finding application model
      ├─ P6-02 Local process/port/launchd adapters
      ├─ P6-03 Container runtime adapter
      └─ P6-04 Asset runtime UI
          └─ P6-05 Dogfood and performance gate
              └─ P6-06 Reviewed deterministic actions (optional)

P7-00 Module selection audit
  ├─ P7-01..05 Projects
  ├─ P7-06..10 Agent Capabilities
  └─ P7-11..15 Knowledge Collections
      └─ P7-16 Cross-module hardening and plugin-ABI decision
```

### 5.2 Phase 进入门禁

| Phase | 必须已经成立 |
| --- | --- |
| 5 | Phase 4 全部 exit criteria；CLI 已覆盖 unified library interface；workspace 全绿 |
| 6 | Phase 5 可完整操作已有 canonical 能力；runtime 数据模型 ADR 已批准 |
| 7 | Phase 6 read-only observation 已真实使用；三个候选模块都有真实 owner/数据源和验收样本 |

### 5.3 仓库级质量门禁

所有 Rust 任务结束时运行：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Phase 5 创建 Desktop 后，再增加并固定以下脚本名；包管理器只选择一种并提交 lockfile：

```bash
npm run typecheck
npm run lint
npm run test
npm run build
```

若最终选择 `pnpm`，把上述调用统一改为 `pnpm`，仓库内不得混用 npm/yarn/pnpm lockfile。端到端测试在
P5-10 加入固定命令 `npm run test:e2e`。本文不固定依赖版本号；创建任务使用当时稳定版本并提交精确 lockfile。

---

## 6. Phase 5 — Desktop Application

### 6.1 产品目标

Phase 5 交付 AssetMesh 的主要图形客户端。它是 Phase 4 application interface 的 adapter，不拥有第二套领域逻辑。

成功标准：用户无需 CLI 即可完成 Media、Software、Services、Relations、Activity、Duplicate Review、Import 和
Export 的主要流程；CLI 与 Desktop 对相同输入产生相同领域结果。

### 6.2 技术选择

- Desktop shell：Tauri 2。
- UI：React + TypeScript。
- 构建：Vite。
- Server state：TanStack Query；只缓存 transport 结果，不成为 canonical truth。
- 表单：先使用受控表单和 typed validation；只有三个复杂表单出现重复后才引入表单框架。
- 样式：CSS variables + CSS Modules 或同等的本地作用域方案；不引入整套 dashboard UI kit。
- 图：关系视图先实现 accessible list/path，再选择 React Flow 或等价库；图不是唯一入口。
- 测试：Rust command contract tests、React Testing Library、少量 Playwright 关键路径。

### 6.3 Desktop interface

Tauri adapter 对 UI 暴露按用户任务组织的窄 command。名称可以随实现微调，行为不可下沉到 UI：

```text
library_list(query) -> Page<AssetSummaryDto>
library_get(asset_id) -> AssetDetailDto
library_search(query) -> Page<AssetSummaryDto>

activity_list(query) -> Page<ActivityDto>
relation_neighbors(query) -> RelationNeighborhoodDto
relation_impact(query) -> ImpactDto
duplicates_list(query) -> Page<DuplicateCandidateDto>

media_command(command) -> MutationReceiptDto
software_command(command) -> MutationReceiptDto
service_command(command) -> MutationReceiptDto
relation_command(command) -> MutationReceiptDto
duplicate_command(command) -> MutationReceiptDto

portable_preview_import(path) -> ImportPreviewDto
portable_apply_import(request) -> MutationReceiptDto
portable_export(path) -> ExportReceiptDto
```

规则：

- command DTO 是 closed enum 或 tagged union，不接收任意 command string。
- mutation 返回 receipt 和受影响 Asset ID；UI 随后 invalidates/re-reads。
- `library_get` 返回 typed detail union；UI 使用 exhaustive switch。
- transport DTO 不暴露数据库字段名、SQL null 习惯或内部 repository 类型。
- command handler 不创建散落的 application modules；应用依赖集中在 `DesktopState` 或等价 composition root。

### 6.4 视觉方向：个人数字资产工作台

AssetMesh 应像一张持续维护的个人目录台，而不是 SaaS 指标 dashboard。最鲜明的视觉元素是贯穿 Library 和
Detail 的“关系线”：选中资产后，细线把它与依赖、来源和归属连接起来。其他表面保持安静。

#### 色彩 token

| Token | Light | Dark | 用途 |
| --- | --- | --- | --- |
| `canvas` | `#F3F5F6` | `#15191B` | 窗口背景 |
| `surface` | `#FFFFFF` | `#1D2326` | 主工作面 |
| `ink` | `#172126` | `#E8EEF0` | 主文字 |
| `muted` | `#66747B` | `#98A6AC` | 次要信息 |
| `mesh` | `#147D78` | `#4BC0B6` | 选中、关系和确认 |
| `attention` | `#A86716` | `#E0A34A` | 待审、过期、部分失败 |
| `danger` | `#B63B32` | `#F07167` | 破坏性动作和错误 |

文字使用 macOS 系统字体栈以保持原生感；资产 ID、版本和路径使用系统等宽字体。标题不使用装饰性全大写，也不以
单词着色制造强调。正文行长控制在 80 字符以内。

#### 布局

```text
┌──────────────┬─────────────────────────────────┬──────────────────────┐
│ Library rail │ Collection workspace            │ Inspector            │
│              │                                 │                      │
│ All Assets   │ Search / filter / sort           │ Selected asset       │
│ Media        │ ───────────────────────────────  │ typed details        │
│ Software     │ continuous asset rows            │ relations            │
│ Services     │ grouped only when meaningful     │ recent activity      │
│ Projects     │                                 │ actions              │
│ Knowledge    │                                 │                      │
│              │                                 │                      │
│ Relations    │                                 │                      │
│ Activity     │                                 │                      │
└──────────────┴─────────────────────────────────┴──────────────────────┘
```

- ≥ 1180 px：三栏；Inspector 固定宽度 340–400 px，不覆盖内容。
- 760–1179 px：rail 收窄，Inspector 成为右侧 sheet。
- < 760 px：单栏，asset detail 使用完整页面；桌面应用仍需支持窄窗口。
- Library 使用连续 ledger rows，不把每个资产切成相同圆角卡片。
- Overview 展示“最近变化、进行中、待审、即将续费”的混合时间线，不展示无行动意义的大数字。
- Graph 是 Relations 的一个视图切换；默认可访问的 path/list 与 graph 共享同一 query 结果。

#### 交互规则

- 单击 row 选中并更新 Inspector；Enter 打开完整详情。
- destructive 或 identity-changing 操作显示影响预览，再要求明确确认。
- 保存成功使用与按钮相同的动词，例如 `Archive` → `Archived`。
- 空态给出一个可执行下一步；错误态显示 failure category 和恢复动作。
- 乐观 UI 只用于纯 presentation 状态。Canonical mutation 必须等待 receipt。
- 只在打开/关闭 Inspector、展开关系、确认保存时使用响应用户动作的短动画；支持 reduced motion。
- 键盘 focus 始终可见；颜色不是状态的唯一载体。

### 6.5 页面信息架构

```text
Overview
Library
  All Assets
  Media
  Software
  Services
  Projects       # Phase 7 前隐藏，不展示空壳
  Knowledge      # Phase 7 前隐藏，不展示空壳
Relations
Activity
Import / Export
Settings
```

页面只在 capability 存在时注册。Phase 5 不创建 Projects、Knowledge、Runtime 的 placeholder 页面。

### 6.6 Phase 5 任务卡

#### P5-00 — Phase 4 exit audit

目标：证明 Desktop 可以只依赖 application interface 开发。

工作：

- 对照 `docs/11-unified-library-core.md` 的每一项 exit criterion 找到实现和测试。
- 记录 UI 仍需要但 application 未提供的查询；这些缺口先作为 Phase 4 修复，不在 Tauri 中绕过。
- 确认 CLI 没有 Phase 4 业务逻辑副本。

完成条件：

- 全部 Phase 4 exit criteria 均有测试位置。
- Desktop 首个 read slice 不需要直接调用 repository。
- workspace 质量门禁全绿。

#### P5-01 — Desktop skeleton and transport smoke

目标：创建可启动的 Tauri/React shell，并通过一个真实 application query 验证 seam。

允许范围：`apps/desktop/**`、workspace manifest；除 composition 所需外不改领域行为。

实现：

- 创建 Tauri 2 + React + TypeScript + Vite 应用。
- composition root 打开指定 AssetMesh DB 并执行迁移。
- 暴露 `app_capabilities` 和一个 `library_list` smoke command。
- 提供 loading、ready、setup/corrupt failure 三种启动状态。

测试：

- Rust command test 使用临时 DB。
- React test 验证启动状态和一条真实 asset row。
- 不 mock application 规则；可以 fake transport 验证 presentation。

完成条件：应用从临时 DB 渲染真实 AssetSummary；没有 SQLite/Tauri 类型进入 `core`。

#### P5-02 — Library read slice

目标：完成 All Assets 的分页、排序、生命周期和 kind/tag 过滤。

实现：

- URL/router state 或等价可恢复 navigation state。
- deterministic pagination；刷新不改变相同数据的顺序。
- merged tombstone 默认隐藏，archived 必须显式选择。
- row 显示名称、kind、subtitle、tags、updated time；字段来自 `AssetSummary`。

测试：跨 Media/Software/Service 的分页、空态、错误态、窄窗口和键盘选择。

完成条件：UI 不按模块自行拼 summary；同一 query 的 CLI/Desktop 结果集合一致。

#### P5-03 — Asset detail and typed domain panels

目标：以统一 shell 展示共享信息和 typed details。

实现：

- Header、Summary、Domain details、Relations、Activity、Sources、Actions。
- Media/Software/Service 使用 exhaustive typed renderer。
- external refs 可复制；credential-bearing 内容不显示。
- merged asset 显示 redirect，不当成普通 live asset。

测试：三种 detail、archived、merged、缺少可选字段、未知未来 detail version 的安全失败。

完成条件：UI 只使用一个完整 detail query，不发起 N+1 repository 风格请求。

#### P5-04 — Search, filters and navigation

目标：统一搜索与普通 Library 使用相同 row vocabulary。

实现：

- 250–350 ms 输入 debounce，仅减少 transport 调用，不改变搜索语义。
- kind/lifecycle/tag filter 与查询文本组合。
- CJK、substring 行为由 application/search adapter 返回，UI 不实现 fallback。
- command palette 只导航或调用已有 deterministic command。

测试：输入取消、旧响应不覆盖新响应、空查询、CJK、archived/merged 策略。

完成条件：搜索结果可直接打开统一 detail；不存在第二套 SearchCard 领域模型。

#### P5-05 — Mutation pattern and receipts

目标：冻结所有桌面写操作共用的交互和 transport 模式。

实现一个代表性低风险操作，例如编辑 Software purpose：

```text
load detail + revision
  → edit draft
  → validate transport input
  → application command
  → transaction
  → MutationReceipt
  → invalidate + read-back
```

要求：

- stale revision 显示冲突，不自动重试。
- no-op 明确返回 no-change receipt 或不发命令，行为固定。
- receipt 包含 operation、asset IDs、revision/observed result、warnings。
- UI 不把 toast 当成成功证据；read-back 后才刷新展示。

完成条件：成功、validation、stale、transaction failure 四条路径有测试，之后所有写页面复用该模式。

#### P5-06 — Media, Software and Service workflows

目标：覆盖三个已实现模块的主要写流程。

顺序：Media → Software → Services。每个模块完成后运行全量测试再开始下一个。

最低流程：

- Media：add/edit、status transition、progress、rating、archive。
- Software：manual add/edit、discovery preview、adopt create/update、archive。
- Services：add/edit、subscription fields、record renewal、archive。

规则：

- discovery scan 不写 canonical data。
- money 在 transport 层用 decimal string，application 转 minor units。
- renewal 日期和下一边界由用户明确输入，不推算。
- user-owned purpose/notes 不被 provider 覆盖。

完成条件：每个流程至少一条 Tauri contract test、一条 UI test；关键路径有真实临时 SQLite integration test。

#### P5-07 — Relations and impact explorer

目标：让关系可读、可编辑、可解释。

实现：

- incoming/outgoing/neighbors list。
- dependency/dependent/impact path，显示 depth 与 relation evidence。
- add/remove relation 使用 registry 提供的有效类型。
- graph view 在 list/path 完成后加入；布局坐标只在 UI。

测试：inverse、symmetric、cycle、depth cap、archived、merged redirect、删除确认。

完成条件：图和列表来自同一个 application result；任何关系都能通过键盘和无图模式操作。

#### P5-08 — Activity and duplicate review

目标：交付全局历史查询和显式 merge review。

实现：

- activity 按 asset/kind/type/time 过滤和分页。
- duplicate candidate 展示具体 evidence，不显示伪精确 confidence 分数。
- `keep separate/dismiss` 只有持久语义已设计时才实现；否则保持 ephemeral。
- merge preview 显示 winner/loser、字段冲突、relations 影响。
- merge failure 保持双方 canonical state 不变。

完成条件：能从 candidate 进入 explicit merge；UI 不通过相似度自动选择 winner。

#### P5-09 — Import/export and settings

目标：为 portable data 提供安全图形入口。

实现：

- 文件/目录 picker 在 Tauri adapter 处理路径授权。
- import 必须先 preview，再 explicit apply；preview 与 apply 使用同一 preflight。
- export 显示目标、manifest 摘要、完成 receipt。
- settings 仅包含真实配置：DB location、theme、provider setup 状态；不放未来开关。

测试：malformed bundle、unsupported version、collision、dry-run/apply 对称、取消 picker。

完成条件：UI 无法绕过 preflight；错误不会产生部分未披露 mutation。

#### P5-10 — Desktop hardening and release gate

目标：证明桌面端不是演示壳。

必须完成：

- Playwright 或等价 E2E：创建 → 搜索 → 打开 → 修改 → 建关系 → export。
- 恢复旧数据库 fixture 并启动 Desktop。
- 键盘 navigation、focus、screen-reader label、contrast、reduced motion 检查。
- 1k、10k、50k asset 的 list/search 基准；UI 使用 virtualization 仅在测量证明需要时。
- crash/reopen 不损坏 canonical data；取消长 read 不触发 mutation。
- packaging、签名/notarization 可以作为发行子任务，但不改变领域实现。

Phase 5 退出条件：README 中列出的 Phase 1–4 主要能力均能从 Desktop 操作；Desktop 不直接访问 repository/SQL，
并且 CLI/Desktop application contract tests 一致。

---

## 7. Phase 6 — Runtime Enrichment

### 7.1 产品目标

Runtime 回答“这个资产现在处于什么可观察状态，依据是什么”。它附属于 Software、Service、Project、Harness 等
资产，不建立独立监控世界。

### 7.2 领域模型

建议的 application vocabulary：

```rust
RuntimeTarget {
    asset_id,
    capability,
}

RuntimeObservation {
    target,
    status,          // ready | degraded | stopped | unavailable | unknown
    observed_at,
    expires_at,
    facts,
    evidence,
    completeness,
}

RuntimeFinding {
    code,
    severity,        // info | warning | error
    asset_id,
    summary,
    evidence_refs,
    recovery_hint,
}
```

`facts` 在 application interface 上使用 capability-specific typed union，不用无限制 JSON。Adapter 的原始 payload 不
离开 provider crate。`unknown` 与 `unavailable` 是正常结果，不转换成 stopped 或 healthy。

### 7.3 Runtime interface

```text
RuntimeQuery
  observe(targets, options) -> ObservationBatch
  latest(asset_id) -> RuntimeView
  explain(finding_id) -> FindingExplanation

RuntimeProvider
  capabilities() -> ProviderCapabilities
  observe(targets, deadline, cancellation) -> ProviderResult
```

第一版只读。持久化策略：

- 当前 UI session 可内存缓存，带 TTL 和 source timestamp。
- 若真实扫描成本要求跨进程缓存，缓存放 infrastructure，明确 schema 和过期；删除后可重建。
- runtime snapshot 默认不进入 portable export。
- activity 只记录用户触发且有耐久意义的动作；周期 scan 不产生事件洪水。

### 7.4 安全规则

- 固定 executable 和 argv；不提供 generic shell/command field。
- 路径先 canonicalize 并做 containment 检查。
- 超时、输出字节、对象数量、并发数均有硬预算。
- evidence 只保存归一化事实、digest 或安全摘要。
- secret、环境变量全集、credential-bearing URL、原始 stderr 不进入 DTO、日志或 fixture。
- provider 部分失败保留 sibling results，并标明 completeness。
- 扫描不导入插件代码，不执行 Skill 内容，不自动启动进程。

### 7.5 Phase 6 任务卡

#### P6-00 — Runtime contract and threat model

目标：在编码前冻结 Observation、Finding、Evidence、capability、TTL、failure、budget。

产物：一份 ADR、typed contract 草案、三类 synthetic fixtures、明确的 secret redaction table。

完成条件：能够表达 running-but-unhealthy、healthy-but-not-active、stopped、unknown、provider unavailable，且不会用一个
Boolean 压平状态。

#### P6-01 — Runtime application module

目标：实现不依赖 OS 的 `observe/latest/explain` 深模块。

实现：provider registry、target routing、deadline/cancellation、partial results、finding derivation、内存 fake。

测试：deterministic ordering、partial failure、timeout、unknown capability、stale cache、evidence reference integrity。

完成条件：使用 fake providers 即可完整测试 application 行为；没有 subprocess 或 Tauri 依赖进入 core。

#### P6-02 — Local process, port and launchd adapters

目标：为明确声明 runtime locator 的 Software/Service 提供本机观察。

范围：

- process identity 使用 executable/bundle/declared locator，不只靠模糊名称。
- port 只验证明确声明 endpoint；不全机无界扫描。
- launchd 读取 label 状态，并将 supervisor 与 semantic health 分开。
- semantic health 只调用资产声明的固定 adapter contract。

完成条件：fixture + 隔离真实 subprocess 测试覆盖 healthy、stopped、timeout、malformed、permission denied。

#### P6-03 — Container runtime adapter

目标：观察 Docker/OrbStack 中明确关联的容器和 compose project。

规则：

- 先实现 read-only list/inspect/health/log-tail metadata；日志正文不默认持久化。
- container identity 与 AssetMesh identity 通过 namespaced external ref 或明确 relation 匹配。
- 不以 container name 相似度自动绑定 canonical asset。
- Docker 不可用时返回 named unavailable，不影响其他 provider。

完成条件：fake CLI fixtures、版本差异、空环境、large output truncation、无权限路径有测试。

#### P6-04 — Runtime asset UI

目标：在 Asset detail 内呈现 runtime，而不是增加首页监控大盘。

实现：

- Detail 中 `Runtime` 区域显示观察时间、状态、completeness、evidence 和 refresh。
- Overview 只显示需要行动的 Findings；健康资产保持安静。
- Logs 使用按需、bounded fetch；离开视图取消请求。
- unavailable 提供 setup/retry，不显示假健康。

完成条件：UI 明确区分 canonical detail 与 observed runtime；刷新 runtime 不改变 canonical updated_at。

#### P6-05 — Dogfood and performance gate

目标：证明 runtime 值得保留，并决定是否需要 background job/persistent cache。

至少连续四周记录：

- 触发了实际诊断或节省调查的 Finding；
- scan p50/p95 时间；
- timeout/partial/unavailable 比例；
- cache 命中和 stale 情况；
- 用户实际主动刷新频率。

门禁：至少三次有用事件、至少两种 failure class。未通过时冻结为按需 read-only 功能，不增加后台扫描。

#### P6-06 — Reviewed deterministic actions（可选）

进入条件：P6-05 通过，并且每个动作存在 owner-native、固定、可验证 interface。

每个动作单独立项，流程固定：

```text
fresh observation
  → immutable plan + expected revision
  → impact preview
  → explicit approval
  → owner-native action
  → read-back
  → receipt
```

首批候选仅 `open`、`restart`；不实现任意 shell、批量修复、自动重启或 uninstall。一个动作必须覆盖 stale plan、partial
failure、timeout 和 read-back mismatch 后才能进入下一个动作。

Phase 6 退出条件：至少两个 provider 通过同一 Runtime interface 返回可解释观察；UI 不把观察写进 canonical fields；任何
动作都不是 scan 的副作用。

---

## 8. Phase 7 — Additional Personal Modules

Phase 7 不等于“把所有个人工具搬进来”。本阶段只选择已有真实数据、明确 owner、可移植 canonical identity 的模块。

### 8.1 模块顺序

1. Projects：最直接复用 Asset、relations、Git provider，并给后续 context 提供锚点。
2. Agent Capabilities：复用 Software、Project、Runtime，同时验证复杂 provenance。
3. Knowledge Collections：验证外部 Core 投影，而不复制其私有知识实体。

Queue/Tasks 和 Learning Records 保持候选。只有独立规格证明其 typed details、identity、portable semantics 后再进入新
Phase；PCC/LeetCode 中存在一个 UI 不足以证明它们必须成为 AssetMesh module。

### 8.2 P7A — Projects

#### Canonical model

```rust
ProjectRecord {
    asset_id,
    project_type,        // git | workspace | personal
    lifecycle,           // active | paused | archived is mapped deliberately
    role,                // product | library | experiment | learning | infrastructure | other
    purpose,
    primary_location,
    default_branch,
    started_at,
    last_meaningful_at,
    notes,
}
```

`primary_location` 是用户确认的 locator，不用它替代 Asset ID。Git dirty state、HEAD、ahead/behind、remote reachability 是
observation，不是 canonical ProjectRecord。

Namespaced external refs 候选：

```text
github_repo_id:<numeric-id>
gitlab_project_id:<host>/<numeric-id>
remote_url_digest:<normalized-host-path-digest>  # 仅在没有稳定 provider ID 时
```

路径不是 external ref；机器迁移会改变路径。

#### Relations

```text
project uses software/service
project depends_on project/service
project hosted_on service.vps
project deployed_to service.local/service.vps   # 新类型需 registry + migration
project presented_by software.plugin            # 只有真实用例后加入
```

每个新 relation type 必须同时定义 canonical direction、inverse、merge 行为、portable 语义和 migration CHECK 更新。

#### Project provider

Local Git adapter：

- 扫描仅限用户明确 roots，深度和 candidate 数有上限。
- 读取 remote、HEAD、branch、worktree dirty、last commit。
- 不执行 checkout/reset/clean/stash/pull。

GitHub adapter：

- 通过已认证 `gh` 读取 repository identity 和 metadata。
- 不读取 token，不接收 frontend-supplied argv。
- 支持 discovery、inspect、local checkout match。
- Phase 7 不实现 PR/Issue/Review/Checks/Actions 日志客户端。

#### Project 任务卡

##### P7-01 — Project contract

冻结 domain invariants、kind、typed details、external refs、relations、search projection、activity、merge policy、portable V1。
完成条件：规格能回答空路径、仓库重命名/转移、多个 checkout、无 remote、archived、merge conflict。

##### P7-02 — Project canonical vertical slice

实现 domain、ports、application CRUD、search projection、in-memory tests。完成条件：不依赖 Git 或 GitHub 即可完整管理
manual project。

##### P7-03 — SQLite and portability

新增不可修改的顺序 migration、repository、module metadata、`modules/projects.jsonl`、legacy bundle compatibility。
完成条件：旧 Phase 1–6 DB 真实迁移 fixture、round-trip、idempotent restore、undeclared-section rejection 全部通过。

##### P7-04 — Git/GitHub discovery and adoption

实现 local Git 与 GitHub read-only adapters，candidate classification 和 explicit adoption。完成条件：scan-never-writes、exact
external-ref match、rename/transfer、ambiguous checkout、dirty worktree 都有测试。

##### P7-05 — Project UI and cross-module relations

实现 Library/detail/create/edit/discover/adopt，及 Software/Service/Project relations。完成条件：从 Project detail 可解释其
checkout、owner、dependencies、runtime，但 PR/Issue 内容不存在于 UI 或 schema。

### 8.3 P7B — Agent Capabilities

#### 领域位置

新增独立 `Agent Capability` module，而不是把每个磁盘文件伪装成 Software asset。

Canonical asset：用户希望长期记住的 Skill identity 或 Skill distribution。Observed occurrence：某个 Harness 在某个 Host/Scope
发现的一次 copy/link/registration。Occurrence 不自动成为 Asset。

#### Canonical model

```rust
AgentCapabilityRecord {
    asset_id,
    capability_type,    // skill initially
    declared_name,
    description,
    provenance,
    lifecycle_owner,
    source_revision,
    content_digest,
    notes,
}
```

```text
CapabilityObservation
  host
  harness
  scope
  context
  occurrences[]
  activations[]
  replicas[]
  collisions[]
  findings[]
```

必须保持以下区别：

- identity ≠ declared name；
- occurrence ≠ deployment；
- file presence ≠ activation；
- replica ≠ collision；
- cache/backup ≠ installation；
- lifecycle owner ≠ scanner。

#### Harness adapters

首批适配 Hermes、Codex、Claude Code、OpenCode、ZCode 和 Vercel skill lock。迁移 Skill Control Center 的行为和 fixtures 时：

- 在 Rust 中重新实现明确 contract，不逐行翻译 Python。
- 保留真实路径拓扑、ledger、precedence、digest 的测试含义。
- 不执行 `SKILL.md`；只解析 bounded frontmatter 和内容 digest。
- repository context 是 observation request 的一部分，不能声称 user-level baseline 对所有项目都成立。

#### Agent Capability 任务卡

##### P7-06 — Capability domain and ADR

冻结 identity、distribution、deployment、occurrence、activation、host、scope、provenance 和 owner 词汇。完成条件：North-Star
`tdd` 场景能表示为一个 canonical identity、多次 exposures，而不是 38 个“已安装 Skill”。

##### P7-07 — `observe` / `explain` application module

实现 host-scoped、context-aware、read-only interface；使用 fake adapters 先完成 collision/replica/effective-surface derivation。
完成条件：删除 UI 后模块仍能通过同一 interface 回答“为什么这个 Skill 对此 Harness 有效”。

##### P7-08 — Harness adapters and scan budgets

逐个 adapter 迁移，每次只加一个，并覆盖 supported/degraded/malformed/unavailable。完成条件：真实 host isolated integration
不执行 Skill、不改变 harness config，并可取消扫描。

##### P7-09 — Canonical adoption and portability

允许用户把 observation 中的 identity 显式采纳为 Asset；不移动、不复制、不安装文件。实现 module schema 和 portable V1。
完成条件：adoption 可丢弃观察缓存后仍保留用户确认的 provenance，且不偷走 native lifecycle ownership。

##### P7-10 — Capability topology UI

实现 Harness surface、Skill detail、occurrence topology、collision/replica 和 evidence。Lifecycle 动作只深链到 native owner；本任务
不实现安装/update/remove。完成条件：一个 Skill 的所有 exposures 在一页可解释，unknown 明确呈现。

### 8.4 P7C — Knowledge Collections

#### 所有权

AssetMesh 只管理 Knowledge Collection 资产及跨域关系。Hermes Obsidian 或其他知识 Core 继续拥有文档、Source Unit、Atom、
Decision、Review Item、Proposal 和 Vault 写入。

#### Canonical model

```rust
KnowledgeCollectionRecord {
    asset_id,
    collection_type,      // obsidian_vault | directory | course | external
    canonical_locator,
    purpose,
    language,
    owner_adapter,
    notes,
}
```

外部摘要属于 observation：

```text
document_count
source_unit_count
atom_count
last_successful_scan
product_ready
warnings
owner_schema_version
```

这些摘要不进入 canonical record 的 portable truth；portable bundle 保存重新连接 owner 所需的安全 locator 和 explicit external
refs，不复制 owner Registry。

#### Knowledge relations

```text
knowledge collection belongs_to project/collection
project uses knowledge collection
course references knowledge collection
knowledge collection maintained_by software/service
```

只有 relation registry 已定义的语义才能使用；新的 `belongs_to`/`references` 需要单独 ADR 和 migration。

#### Knowledge 任务卡

##### P7-11 — Knowledge Collection contract

冻结 canonical/owner-owned 分界、locator、external ref、merge、portable 和 unavailable 语义。完成条件：删除 AssetMesh DB 不影响
Vault；删除 owner Registry 后 AssetMesh 不假装仍能检索 atoms。

##### P7-12 — Canonical vertical slice

实现 CRUD、search projection、relations、activity 和 in-memory tests。完成条件：一个完全手工 collection 不依赖 Obsidian 也能
存在。

##### P7-13 — SQLite and portability

新增 migration、repository、module schema、`modules/knowledge.jsonl` 和旧 bundle compatibility。完成条件同 P7-03。

##### P7-14 — Owner adapter

先适配 Hermes Obsidian 的稳定 CLI JSON envelope：capabilities、health、collection summary、open door。不得读取其 SQLite 私有
schema。完成条件：supported/degraded/malformed/unavailable 和 schema mismatch 均有 contract test。

##### P7-15 — Knowledge UI

实现 collection detail、owner health、summary、relations 和明确外部打开/分析入口。AssetMesh 不显示 Atom 编辑器或 Vault 文件树。
完成条件：用户能判断 collection 是什么、在哪里、由谁维护、最近是否成功观察，并可进入 owner 完成深层任务。

### 8.5 P7-16 — Cross-module hardening and plugin decision

完成：

- Media、Software、Services、Projects、Agent Capabilities、Knowledge 的 unified list/detail/search。
- 至少三条真实跨模块 relation chain 和 impact test。
- 全部 module portable round-trip + historical bundle compatibility。
- 50k 混合资产性能基准。
- Desktop、CLI 对新增模块的同 interface 覆盖。
- 从干净机器/用户目录恢复一个脱敏 personal estate fixture。

第三方 plugin ABI 的判定：

只有在至少三个第一方模块的 registration 需求稳定重复、且外部模块有真实开发者时才立项。满足条件也先写 ADR 和最小
descriptor，不直接开放动态 native code loading。若需求仍可通过 provider/owner adapter 满足，继续保持 compile-time modules。

Phase 7 退出条件：三个新增模块都拥有 typed details、canonical identity、search、relations、activity、portable schema、CLI/Desktop
路径；外部 owner 的私有状态没有被复制成第二真相。

---

## 9. 编码指导

### 9.1 Rust

- Domain constructor 建立不变量；repository boundary 再验证跨记录不变量。
- 对有限状态使用 enum，对可选 patch 使用明确的 `Unchanged/Clear/Set`，不让 `Option` 同时表达三种意思。
- Application command/query 使用专用 request/result 类型，避免长参数列表。
- `serde_json::Value` 只允许出现在外部不可信 payload 的 adapter 内部和版本化 activity payload；typed details 不使用它。
- `AppError` 增加稳定 variant 时同步 CLI/Desktop mapping tests。
- 时间通过 `Clock`，ID 通过 `IdGenerator`；测试不直接依赖 wall clock/随机 UUID。
- 排序总有 Asset ID 或稳定 ID tie-breaker。
- Provider 接收 runner/filesystem 等依赖；测试不 monkeypatch 全局环境。
- 迁移 append-only；已发布 migration 永不修改。

### 9.2 TypeScript/React

- feature 目录按用户任务组织，不按 `components/hooks/utils` 全局分类堆放。
- transport DTO 在一个 adapter 层解码；页面不接触 `unknown` payload。
- server state 由 query layer 管理，表单 draft 保持局部；不要把整个应用放进一个全局 store。
- exhaustive switch 处理 AssetDetails 和 failure category，新增 variant 时编译失败。
- presentation helper 保持纯函数；业务 invariant 不在 TypeScript 复制。
- 每个异步视图显式覆盖 idle/loading/ready/empty/partial/error/stale。
- 测试通过用户可见角色、名称和行为定位元素，不依赖 CSS class 或大 snapshot。
- 一个 feature 至少有一个真实 transport contract fixture；fixtures 必须脱敏且有 schema/version。

### 9.3 SQLite 与 portable data

- canonical schema 变化必须同时考虑 migration、module metadata、portable wire DTO、import preflight、round-trip、legacy fixture。
- wire DTO 用 `*V1` 等明确版本名；不直接 serialize mutable domain struct。
- 新 section 由 manifest declaration 决定权威性；存在文件但未声明视为 corruption。
- import 在 mutation 前完成所有可完成的 validation；apply 与 dry-run 使用同一 plan。
- rebuildable state 不写入 portable bundle。

### 9.4 Provider

- Provider 只返回 candidate/observation/enrichment，不直接拿 repository。
- 所有外部调用有 deadline、cancellation、byte/object/concurrency budget。
- 输入先 normalize/validate，再形成固定 argv 或 endpoint。
- Adapter 测试至少包含 supported、degraded、malformed、unavailable。
- live test opt-in、read-only、bounded；普通 `cargo test` 不依赖用户账户和网络。

### 9.5 UI 设计评审清单

- 页面第一眼是否在回答一个用户问题，而不是展示技术模块？
- 主要动作是否只有一个，名称是否精确？
- 结构是否依靠内容层级，而不是一组相同卡片？
- empty/error/partial/stale 是否给出真实下一步？
- Inspector 是否使用完整 application DTO，而非多次补数据？
- 键盘、窄窗口、dark mode、reduced motion 是否可用？
- 删除一个装饰元素后是否更清楚？若是，删除它。

---

## 10. 测试策略

### 10.1 测试层次

| 层次 | 验证内容 | 不验证内容 |
| --- | --- | --- |
| Domain | 单对象不变量、纯状态转换 | SQLite、UI |
| Application | use case、事务编排、matching、typed result | SQL 细节、DOM |
| Port contract | fake 与 SQLite/provider adapter 行为一致 | 页面布局 |
| Adapter | SQL、filesystem、CLI、Tauri mapping | 重复领域规则 |
| UI | 用户任务、状态呈现、accessibility | repository 和 matching 实现 |
| E2E | 少量跨层关键路径 | 所有边界组合 |

### 10.2 每类变化的最低测试

| 变化 | 最低要求 |
| --- | --- |
| Domain field/invariant | constructor/update + invalid cases |
| Application command | success/no-op/conflict/rollback |
| Query | filter/order/pagination/lifecycle/snapshot |
| Migration | previous real DB fixture → latest → reopen |
| Portable section | export/import/idempotence/legacy/corruption |
| Provider | success/degraded/malformed/unavailable/budget |
| Tauri command | DTO mapping + typed error + temporary DB |
| React workflow | ready/empty/error/stale + keyboard action |
| Destructive action | preview/stale/partial/read-back mismatch |

### 10.3 Fixture 规则

- fixtures 只含合成或充分脱敏数据。
- 每个外部 adapter fixture 记录来源版本和采集方式，但不记录 credential/private payload。
- historical fixture 一经用于兼容性测试便不可改写；新增版本使用新文件。
- golden snapshot 只用于真正稳定的 wire contract，不用于大型 DOM 或易变文案。

---

## 11. 任务切分和变更规模

为降低 Flash 级模型偏移，默认限制：

- 一个任务修改不超过两个主要模块；migration + 对应 repository/portable 测试视为一个领域纵切。
- 一个任务新增一个 public interface；超过一个时优先拆分。
- UI 任务只依赖已经存在并测试通过的 backend capability。
- provider 任务先交付 adapter contract，不同时做 adoption UI。
- refactor 与行为变化分开提交；若必须共同发生，测试需分别证明旧行为保留和新行为加入。

遇到下列情况立即停止并升级设计：

- 需要 UI 直接查 repository 才能完成页面。
- 需要把 typed details 改成任意 JSON 才能复用页面。
- 需要修改已发布 migration。
- 需要在 write transaction 中调用外部程序或网络。
- 需要从 path/name 猜测 canonical identity 并自动合并。
- 需要读取外部 Core 私有 SQLite 才能集成。
- 需要 generic shell、arbitrary HTTP 或任意 filesystem write。
- 任务无法用一个可观察结果描述完成。

---

## 12. 给执行模型的任务提示词模板

将 `<TASK_ID>` 替换成单个任务 ID：

```text
在 AssetMesh 中实现 docs/12-phases-5-7-execution-plan.md 的 <TASK_ID>。

先阅读该文档第 1–4 节、目标 Phase 总则和完整任务卡，再阅读任务卡引用的现有文件。
只实现这个任务，不创建后续任务的空壳，不改变未被任务要求的产品范围。

开始前：
1. 运行 git status --short，保护已有修改。
2. 用 rg 定位现有 interface、tests 和命名。
3. 写出本任务的可观察完成条件。

实现时：
- 先补能证明缺失行为的测试。
- 通过 application interface 工作；adapter 不拥有业务规则。
- canonical/derived/owner-owned 数据遵守文档表格。
- 新外部 I/O 必须 bounded、cancelable、typed failure、可 fake。

结束时运行任务卡局部测试和仓库级质量门禁，并按第 1.3 节模板报告。
如果任务需要越过文档的 stop condition，停止实现并报告具体 seam 缺口。
```

### 12.1 Review 提示词模板

```text
审查 <TASK_ID> 的实现，只报告能由代码或测试证明的问题。

按两条轴分别检查：
1. Spec：逐项核对任务卡的完成条件、非目标和 stop condition。
2. Architecture：检查依赖方向、module depth、canonical/derived 分界、事务、portable 和 adapter 安全。

每个 finding 给出文件/行、失败场景、严重度和最小修复方向。
如果没有 finding，列出实际检查过的接口和测试，不要用“看起来没问题”代替证据。
```

---

## 13. 最终产品形态

Phase 7 完成后，AssetMesh 应呈现为：

```text
一个本地优先、可导出、以资产为中心的个人数字目录

Canonical library
  Media
  Software
  Services
  Projects
  Agent Capabilities
  Knowledge Collections

Shared capabilities
  Identity / external refs
  Search
  Relations / impact
  Activity
  Duplicate review / merge
  Import / export

Contextual observations
  Runtime health
  Git / GitHub state
  Harness activation topology
  External knowledge owner health
```

它不应成为：

- 第二个 GitHub、Obsidian、Docker Desktop 或 Skill marketplace；
- 系统级实时监控 dashboard；
- 保存 credential 的账户中心；
- 任意命令执行器；
- 自动生成或静默改写 canonical truth 的 AI 系统；
- 为了统一而复制所有外部 Core 私有数据的数据库。

最终判断始终回到一句话：**AssetMesh 记住用户长期拥有和关心的数字资产，并解释它们之间的关系；外部 owner 继续拥有
自己的深层真相。**
