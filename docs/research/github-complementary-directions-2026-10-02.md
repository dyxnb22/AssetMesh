# AssetMesh：GitHub 互补方向候选研究

查询日期：2026-10-02。本文用于头脑风暴，不构成路线图或实施承诺。用户会持续使用 Codex；筛选标准是能否增加持久、可核实的个人事实，或显著降低高频操作成本。已有资产分类、关系、历史、显式合并、CLI、服务启停和备份不作为新功能重新推荐。

## 候选结论

优先验证六种实际问题：换机后能否重建、哪些软件值得继续保留、外部变化是否影响自己、设备实际状态与个人记录是否一致、已购买权益能否找到证据、网站消失后是否还保有当时的资料。先选一个真实场景验证收益，不为了接入而部署更多服务。

Codex 可以生成临时报表、提出解释和执行脚本。AssetMesh 如需新增能力，应集中保存来源标识、资产映射、用户确认结果、验证时间和少量可复用视图。单次整理即可完成的需求不值得产品化。

## 已核实的参考项目

星数仅表示关注规模，不代表质量、适配性或安全性。下表星数为查询当日 GitHub 页面显示的约数；发布日采用 GitHub API 的 UTC 日期。活动证据只说明观察到近期维护，不保证未来维护。

| 项目 | 核实功能 | 关注规模 | 活动证据 |
| --- | --- | --- | --- |
| [HomeBox](https://github.com/sysadminsmedia/homebox) | 家庭物品、位置、文档、保修、购买与维护记录 | 约 7.4k stars | [v0.26.2](https://github.com/sysadminsmedia/homebox/releases/tag/v0.26.2)，2026-06-14 |
| [Paperless-ngx](https://github.com/paperless-ngx/paperless-ngx) | 文档归档、OCR、原件与 PDF/A 保存、标签和全文检索 | 约 46.2k stars | [v3.2.1](https://github.com/paperless-ngx/paperless-ngx/releases/tag/v3.2.1)，2026-09-20 |
| [Linkwarden](https://github.com/linkwarden/linkwarden) | 网页截图、PDF、单文件 HTML 保存，阅读、标注、检索 | 约 19.9k stars | [v2.16.3](https://github.com/linkwarden/linkwarden/releases/tag/v2.16.3)，2026-09-09 |
| [Wallos](https://github.com/ellite/Wallos) | 周期订阅、币种、支出统计和付款通知 | 约 8.6k stars | [v5.8.3](https://github.com/ellite/Wallos/releases/tag/v5.8.3)，2026-10-01 |

HomeBox 应参考 `sysadminsmedia/homebox` 的延续版本；原始 [hay-kot/homebox](https://github.com/hay-kot/homebox) 已于 2024-06-12 归档。Paperless 功能进一步对照[官方文档源码](https://github.com/paperless-ngx/paperless-ngx/blob/dev/docs/index.md)，该开发分支文档可能包含尚未进入稳定版本的功能，因此本研究只依赖成熟的 OCR、归档和检索能力。

发布元数据核实来源：[HomeBox API](https://api.github.com/repos/sysadminsmedia/homebox/releases/latest)、[Paperless API](https://api.github.com/repos/paperless-ngx/paperless-ngx/releases/latest)、[Linkwarden API](https://api.github.com/repos/linkwarden/linkwarden/releases/latest)、[Wallos API](https://api.github.com/repos/ellite/Wallos/releases/latest)。

## 产品推演：四个具体互补场景

以下为针对 AssetMesh 的设计推演，不是参考项目已经实现的集成。

### 1. 实物设备与数字环境的一致性

场景：准备出售旧电脑或替换 NAS 时，要知道这台设备承载哪些服务、安装了哪些付费软件、哪些数据尚未迁出。HomeBox 可直接负责设备照片、位置、序列号、购买和保修资料；AssetMesh 最小只保存 HomeBox 外部条目引用，以及设备与现有软件/服务的已确认部署对应关系。先支持少数重要设备，无需扩成家庭库存管理系统。

价值来自跨系统的实际对应关系：某个服务究竟运行在哪台设备上，需要实际盘点与确认。对话可以推导迁移清单，但不能凭空知道这份映射；若只有一台电脑、迁移问题很少出现，则直接用文档即可，不必建设。

### 2. 已购买权益与原始证据

场景：重装付费软件时，快速确定买的是哪个版本、使用哪个账户、授权范围是什么，以及购买凭证在哪里。Paperless-ngx 负责扫描、OCR、归档、检索；AssetMesh 最小保存购买/授权事实、来源文档引用和确认状态，关联现有软件或服务。OCR 提取建议交给现有工具，密钥继续交给凭据管理工具。

价值来自可追溯的个人权益记录。Codex 可以读合同并提取候选结论，但授权结论与原始证据的持久对应关系必须有稳定落点；无需为此重建文档库。先用 5 个已购买软件验证能否明显减少重复查找。

### 3. 资产资料的留存与可复现引用

场景：当年采用某工具是因为某份兼容性说明或配置教程，半年后页面修改或消失。Linkwarden 直接负责网页抓取和保存；AssetMesh 最小保留“这条资产依据哪份资料、何时验证、适用于哪个版本”的引用，并能打开存档。

不要重新建设书签管理器、阅读器或爬虫。对话可总结现有页面，但不能恢复从未保存的旧页面；价值来自事先保存的原始资料与资产版本的对应关系。仅保存自己确实依赖的安装步骤、重要条款或兼容性资料，避免把所有收藏再复制一份。

### 4. 订阅与实际用途的去留判断

场景：两个服务是否重复付费，取消其中一个会影响哪些工作流。Wallos 可直接负责订阅周期、币种、支出统计和付款通知；AssetMesh 最小只关联订阅与现有软件/服务，记录实际用途、替代选择和用户最终保留/取消的理由。无需重建费用看板或续费引擎。

这项的必要性低于前几项：若只要每月费用表，Wallos 或现成表格已经足够。只有需要把成本和个人用途、依赖、实际使用事实结合起来，稳定的资产映射才提供额外价值。分析本身完全可以让 Codex 按需生成。

## 主线程补充核实的参考项目与推演

以下元数据由同一研究任务的主线程于 2026-10-02 通过官方 GitHub API 核实；均未归档。`pushed_at` 为仓库活动信号，不等于稳定版发布日期。

| 项目 | 精确 stars | 最近 push 日期 | 核实依据 |
| --- | ---: | --- | --- |
| [chezmoi](https://github.com/twpayne/chezmoi) | 21,793 | 2026-09-28 | [GitHub API](https://api.github.com/repos/twpayne/chezmoi)；官方 README 说明跨设备 dotfile 管理 |
| [ActivityWatch](https://github.com/ActivityWatch/activitywatch) | 19,036 | 2026-10-01 | [GitHub API](https://api.github.com/repos/ActivityWatch/activitywatch)；官方 README 说明本地应用/网页与 AFK 记录、REST API 和 JSON 导出 |
| [changedetection.io](https://github.com/dgtlmoon/changedetection.io) | 34,741 | 2026-10-02 | [GitHub API](https://api.github.com/repos/dgtlmoon/changedetection.io)；官方 README 说明网页/JSON 变化监测与 REST API |
| [osquery](https://github.com/osquery/osquery) | 23,600 | 2026-10-01 | [GitHub API](https://api.github.com/repos/osquery/osquery)；官方 README 说明以 SQL 查询进程、端口及 macOS launchd 等系统状态 |

### 5. 资产重建档案

推演：每个重要资产保存配置仓库、备份位置、安装入口、恢复步骤引用以及最近恢复验证时间。场景是换机、磁盘损坏或环境被改坏后，确定能否恢复到可工作的状态。配置同步交给 chezmoi；包清单交给 [Homebrew Bundle](https://docs.brew.sh/Brew-Bundle-and-Brewfile)。`brew bundle dump` 保存受支持的已安装包清单，之后可据此安装；默认行为可能升级，不应描述为精确版本恢复。

AssetMesh 的新增价值是“恢复准备是否齐全、上次何时实际验证”的逐资产事实，不是再次实现已有备份导出，也不是重建配置管理工具。Codex 负责执行和诊断，验证结果进入档案。

### 6. 使用事实驱动的软件去留

推演：只导入 ActivityWatch 的日级使用汇总，映射到资产和已有费用；按需生成“付费却长期未用”“同类工具重复”等候选列表。原始浏览活动留在 ActivityWatch，AssetMesh 保存经用户确认的资产映射和去留结论。时长不等于价值，缺数据不等于未使用。

若只有一份临时使用报告，交给 Codex 即可；持续比较多个月、保留确认结果时才值得进入资产库。

### 7. 外部变化对个人资产的影响

推演：changedetection.io 负责监测价格、支持政策、兼容性说明等外部变化；AssetMesh 只把变化对应到自己确实拥有的资产与依赖，记录“受影响/不受影响/已处理”的确认结果。不要重建爬虫和通知引擎。

例：一个服务停止某项能力，重点是自己哪条工作流依赖它，以及替代是否已完成。Codex 可解释变更，持久映射与处理状态使下次检查不会重新从零开始。

### 8. 观察到的环境与用户记录的偏差

推演：以已有软件发现能力为起点保留两次盘点的差异，区分“系统观察到”与“用户确认在用”。例如卸载后残留的启动项、迁移后失效的安装位置、新出现但尚未归类的软件。osquery 提供现成的系统查询能力，但不必在第一版引入。

这是对已有发现流程的深化，而非新增一个监控平台。Codex 可以执行检查；持续保留盘点基线、处理决定和再次出现的偏差时，AssetMesh 才有独立价值。

## 收敛建议

最终讨论可收敛为六项：重建档案、使用与成本、变化影响、环境偏差、购买权益证据、依赖资料留存。设备到服务的映射可以作为重建档案的延伸。每项先用少量真实资产回答一个问题；如果现成工具加一次 Codex 对话就已解决，不增加 AssetMesh 功能。

不默认推荐连接器。轻量验证顺序可以是：重建档案先手写三项资产的引用与一次恢复记录；使用与成本先读取一个月导出文件；变化影响先用已有监测结果关联两项资产；环境偏差先比较两份软件清单；权益证据先挂五个文档链接；资料留存先为三份真正依赖的网页添加存档引用。出现反复更新且手工维护困难的需求后，再固化接口。

主线程补充核实：[ActivityWatch 官方示例](https://docs.activitywatch.net/en/latest/examples.html) 已涉及代理/AI 使用活动数据，进一步说明单次用量报表可以直接由 Codex 完成。资产映射与历次留用、停用决定的长期沉淀才是本项目的潜在增量。
