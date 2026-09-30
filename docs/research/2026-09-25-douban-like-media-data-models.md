# 类豆瓣媒体管理软件如何管理媒体数据 —— 五个开源项目的一手代码调研

> 日期：2026-09-25。方法：GitHub API 列目录 + 按 tag/commit 固定的 raw 文件阅读，未参考博客、文档站或二手总结。
> 由三个并行调研任务分别撰写后合并；每个项目一节，每节末尾附「关键证据清单」，每条论断都指向实际打开过的文件。
> 关注点：条目与用户记录的数据模型、外部元数据源与 ID 映射、已有数据的导入与匹配去重、增量同步。

## 仓库路径核实

调研任务给定的若干仓库路径是错的，下表是核实后的真实位置。后续引用一律使用真实路径。

| 任务中的写法 | 核实结果 | 说明 |
|---|---|---|
| `bangumi/server` | ✅ 存在 | 680★，`pushed_at` 2026-09-24，pin 在 tag `v1.12.10` |
| `Somtom/Ryot` | ❌ 404 → **`IgnisDa/ryot`** | 3606★，`pushed_at` 2026-09-25，pin 在 `v10.5.1` |
| `MediaTracker/MediaTracker` | ❌ 404 → **`bonukai/MediaTracker`** | 934★，最后提交 2025-02，准停更但未归档 |
| `kitsu-core/kitsu` | ❌ 404 → **`hummingbird-me/kitsu-server`** | 189★，默认分支 `the-future`，2026-06 仍活跃 |
| `jxxghp/MoviePilot` | ✅ 存在，但**简报里的路径全部失效** | 11802★，默认分支 `v3`，pin 在 `3428aed4`。`app/core/meta.py`、`app/helper/`、`app/modules/metatube*` 在 v3 的 DDD 重写中已不存在 |

两条**前提被证据推翻**的地方，记录在此以免后人再踩：

1. 「MediaTracker 的元数据源是声明式 YAML integration 文件」——**不成立**。全仓 YAML 只有 `codecov.yml` / `heroku.yml`；`server/src/metadata/provider/` 下是 4 个 TypeScript class 适配器。仓库里所谓 "Integrations" 指的是 Plex/Jellyfin/Kodi 的**播放态回传**，不是元数据源声明。结论：MediaTracker 与 AssetMesh 的 Rust provider adapter 属于同一种设计，而非反例。
2. 「MoviePilot 有豆瓣↔TMDB 的 ID 映射表」——**不存在**。跨源身份转换是按需求值的封闭规则表，结果不回写任何表（见 MoviePilot 一节 §1）。

## 结论摘要（TL;DR）

**七条跨项目规律：**

1. **条目与用户记录必须分表，且条目行上零用户状态。** 五个项目无一例外：bangumi `chii_subject_interests`、Kitsu `library_entries`、Ryot `seen` + `user_to_entity`、MediaTracker 把用户态拆成 `seen`/`userRating`/`progress`/`list` 四张互独立的表。条目表上那些 `subject_wish/collect/doing` 计数与 `field_rate_1..10` 全是**从 interest 表汇总出的派生值**。MediaTracker 甚至走到另一端——**根本没有统一的 status 列**，"某用户对某片的状态"是由那四张表推导出来的查询视图。
2. **外部身份一律是「命名空间 + 外部 ID」的复合键，并且唯一性带类型维度。** Ryot `UNIQUE(identifier, source, lot)`；Kitsu `mappings` 上 `UNIQUE(external_site, external_id, item_type, item_id)` 且站点词表**自带粒度命名空间**（`thetvdb/season`、`imdb/episodes`）；MediaTracker 用 8 个 `unique(xxxId, mediaType)` 复合索引；bangumi 是反例（只有一个 `subject_uid varchar(20)` 混装 isbn/imdb，未加唯一索引）。
3. **没有一个项目把 provider 原始 payload 落库。** 五处逐一确认无 raw JSON 列（Ryot 只有带 TTL 的 `application_cache`；Kitsu 的 `scrapes` 是 URL 血缘表而非响应快照；MoviePilot 的 `MediaInfo.tmdb_info/douban_info` 只活在内存对象里）。共同代价是**无法离线重放**，字段语义变了只能重新打远端。这与 AssetMesh 的 ADR 0009 立场一致。
4. **导入解析顺序高度趋同：本源外部 ID → 借交叉引用再试一次 → 标题+类型+集数的检索式猜测。** 最完整的是 Kitsu 三级：`Mapping.lookup("anilist/<type>", id)` → miss 则用 `idMal` 查 MAL 映射，**且一旦命中立刻回写一条 anilist 映射**（从交叉引用自举，下次不再二跳）→ 再 miss 才 `Mapping.guess`（Algolia 全文 + `kind:<type>` + `episodeCount:(n-2) TO (n+2)` 窗口剪枝）。Ryot 干脆没有 fuzzy：ISBN 换不出 ID 就记失败。
5. **增量刷新只有两种范式，且四种实现都保住了「用户手填不被覆盖」。** TTL 轮询（MediaTracker 每小时全库扫、`lastTimeUpdated` 24h/30d 双阈值、且只刷「有人在用」并排除 `source IN ('user','goodreads')`）vs 脏标记水位（Ryot 每轮把用户拥有的实体 `is_partial = true` 打回，`update_metadata` 第一句就是 `if !is_partial { return }`，刷新范围 = 一个内置的 `Monitoring` 片单）。Kitsu 只刷「在播子集」，MoviePilot 用显式字段白名单 + `manual_total_episode`/`max()` 护栏。
6. **结构性变更（删季/删集）是最容易毁掉用户数据的地方，唯一做对的项目值得照抄。** MediaTracker 的 `getItemsToDelete` 在删除前先查这些 `episodeId` 是否被 `seen`/`progress`/`listItem`/`userRating` 引用，**被引用则 throw 中止整次删除**，降级为"保留旧季/集结构、只更新头部"；重识别时用自然键 `seasonNumber`/`episodeNumber` 把新 payload 贴回旧主键，从而保住引用完整性。
7. **多语言标题的最轻解法是「指针列 + locale map」。** Kitsu 每张 media 表都有 `titles hstore` + `canonical_title`（存的是**指向 titles 哪个 key 的 locale 名，不是文本**），并强制 `titles` 里至少存在一个 `en*` 键；Ryot 用独立 `entity_translation` 表按语言建三个唯一索引；bangumi 只有 `subject_name` / `subject_name_cn` 两列。加 `title_cn`/`title_en` 列或另建 alias 表都比这两种重。

**最有价值的三个分歧：**

- **季/集到底落不落表。** 落表的三种形状差异极大：bangumi 是**扁平单层 episode 表、没有 season 表**（季在 bgm 里是独立 subject，季间靠 `chii_subject_relations` 串联，`ep_disc`+`ep_sort` 只做分组排序）；MediaTracker 是 **season + episode 双表**且各自带外部 ID、带自然键唯一约束；Kitsu 是 **episode 单表 + `season_number` 整型列**（"季"是对 episodes 的 group-by 结果）。唯一的例外 Ryot 把整季整集塞进 `show_specifics` JSON——代价是**无法对单集做行级查询与统计**，但它仍然要在 `seen.show_extra_information` 里记 `{season, episode}`。**结论：只要需要 per-episode 的用户记录，季/集就必须是行。**
- **人物是不是一等实体。** bangumi / Kitsu / Ryot 是（三种建模：bangumi 用 7 个带索引的**布尔身份列** + `person_fields` 用 `enum('prsn','crt')` 让角色与人物共用扩展表 + `crt_cast_index` 的三元主键 `(crt_id, prsn_id, subject_id)` 表达"声优↔角色↔作品"；Kitsu 用 `people`/`characters` + 多态 `castings`/`media_staff`；Ryot 用 `metadata_to_person` 主键 `(metadata_id, person_id, role)` 加一个 `character` 列，同一行既表达 staff 也表达 cast）。**MediaTracker 完全不是**——演职人员只是 `mediaItem` 上的扁平文本列，因此它永久丧失了"这个导演还拍过什么"这类查询能力。
- **片单/集合的存储形态。** bangumi `index` + `index_related` 的**行级成员**（`(idx_rlt_type, idx_rlt_sid, idx_rlt_order)`，支持人工排序）；MediaTracker 把 watchlist **折叠成一个带 `isWatchlist` 标记的特殊 list**，让"想看"与"自定义清单"共用一套存储，成员颗粒可细到季/集；Kitsu 服务端根本没有用户自定义清单（由社交 `Post` 承担），而是 `library_entries` 的 status 分桶 + `media_ignores`；Ryot `collection UNIQUE(name, user_id)` + `collection_to_entity` + 一张**反范式物化表** `collection_entity_membership` 供后台任务一次查全。


---

## 项目：bangumi/server（bgm.tv 后端）

- Stars / 最近提交：680 stars，`pushed_at` 2026-09-24T01:47:55Z（`GET /repos/bangumi/server`）；本次固定在 tag `v1.12.10`（commit `0a87b2b37c84a6ee6a12e4281104117c47674142`），`canal/` 目录在 v1.12.10 的 tree 里不存在，相关证据固定在 `master`。
- 数据载体：MySQL + GORM，且 **仓库内没有任何 `.sql` 迁移文件**（全仓 tree 里 `migration|\.sql$` 零命中）。表结构以"既存线上库为真源、代码反向生成"的方式存在：`internal/dal/dao/chii_*.gen.go` 是由 `internal/cmd/gen/gorm/main.go` 生成的 struct，字段 tag 里带完整列类型、`column:`、`primaryKey`、`index:`/`uniqueIndex:` 定义。也就是说 bangumi 的 schema 文档 = 这些 `.gen.go`。

### 1. 实体与用户记录建模

**条目 canonical identity**：内部自增 ID 为唯一真源，`chii_subjects.subject_id` 是 `mediumint(8) unsigned` 主键、`autoIncrement`。外部 ID 只有一个槽位：`subject_uid varchar(20) NOT NULL`，源码注释直接写着 `// isbn / imdb`。**没有外部 ID → 内部 ID 的映射表**，也没有 `provider + provider_id` 这类列；`subject_uid` 只是给 isbn/imdb 用的一个混合字符串字段，未加唯一索引。

**多媒介类型共用同一张表**：`subject_type_id`（列 `subject_type_id smallint unsigned`，参与 `subject_type_id`/`browser`/`order_by_name` 多个复合索引）做类型判别。`internal/model/subject_type.go` 定义 `SubjectType = uint8`：`1 书籍 / 2 动画 / 3 音乐 / 4 游戏 / 6 三次元`；`openapi/components/subject_type.yaml` 是公开 API 的同一份枚举（enum `[1,2,3,4,6]`，并明确写"没有 `5`"）。**注意 5 是个历史空洞，枚举值不连续**。所有类型共用 `chii_subjects` 的固定列（`subject_name` / `subject_name_cn` / `field_infobox` / `field_summary` / `field_5`("author summary") / `field_volumes` / `field_eps`），类型专有信息塞在 `field_infobox` 这个 `mediumtext` 里（wiki 文本），而不是 JSON 列。

**分表的是"附属数据"而不是"类型"**：`chii_subject_fields` 按 `field_sid`（与 subject 同值的主键，1:1）存放统计与播出信息（`field_tags mediumtext`、`field_rate_1..10`、`field_year`/`field_mon`/`field_week_day`/`field_date`、`field_rank`、`field_redirect`），`chii_subjects` 通过 `foreignKey:subject_id;references:field_sid` 关联。这样高频写入的评分计数/标签不污染主表行。

**季度与单集**：`chii_episodes` 是**扁平的一层单集表，没有 season 表**：`ep_id` 主键、`ep_subject_id`（外键到条目）、`ep_sort float`（排序号）、`ep_type tinyint`（`EpType = int16`，用于区分正片/SP/OP/ED 等，本次未取到常量定义）、`ep_disc tinyint unsigned`（注释"碟片数"）、`ep_name`/`ep_name_cn`/`ep_duration varchar`/`ep_airdate varchar`/`ep_online mediumtext`/`ep_desc`/`ep_rate`/`ep_lock`/`ep_ban`。
推断：因为一条"季度"在 bgm 里通常是独立 subject，季与季之间靠 `chii_subject_relations` 串联，`ep_disc` + `ep_sort` 只承担同一 subject 内的分组与排序；所以 bgm 的"季"不是 episodes 的父实体，而是条目级的概念。

**条目间关系**：`chii_subject_relations` 主键是 `(rlt_subject_id, rlt_related_subject_id, rlt_vice_versa)` 三列复合（同时也是 `uniqueIndex:rlt_subject_id`），另有 `rlt_relation_type smallint`（关联类型）、`rlt_subject_type_id`、`rlt_related_subject_type_id`、`rlt_order`。把 `vice_versa` 放进主键，等于用一行表达"有向关系"、用两行表达"互逆关系"。

**人物 / 角色 / cast（导演·演员·作者·声优）**：三张表分工明确。
- `chii_persons`：`prsn_id` 主键自增；人物身份不是枚举而是**一组布尔列** `prsn_producer / prsn_mangaka / prsn_artist / prsn_seiyu / prsn_writer / prsn_illustrator / prsn_actor`，每列单独建索引；`prsn_type tinyint` 区分"个人/公司/组合"；`prsn_infobox mediumtext` 又是 wiki 文本；`prsn_redirect` 指向另一 person ID。
- `chii_person_fields`：主键 `(prsn_cat enum('prsn','crt'), prsn_id)` —— **人物与角色共用同一张扩展属性表**，用 enum 做 polymorphic 判别（GORM tag `polymorphic:Owner;polymorphicValue:prsn`），存 `gender / bloodtype / birth_year / birth_mon / birth_day`。
- `chii_person_cs_index`（staff 表）：主键 `(prsn_type enum('prsn','crt'), prsn_id, prsn_position smallint, subject_id)`，`prsn_position` 就是"监督/原案/脚本"的 position 编码，附带 `subject_type_id`、`summary`、`prsn_appear_eps`（可选，参与的具体集数）。**同一张表既能挂人物也能挂角色**（靠 `prsn_type` enum）。
- `chii_crt_cast_index`（cast 表，声优↔角色↔条目）：主键 `(crt_id, prsn_id, subject_id)`，加 `subject_type_id` 索引、`summary varchar(255)`（注释"幼年，男乱马，女乱马，变身形态"）。
- `chii_crt_subject_index`（角色↔条目）：主键 `(crt_id, subject_id)`，`crt_type`（主角/配角）、`crt_order tinyint`（显示顺序）、`ctr_appear_eps mediumtext`。
即 bgm 把"某人以某身份参与某条目的某些集"建模为**一条带 position/role 编码和 appear_eps 文本的关系行**，声优—角色—作品是三元关系而非二元关系。

**"同一个人来自两个源"的处理**：没有跨源 ID 映射，只有 **redirect 列**：`chii_subjects.subject_redirect`（`model.Subject` 里有 `Redirect SubjectID`）、`chii_subject_fields.field_redirect`、`chii_persons.prsn_redirect`。别名/改名走 `chii_rev_history` + `chii_rev_text`（见下），不建 alias 表。

**片单 / collections**：叫 index。`chii_index`（`idx_id` 主键、`idx_type`、`idx_title`、`idx_desc`、`idx_stats`、`idx_uid` 创建人、`idx_subject_total`、`idx_collects`、`idx_ban`）+ `chii_index_related`（`idx_rlt_id` 主键自增，`idx_rlt_rid` = 所属 index，`idx_rlt_cat`、`idx_rlt_type`（小写：被收录对象的类型，subject/character/person/episode/…）、`idx_rlt_sid`（被收录对象 ID）、`idx_rlt_order`、`idx_rlt_comment`）。**片单成员是 (index, type, subject_id) 的行，不是 JSON 数组**，且有 `idx_order` 复合索引支持人工排序。注意 `idx_rlt_sid` 只是裸 ID + type 判别，没有 DB 级外键。

**用户自己的记录 = 独立表，绝不是条目上的字段**：`chii_subject_interests`（GORM struct 名 `SubjectCollection`）。
- 唯一键 `uniqueIndex:user_interest` = `(interest_uid, interest_subject_id)` —— 每个用户对每个条目**最多一行**。
- 状态 `interest_type tinyint`：`openapi/components/collection_type.yaml` 给出公开枚举 `1 想看 / 2 看过 / 3 在看 / 4 搁置 / 5 抛弃`（`internal/model/collection_type.go` 中 `CollectionTypeWish/Done/Doing/OnHold/Dropped`，另有 `CollectionTypeAll = 0` 仅作查询哨兵）。
- 评分 `interest_rate tinyint`、评论 `interest_comment mediumtext`、`interest_has_comment`、**用户私有标签 `interest_tag mediumtext`**（与条目级 `field_tags` 分开）。
- 进度：`interest_ep_status`、`interest_vol_status`（都只是整数计数，**没有 per-episode 的观看记录表**；repo 内 `ep_status` 关键字零命中）。
- 时间戳：每个状态各一个 dateline（`interest_wish_dateline` / `interest_doing_dateline` / `interest_collect_dateline` / `interest_on_hold_dateline` / `interest_dropped_dateline`）+ `interest_lasttouch`，另有 `interest_private`。
- 条目表上那些 `subject_wish / subject_collect / subject_doing / subject_on_hold / subject_dropped` 与 `field_rate_1..10` 是**从 interest 表汇总出的计数器**，不是用户状态本身。

### 2. 外部元数据与 ID 映射

bgm **本身就是元数据源**，所以没有 provider 抽象层：仓库里搜不到 provider/adapter/normalizer 之类的东西，也没有第三方抓取路径。对外只有两个 ID 落点：`subject_uid`（isbn/imdb，varchar(20)、非唯一）和人物表上的 `prsn_img_anidb`（残留的 anidb 图片 ID），说明历史上做过 anidb 对齐但**没有保留映射表**。
推断：多源/重复条目在 bgm 的解法是人工合并 + redirect 列（`subject_redirect` / `field_redirect` / `prsn_redirect`），而不是自动 ID 对齐。

**原始 provider payload 不落地**：`chii_subjects` 只有 `field_infobox mediumtext`（wiki 语法文本，`pkg/wiki` 有自己的解析器和 `wiki-syntax-spec` 子模块）与 `field_summary`，没有任何 raw JSON 列。用户侧的原始输入（tags）也立刻被归一：`model.Subject` 里 `CompatRawTags []byte // compat field for old tags` 明示这是为兼容旧格式保留的二进制 blob。

**字段优先级**：不是"provider vs user"的优先级，而是 **"revision vs 当前行"**。`chii_rev_history`（`rev_id` 主键自增、`rev_type tinyint`（"条目，角色，人物"）、`rev_mid`（对应条目/人物 ID）、`rev_text_id`、`rev_dateline`、`rev_creator`、`rev_edit_summary varchar(200)`）+ `chii_rev_text` 存每次编辑的完整快照；`internal/model/revision.go` 里 `SubjectRevisionData{ Name, NameCN, VoteField, FieldInfobox, FieldSummary, Platform, TypeID, SubjectID, FieldEps, Type }` 就是"待审/已审的一版条目字段"，`PersonRevisionData map[string]PersonRevisionDataItem`（key 形如 `prsn_name`/`prsn_infobox`，用 `mapstructure` tag 对齐老库列名）。
即：**内容修改一律先写 revision，通过后才回写主表**，`rev_type + rev_mid + rev_id` 提供可回溯的字段级历史。多语言只有固定两列 `subject_name` / `subject_name_cn`（`NameCN`），没有 alias/translation 表。

### 3. 导入 / 迁移已有数据

仓库内**没有任何面向用户的导入/迁移机制**：`encoding/csv` 在 GitHub code search 里 `total_count = 0`；`internal/cmd/` 下只有 `archive/main.go`、`gen/gorm`、`gen/generic`、`gen/openapi`（代码生成器与归档任务），没有 importer/seed/migrate 命令。

- 推断：数据入库发生在 PHP 时代与人工编辑路径（`chii_rev_*` + `web/handler` 的 edit 流程），Go 服务只读多写少；`internal/compat` 包名与 `CompatRawTags` 说明仓库的主要"迁移"负担是**向后兼容旧字段格式**，而不是导入外部库。
- 唯一可借鉴的既有导入痕迹是 `subject_uid` 承载 isbn/imdb，供按外部编号精确查条目（无评分、无 fuzzy、无待审队列）。
- 结论：bangumi/server 在"把已有片单搬进来"这件事上没有可抄的代码路径；它的价值全部在 §1 的表结构上。

### 4. 增量同步

- 方向：**内部单向**。没有 last-sync watermark、没有 tombstone 表、没有对外拉取。
- 机制：`canal/readme.md`（master）写明"基于 debezium 和 kafka 的 binlog 订阅，暂时用于处理用户密码修改"。`canal/on_subject.go` 里 `OnSubject` / `OnSubjectField` 反序列化 `{subject_id}` / `{field_sid}` 作 key，然后按 `payload.Op` 分发到 `internal/search`：`opCreate → EventAdded`、`opUpdate | opSnapshot → EventUpdate`、`opDelete → EventDelete`。
  即：**变更捕获用 binlog 事件 + 一个快照回填 op，下游只有搜索引擎索引**（`SearchTargetSubject`），MySQL 行本身仍是唯一权威。
- 缓存/失效：`internal/cache` 包存在；`chii_*` 表里的 `*_lasttouch`、`*_lastpost` 列（如 `interest_lasttouch`、`ep_lastpost`、`prsn_lastpost`）是供列表排序与增量重建用的时间戳水位。
- 软删除/封禁：没有 `deleted_at`，统一用 `*_ban`（`subject_ban`、`ep_ban`、`prsn_ban`、`idx_ban`）+ `*_lock`；`model.Subject.Locked()` 判定 `Ban == 2`（常量 `subjectLocked = 2`）。**封禁状态是带枚举语义的整数，不是布尔**。

### 关键证据清单

- [internal/dal/dao/chii_subjects.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_subjects.gen.go) — 条目主表全部列名/类型/索引，含 `subject_uid` = isbn/imdb、`subject_type_id` 判别、`subject_redirect`、五个状态计数器。
- [internal/dal/dao/chii_subject_fields.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_subject_fields.gen.go) — 1:1 附属表：`field_tags`、`field_rate_1..10`、`field_date`、`field_redirect`，证明"统计与播出信息拆表"。
- [internal/dal/dao/chii_episodes.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_episodes.gen.go) — 单集表扁平、无 season 父表；`ep_sort`/`ep_type`/`ep_disc` 三列承担排序、类型与分组。
- [internal/dal/dao/chii_subject_interests.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_subject_interests.gen.go) — 用户记录独立表，`uniqueIndex:user_interest = (interest_uid, interest_subject_id)`，含 `interest_rate`/`interest_tag`/`interest_ep_status`/每状态 dateline/`interest_private`。
- [internal/dal/dao/chii_persons.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_persons.gen.go) — 人物身份是 7 个带索引的布尔列 + `prsn_redirect` + `prsn_infobox` wiki 文本。
- [internal/dal/dao/chii_person_fields.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_person_fields.gen.go) — 主键 `(prsn_cat enum('prsn','crt'), prsn_id)`，人物与角色共用扩展表。
- [internal/dal/dao/chii_person_cs_index.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_person_cs_index.gen.go) — staff 关系四列主键 + `prsn_position`（监督/原案/脚本）+ `prsn_appear_eps`。
- [internal/dal/dao/chii_crt_cast_index.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_crt_cast_index.gen.go) — 声优↔角色↔条目三元主键 `(crt_id, prsn_id, subject_id)`，`summary` 存形态备注。
- [internal/dal/dao/chii_crt_subject_index.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_crt_subject_index.gen.go) — 角色↔条目 + `crt_type`（主角/配角）+ `crt_order`。
- [internal/dal/dao/chii_subject_relations.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_subject_relations.gen.go) — 条目互链，主键含 `rlt_vice_versa`，用行表达方向性。
- [internal/dal/dao/chii_index.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_index.gen.go) / [chii_index_related.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_index_related.gen.go) — 片单主表 + `(idx_rlt_type, idx_rlt_sid, idx_rlt_order)` 的行级成员表。
- [internal/dal/dao/chii_rev_history.gen.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/dal/dao/chii_rev_history.gen.go) — `rev_type`/`rev_mid`/`rev_text_id`/`rev_creator`/`rev_edit_summary`，revision 概念的实现。
- [internal/model/revision.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/model/revision.go) — `SubjectRevisionData` / `PersonRevisionData`（`mapstructure` tag 直接映射老库列名），证明"编辑先写版本再回写主表"。
- [internal/model/subject.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/model/subject.go) — `Subject`/`Episode`/`Platform` 业务侧 struct、`subjectLocked = 2`、`CompatRawTags []byte // compat field for old tags`。
- [internal/model/subject_type.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/model/subject_type.go) / [openapi/components/subject_type.yaml](https://raw.githubusercontent.com/bangumi/server/v1.12.10/openapi/components/subject_type.yaml) — 公开 API 的 subject type enum `1/2/3/4/6`，明确"没有 5"。
- [internal/model/collection_type.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/model/collection_type.go) / [openapi/components/collection_type.yaml](https://raw.githubusercontent.com/bangumi/server/v1.12.10/openapi/components/collection_type.yaml) — 想看/看过/在看/搁置/抛弃 的五值枚举与 0=All 哨兵。
- [internal/model/type.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/model/type.go) — `SubjectID`/`EpisodeID`/`PersonID`/`CharacterID`/`UserID`/`GroupID` 全是不同 newtype 并实现 `driver.Valuer`，跨类型 ID 不会互相传错。
- [openapi/components/subject_collection.yaml](https://raw.githubusercontent.com/bangumi/server/v1.12.10/openapi/components/subject_collection.yaml) — 用户记录对外形状：`type`/`rate`/`tags`/`ep_status`/`vol_status`/`updated_at`/`private` 全 required。
- [canal/readme.md](https://raw.githubusercontent.com/bangumi/server/master/canal/readme.md) / [canal/on_subject.go](https://raw.githubusercontent.com/bangumi/server/master/canal/on_subject.go) — debezium+kafka 的 binlog 订阅，op 分发到 search 的 Added/Update/Delete，是唯一的增量通道。
- [internal/cmd/gen/gorm/main.go](https://raw.githubusercontent.com/bangumi/server/v1.12.10/internal/cmd/gen/gorm/main.go) — 配合"仓库无 .sql 迁移、schema 由线上库反向生成"的判断。

---

## 项目：IgnisDa/ryot（任务里写作 Somtom/Ryot，该名字 404）

- Stars / 最近提交：3606 stars，`pushed_at` 2026-09-25T03:03:14Z（`GET /repos/IgnisDa/ryot`）；固定在 tag `v10.5.1` / commit `302931fac46843696f2706f435f08c8d988bf3ab`。
- 数据载体：**PostgreSQL + SeaORM**，迁移是 Rust 代码而非 SQL 文件（`crates/migrations/sql/src/m2023*_*.rs`，`sea_orm_migration` + `DeriveMigrationName`，`Iden` enum 即列名）。实体 struct 在 `crates/models/database/src/*.rs`，用 `#[sea_orm(primary_key)]` / `DeriveActiveEnum`。仓库同时有 `apps/website/app/drizzle/migrations/*.sql`（Drizzle），但那是官网站自己的库，不是 tracker 主库。

### 1. 实体与用户记录建模

**canonical identity = (内部 UUID/text id) + (外部 identifier, source, lot) 三元组**。`metadata` 表：`id text PRIMARY KEY`，同时建 **唯一索引 `metadata-identifier-source-lot__unique-index` on `(identifier, source, lot)`**。也就是"同一个外部 ID 在不同 MediaLot 下允许各存一行"（例如 TMDB 的某人/某作既当 Movie 又当 Show）。`source` 列存 `MediaSource` 枚举、`identifier` 存 provider 原始 ID 字符串，**没有独立映射表**，映射就是这两列。

**多类型共用一张表 + per-type JSON 列（"混合"方案）**：类型判别列是 `lot text NOT NULL`，枚举在 `crates/models/enum/src/media_enums.rs`：`MediaLot = Book | Show | Movie | Anime | Manga | Music | Podcast | AudioBook | VideoGame | ComicBook | VisualNovel`（`rename_all = "snake_case"`，DB 里存小写串）。
专有字段不是 per-type 表，而是 **每种 lot 一个 `json_binary` 列**：`movie_specifics / show_specifics / book_specifics / anime_specifics / manga_specifics / music_specifics / podcast_specifics / audio_book_specifics / video_game_specifics / comic_book_specifics / visual_novel_specifics`，加上 `watch_providers`、`external_identifiers`、`assets`、`free_creators` 也都是 JSON。公共列只有 `title / description / publish_year / publish_date / production_status / original_language / provider_rating / is_nsfw / is_partial / source_url`。
`media_enums.rs` 里还有一段非常重要的**静态约束**：`meta! { MediaLot, Vec<MediaSource> }` 显式声明每种 lot 允许哪些 provider（`Show => [Tmdb, Tvdb]`、`Book => [Hardcover, Openlibrary, GoogleBooks]`、`Music => [Spotify, MusicBrainz, YoutubeMusic]` …），并有反向 `meta! { MediaSource, Option<MediaLot> }` 给"只服务单一 lot 的 provider"填默认 lot。

**季度与单集：完全 JSON 化，不落表。** `crates/models/media/src/media_specifics.rs`：`ShowSpecifics { runtime, seasons: Vec<ShowSeason>, total_seasons, total_episodes }`，`ShowSeason { id, name, season_number, overview, episodes: Vec<ShowEpisode>, poster_images, backdrop_images, publish_date }`，`ShowEpisode { id, name, episode_number, runtime, overview, poster_images, publish_date }`；`PodcastSpecifics { total_episodes, episodes: Vec<PodcastEpisode> }`；`AnimeSpecifics { episodes, next_ep_airing, airing_schedule }`。
用户的观看进度则**带 season/episode 数字**：`crates/models/media/src/user_interactions.rs` 里 `SeenShowExtraInformation { season: i32, episode: i32, full: Option<bool> }`、`SeenPodcastExtraInformation { episode }`、`SeenAnimeExtraInformation { episode }`、`SeenMangaExtraInformation { volume, chapter }`，作为 `seen` 表的 JSON 列存在。
推断：因为整季整表 JSON 覆盖写，Ryot 无法对单集做行级查询/统计，代价换来了 provider 结构零转换。

**人物与 credits**：`person` 表 `id text PK` + **唯一索引 `person-identifier-source__unique_index` on `(identifier, source, source_specifics) NULLS NOT DISTINCT`** —— 把 `source_specifics` 纳入唯一键，用来区分"同一 provider 下同一 identifier 但额外限定不同"的人（例如 TMDB person 与带 `tvdb_id` 的 Tvdb person）。
`person` 列包含 `name / description / gender / birth_date / death_date / place / website / is_partial / source_specifics JSON / state_changes JSON / alternate_names TEXT[] / assets JSON`；还有三个**由 JSON 长度算出的 STORED 生成列**：`associated_metadata_count`、`associated_metadata_groups_count`、`associated_entity_count`（`GENERATED ALWAYS AS (COALESCE(JSONB_ARRAY_LENGTH("state_changes"->'metadata_associated'),0)) STORED`）—— 用生成列把 JSON 反范式计数变成可索引整数。
关系表 `metadata_to_person`：复合主键 `pk-media-item_person (metadata_id, person_id, role)`，另有 `index integer`（展示顺序）与 **`character` 列**（同一行既表达 staff 也表达 cast+饰演角色）。`metadata_group_to_person` 同构。
"同一个人来自两个源"= **不合并，按 `(identifier, source)` 各存一行**；`PersonSourceSpecifics` 参与唯一键即为此设计。角色不是实体，是 `metadata_to_person.character` 字符串。

**列表 / 片单**：`collection` 表极简（`id`、`name`、`user_id`、`description`、`information_template JSON`、`created_on`、`last_updated_on`），**唯一键 `(name, user_id)`**；成员在 `collection_to_entity`（按 `metadata_id` / `person_id` / `metadata_group_id` / `collection_id` 等分列，见 `merge_metadata` 与 `refresh_collection_to_entity_association` 的用法），另有一张 `collection_entity_membership`（`entity_id` + `entity_lot` + `collection_name` + `user_id`）专门做"哪个用户在哪个片单里有这个实体"的**反范式物化表**，供通知/后台任务一次查全。
`information_template JSON` 让片单自带结构化补充字段模板（"自定义片单信息"），值存在成员的 collection_to_entity 侧。

**用户记录 = 独立表，两层。**
- 归属连接层 `user_to_entity`：对 metadata / person / exercise / metadata_group / collection **各建一个可空 FK 列**，用 CHECK 约束 `user_to_entity__ensure_one_entity` 保证"恰好一个非空"，再用两个 **STORED 生成列 `entity_id = COALESCE(...)` 与 `entity_lot = CASE WHEN ... END`** 把 polymorphic 归一成 `(user_id, entity_lot, entity_id)` 可查询形状；并针对每类分别建 `(user_id, metadata_id)`、`(user_id, person_id)` … 的**部分唯一索引**。这张表上还挂 `needs_to_be_updated boolean`、`media_reason TEXT[]`、`collection_extra_information JSON`。注释写明"用户与实体的关联成立条件：有 seen 记录 / 在片单里 / 写过评论"。
- 状态层 `seen`（`crates/models/database/src/seen.rs`）：`id`(PK) / `user_id` / `metadata_id` / **`state: SeenState`** / **`progress: Decimal`** / `num_times_updated` / `review_id` / `started_on` / `finished_on` / `last_updated_on` / `updated_at: Vec<DateTimeUtc>` / `manual_time_spent` / `providers_consumed_on: Vec<String>`（在哪些平台看的）/ 各 lot 的 `*_extra_information` JSON。
- 评分/评论单独在 `review` 表（`merge_metadata` 里 `review::ActiveModel { metadata_id, ... }` 可证 review 也指向 metadata）。
即：**想看/在看/看过状态在 `SeenState` + `user_to_entity` 上，进度在 `seen.progress`，条目行本身零用户状态字段**。

**其他实体表**：`metadata_group`（系列/合集，例如 TMDB collection）、`metadata_to_metadata_group`（PK `(metadata_id, metadata_group_id)` + `part integer`）、`metadata_to_metadata`（PK 自增 `id`，**唯一索引 `(from_metadata_id, relation, to_metadata_id)`**，`relation` 目前只有 `Suggestion`）、`genre`、`metadata_to_genre`、`entity_translation`、`calendar_event`、`daily_user_activity`、`application_cache`、`integration`、`notification_platform`、`access_link`、`filter_preset`、健身相关（exercise/workout/…）。

### 2. 外部元数据与 ID 映射

**provider 抽象 = 一个 trait + 两个 dispatch 函数**。`crates/traits/src/lib.rs` 定义 `pub trait MediaProvider`，方法集合是 `search_media / get_media_details / search_people / get_person_details / search_metadata_groups / get_metadata_group_details / get_trending_media / translate_metadata / translate_metadata_group / translate_person`（trait 自带 `translate_*` —— 翻译是 provider 契约的一等公民）。
`crates/utils/dependent/provider/src/lib.rs` 里 `get_metadata_provider(lot, source, ss)` 用 `match source { MediaSource::Tmdb => match lot { ... }, MediaSource::Anilist => match lot { ... }, ... }` 把 `(MediaLot, MediaSource)` 映射到 `Box<dyn MediaProvider>`，另有 `get_non_metadata_provider(source, ss)` 给"只要人物/系列能力"的场景。注意 **`MediaSource::Custom => return err()`** —— 用户手建的条目在架构上被明确排除在 provider 体系外，永远抓不到东西。
`MediaSource` 枚举（同 `media_enums.rs`）：`Igdb | Tmdb | Tvdb | Vndb | Custom(默认) | Metron | Itunes | Anilist | Audible | Spotify | MusicBrainz | GiantBomb | Hardcover | Myanimelist | Listennotes | GoogleBooks | Openlibrary | MangaUpdates | YoutubeMusic`，同样 `rename_all = "snake_case"` 存库。

**外部 ID 映射的确切键**：`metadata UNIQUE (identifier, source, lot)`、`person UNIQUE (identifier, source, source_specifics) NULLS NOT DISTINCT`。此外还有第二层"同一行里的其他外部编号"：`metadata.external_identifiers JSON` 列（`MetadataExternalIdentifiers`，update 时整列覆盖写入），以及 `person.tvdb_id`、`MetadataIdentifiers { tvdb_id: Option<i32> }` 之类的 provider 交叉编号。
推断：需要按 IMDB/ISBN 等第三方编号反查时走 `external_identifiers` JSON，不是一等列，因此无法用唯一约束防重。

**原始 payload 是否落地：不落地。** `metadata` / `person` 两张表都**没有 raw/original JSON 列**，provider 返回的结构在 `crates/providers/<name>/src/models.rs` 里被映射成 `PartialMetadataWithoutId` 等归一类型后，只有归一值进库（见 `update_metadata` 的逐字段 `ActiveValue::Set`）。唯一接近"原始缓存"的是 `application_cache` 表（`expire_cache_keys` / `remove_cached_metadata_after_updates` 用它做结果缓存），且它是带过期时间的缓存而非审计副本。
**多语言/别名**：`person.alternate_names TEXT[]`；条目/人物/系列的翻译在独立的 `entity_translation` 表 —— 列 `id`、`language`、`variant`、`value`、`entity_lot`、`metadata_id`、`metadata_group_id`、`person_id`、`show_extra_information JSON`、`podcast_extra_information JSON`，并**按语言分别建三个唯一索引** `(language, metadata_id, variant, show_extra_information, podcast_extra_information)`、`(language, metadata_group_id, variant)`、`(language, person_id, variant)`；翻译有独立后台任务（`MpApplicationJob::UpdateMediaTranslation` → `update_media_translation`）。
**字段优先级**：条目行本身完全归 provider（`update_metadata` 用 provider 值无条件覆盖 title/assets/description/specifics/…），**用户状态不在这些列上，所以不存在字段级冲突**。冲突处理体现在两处：(a) `commit_metadata` 里已存在记录时只允许补 `image`（`if let Some(i) = data.image`），其余字段不动；(b) 自定义条目（`create_custom_metadata` / `update_custom_metadata`，`source = Custom`）是唯一"用户即 provider"的路径。

### 3. 导入 / 迁移已有数据

**入口与调度**：`SingleApplicationJob::ImportFromExternalSource(user_id, input)` → `crates/services/importer/src/lib.rs::perform_import`；`perform_import` 里 `match input.source { ImportSource::Igdb => igdb_importer_service::import(...), ImportSource::Plex => ..., ImportSource::Trakt => ..., ImportSource::Goodreads => ..., }`，覆盖 20 个来源（`crates/services/importer/{anilist,audiobookshelf,generic-json,goodreads,grouvee,hardcover,hevy,igdb,imdb,jellyfin,mediatracker,movary,myanimelist,netflix,open-scale,plex,storygraph,strong-app,trakt,watcharr}`），输入形态有三种：CSV/文件路径（`input.path`，如 watcharr/trakt/grouvee）、URL+key（plex）、账号名（anilist/mal）、JSON 数组（generic-json / 导出文件回灌）。
各 importer 只做"把外部格式转成统一的 `ImportResult`"，随后统一交给 **`crates/utils/dependent/import/src/lib.rs::process_import(is_import: bool, user_id, import, ss, on_item_processed)`** —— 同一个函数被 import 和 export 两条路复用（第一个 bool 参数切换方向）。

**匹配策略：外部 ID 优先，精确命中，无 fuzzy 评分。** `process_import` 开头就按 `HashMap<(MediaSource, String /*identifier*/, MediaLot), ImportOrExportMetadataItem>` 聚合，key 就是三元组；入库走 `commit_metadata(data: PartialMetadataWithoutId, ss, ensure_updated)`，其内部 `Metadata::find().filter(Identifier.eq).filter(Lot.eq).filter(Source.eq).one()`。
没有外部 ID 的源要先"换 ID"：`crates/services/importer/goodreads/src/lib.rs` 里 ISBN13 → `dependent_provider_utils::get_identifier_from_book_isbn(isbn, ...)` 得到 `(identifier, source)`（映射到 Google Books / Hardcover / Openlibrary ID），ISBN 非法或换不出 ID 就直接记失败项，**不做标题模糊匹配**。
**待审/人工复核队列：没有。** 取而代之的是"部分态"：`commit_metadata` 找不到记录时插入一个 stub —— `is_partial = Some(true)`、只带 `identifier/lot/source/title/publish_year/image`，随后由后台任务用 provider 详情补齐。
**失败记录 & 进度**：`import_report` 表（`id`、`user_id`、`source text NOT NULL`、`started_on`、`finished_on`、`was_success boolean NULL`、`details JSON`、`progress decimal`、`source_result JSON`、`estimated_finish_time NOT NULL`）；失败条目形状在 `crates/models/importer/src/lib.rs`：`ImportFailedItem { identifier, step: ImportFailStep, lot, error }`，`ImportFailStep = ItemDetailsFromSource | InputTransformation | MediaDetailsFromProvider | DatabaseCommit` —— **四阶段失败分类 + 每条失败都带回原始 identifier**，这就是它的复核材料。`ImportResultResponse { import: ImportDetails{total}, failed_items }`。
**幂等重跑**：靠 `(identifier, source, lot)` 唯一键 + stub 语义天然幂等（重复导入只会聚合同一行、追加 seen/review/collection）。超时任务被显式回收：`invalidate_import_jobs` 把 `was_success IS NULL AND estimated_finish_time < now()` 的报告置为失败。
**去重与合并**：`crates/utils/dependent/entity/src/lib.rs::merge_metadata(ss, user_id, merge_from, merge_into)` —— 事务内把 `seen`、`review`、`collection_to_entity` 从源行**复制成新 ID 的行再删旧行**（seen 会重置 `id/last_updated_on/num_times_updated` 以避免时间线冲突），即"合并 = 搬迁用户状态，条目本体留给 provider 权威"。
**本地媒体文件夹匹配**：`crates/services/miscellaneous/lookup/src/{matching.rs, patterns.rs, extractors.rs}` 加一组测试（`episode_extraction.rs`、`best_match.rs`、`netflix_formats.rs`、`show_information.rs`）说明存在"从文件名解析 S/E 并挑最佳匹配"的能力（供 plex/jellyfin/kodi 等 sink 使用）。本次未逐个展开这些文件。

### 4. 增量同步

**调度器**：`apps/backend/src/job.rs` 用 apalis + `apalis_cron::Tick`，两个 cron 入口：`run_infrequent_cron_jobs → perform_background_jobs`、`run_frequent_cron_jobs → yank_integrations_data + process_users_scheduled_for_workout_revision + invalidate_import_jobs + cleanup_user_and_metadata_association`。任务分优先级队列 `HpApplicationJob / MpApplicationJob / LpApplicationJob / SingleApplicationJob`。
**"远端变更怎么发现"的核心是 `is_partial` 水位 + 反向标记**：
1. `perform_background_jobs`（`crates/services/miscellaneous/background/src/lib.rs`）依次跑：`update_all_monitored_metadata_and_notify_users`、`update_all_monitored_people_and_notify_users`、`notify_users_for_released_media`、`recalculate_calendar_events`、`sync_integrations_data_to_owned_collection`、`remove_useless_data`、`remove_cached_metadata_after_updates`、**`put_entities_in_partial_state`** …
2. `put_entities_in_partial_state`（`background/src/cleanup.rs`）从 `user_to_entity` 里取出所有非空 `entity_id`，分块 `UPDATE ... SET is_partial = true` —— **凡是任何用户拥有的实体，每轮都被打回"部分态"**。
3. `update_metadata(metadata_id, ss)`（`crates/utils/dependent/entity/src/lib.rs`）第一件事就是 `if !meta.is_partial.unwrap_or_default() { return Ok(default) }`，只有部分态才去 `details_from_provider(lot, source, identifier)`，成功后把 `is_partial = None` 并**整行覆盖所有 provider 字段**，再 `change_metadata_associations(genres, suggestions, groups, people)` 重建关系，最后 `expire_metadata_details_cache`。
   即：**provider 永远权威、覆盖写、无字段级三方合并**；用户自己的数据因为在别的表里所以不受影响。
4. `ensure_metadata_updated`（同文件）提供导入路径的重试：若 `ensure_updated = Some(true)` 且记录仍是 partial，则 `deploy_update_media_entity_job` + 指数退避 `2^(n+1)` 秒，最多 `MAX_IMPORT_RETRIES_FOR_PARTIAL_STATE` 次 —— 保证"先建 stub、等详情到位后再写用户状态"。
**字段级 diff 只用于生成通知，不用于合并**：`generate_metadata_update_notifications(&meta, &details)` 逐条比较 `production_status`、`publish_year`/`publish_date`，并对 show 逐季逐集比较 `seasons.len()`、`episodes.len()`、`before_episode.name != after_episode.name`、`poster_images`、`publish_date`，anime/manga/podcast 各有对应分支；产出 `UpdateMediaEntityResult { notifications }`。
`update_metadata_and_notify_users`（`crates/utils/dependent/notification/src/lib.rs`）拿到通知后，用 `get_users_and_cte_monitoring_entity` 找出谁在 Monitoring 片单里，逐个 `send_notification_for_user` + `refresh_collection_to_entity_association`（把 `collection_to_entity.last_updated_on = now()`）—— 让"我关注的项目有新集了"在 UI 里排序上浮。
**被同步对象的选择**：`background/src/monitoring.rs::get_monitored_entities` 查 `collection_entity_membership` 里 `collection_name == DefaultCollection::Monitoring` 的行，聚合成 `HashMap<entity_id, HashSet<user_id>>` 再分块并发 —— **"增量同步的范围"就是一个内置片单，而不是全库**。
**监控范围**：只有 `EntityLot::Metadata / Person / MetadataGroup` 支持 `UpdateMediaDetails`，`job.rs` 里其他 lot 直接 `Err("Type {:?} not supported for update")`。
**外部服务双向**：`crates/services/integration/src/{push, sink}` —— `sink/{plex,jellyfin,emby,kodi,generic_json,ryot_browser_extension}` 是**入站**（观看事件回传，`ProcessIntegrationWebhook`），`push/{plex,radarr,sonarr,jellyfin}` 是**出站**（把片单/状态推给 *arr），`sync_integrations_data_to_owned_collection` 把外部数据落到一个"拥有者片单"里。
**软删除/墓碑**：没有 deleted_at 与 tombstone 列；靠 `cleanup_user_and_metadata_association` 周期删除已无意义的 `user_to_entity` 关联，以及 `disassociate_metadata`。`MetadataToMetadata` 的唯一键含 `relation`，关系变更是删行+插行。

### 关键证据清单

- [crates/migrations/sql/src/m20230410_create_metadata.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20230410_create_metadata.rs) — `metadata` 全列与 **唯一索引 `(identifier, source, lot)`**、每 lot 一个 `json_binary` specifics 列、`is_partial`、`last_updated_on`、trigram 索引；无任何 raw payload 列。
- [crates/models/enum/src/media_enums.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/models/enum/src/media_enums.rs) — `MediaLot`、`MediaSource` 两个枚举与 `meta!` 声明式"哪个 lot 允许哪些 provider"。
- [crates/migrations/sql/src/m20230413_create_person.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20230413_create_person.rs) — `person` 唯一键 `(identifier, source, source_specifics) NULLS NOT DISTINCT`、`alternate_names TEXT[]`、三个 `state_changes` 派生计数生成列；`metadata_to_person` PK `(metadata_id, person_id, role)` + `index` + `character`。
- [crates/models/media/src/media_specifics.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/models/media/src/media_specifics.rs) — `ShowSpecifics/ShowSeason/ShowEpisode`、`PodcastSpecifics/PodcastEpisode`、`AnimeSpecifics`、`MusicSpecifics` 的 JSON 内嵌结构（季/集不落表）。
- [crates/migrations/sql/src/m20231017_create_user_to_entity.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20231017_create_user_to_entity.rs) — polymorphic 归属表：`ensure_one_entity` CHECK + `entity_id`/`entity_lot` STORED 生成列 + 每类 `(user_id, x_id)` 唯一索引。
- [crates/models/database/src/seen.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/models/database/src/seen.rs) — `seen`：`state`/`progress`/`num_times_updated`/`started_on`/`finished_on`/`manual_time_spent`/`providers_consumed_on`/各 lot extra JSON。
- [crates/models/media/src/user_interactions.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/models/media/src/user_interactions.rs) — `SeenShowExtraInformation{season,episode}`、`SeenMangaExtraInformation{volume,chapter}`、`PersonStateChanges`。
- [crates/migrations/sql/src/m20230504_create_collection.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20230504_create_collection.rs) — 片单表本体与 `UNIQUE(name, user_id)`、`information_template JSON`。
- [crates/migrations/sql/src/m20231219_create_metadata_relations.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20231219_create_metadata_relations.rs) — `metadata_to_metadata` 唯一 `(from, relation, to)`、`metadata_to_metadata_group` PK + `part`。
- [crates/migrations/sql/src/m20251128_create_entity_translation.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20251128_create_entity_translation.rs) — 多语言标题/简介独立表与按语言的三个唯一索引。
- [crates/traits/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/traits/src/lib.rs) — `trait MediaProvider` 的 8 个方法（含 `translate_*`），provider 抽象的确切形状。
- [crates/utils/dependent/provider/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/utils/dependent/provider/src/lib.rs) — `get_metadata_provider(lot, source)` / `get_non_metadata_provider(source)` 的 dispatch，以及 `MediaSource::Custom => err()`。
- [crates/utils/dependent/entity/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/utils/dependent/entity/src/lib.rs) — `commit_metadata`（按三元组 upsert + `is_partial=true` stub）、`ensure_metadata_updated`（指数退避重试）、`update_metadata`（非 partial 直接 return、provider 字段整行覆盖）、`merge_metadata`（搬迁 seen/review/collection_to_entity）、`change_metadata_associations`。
- [crates/services/miscellaneous/background/src/cleanup.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/services/miscellaneous/background/src/cleanup.rs) — `put_entities_in_partial_state`：把用户拥有的实体分块打回 partial，这就是它的"增量水位"。
- [crates/services/miscellaneous/background/src/monitoring.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/services/miscellaneous/background/src/monitoring.rs) — 同步范围 = `collection_name == Monitoring` 的 `collection_entity_membership`，聚合成 entity→users。
- [crates/services/miscellaneous/background/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/services/miscellaneous/background/src/lib.rs) — `perform_background_jobs` 的完整顺序清单 + `invalidate_import_jobs`（超时导入报告置失败）。
- [apps/backend/src/job.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/apps/backend/src/job.rs) — apalis_cron 两个 tick、`MpApplicationJob::UpdateMediaDetails` 对 Person/Metadata/MetadataGroup 的分支、`SingleApplicationJob::ImportFromExternalSource`。
- [crates/utils/dependent/notification/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/utils/dependent/notification/src/lib.rs) — `update_metadata_and_notify_users` / `refresh_collection_to_entity_association`（diff 结果如何回灌到列表排序与通知）。
- [crates/utils/dependent/import/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/utils/dependent/import/src/lib.rs) — `process_import(is_import, ...)` 以 `(MediaSource, identifier, MediaLot)` 为 key 内存去重并聚合 seen/reviews/collections，import 与 export 共用。
- [crates/services/importer/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/services/importer/src/lib.rs) — `perform_import` 的 `match input.source` 全量来源列表与各自的输入形态。
- [crates/models/importer/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/models/importer/src/lib.rs) — `ImportFailStep` 四阶段失败分类 + `ImportFailedItem{identifier, step, lot, error}`。
- [crates/migrations/sql/src/m20230513_create_import_report.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/migrations/sql/src/m20230513_create_import_report.rs) — `import_report`：`was_success NULL`、`progress`、`details JSON`、`source_result JSON`、`estimated_finish_time`。
- [crates/services/importer/goodreads/src/lib.rs](https://raw.githubusercontent.com/IgnisDa/ryot/302931fac46843696f2706f435f08c8d988bf3ab/crates/services/importer/goodreads/src/lib.rs) — ISBN13 → `get_identifier_from_book_isbn` → `(identifier, source)`；换不出 ID 即记失败，证明"外部 ID 优先、无 fuzzy 兜底"。
- [crates/services/miscellaneous/lookup/src/](https://github.com/IgnisDa/ryot/tree/302931fac46843696f2706f435f08c8d988bf3ab/crates/services/miscellaneous/lookup/src) — 存在 `matching.rs` / `patterns.rs` / `tests/episode_extraction.rs` / `tests/best_match.rs`，说明有文件名→S/E 解析与最佳匹配能力（本次仅确认存在，未细读）。

## 项目：jxxghp/MoviePilot

- Stars / 最近提交：11802 stars、1494 forks；`pushed_at = 2026-09-24T06:49:28Z`；默认分支 `v3`，本次研究 pin 在 commit `3428aed4903e514459a757ff8dfdfe0440ce42c7`（v3 是一次完整的 DDD 重构，任务里点名的 `app/core/meta.py`、`app/helper/media.py`、`app/modules/metatube*`、`app/modules/douban/douban.py` 在 v3 树中**均已不存在**，对应职责搬到了 `app/domain/`、`app/chain/`、`app/schemas/`；下列结论全部基于 v3）。
- 技术栈：Python + SQLAlchemy 2.0（`Mapped`/`mapped_column`，同步 `Session` 与异步 `AsyncSession` 双栈）+ Pydantic v2 DTO；迁移机制为 **Alembic**（`database/env.py` + `database/versions/` 共 79 个 revision，强制单 head）；后端按 `DB_TYPE` 在 **SQLite（默认，aiosqlite，WAL/journal 处理）** 与 **PostgreSQL（pg_dump/pg_restore）** 之间切换；缓存为自研 `app/runtime/cache.py`，memory / redis / file 三种后端可插拔、按 region 分区。
- 定位：NAS 媒体库自动化管理（下载站检索 → 订阅 → 识别 → 转移/重命名/刮削 → 媒体服务器同步）。**它没有"媒体条目主表"** —— 库里只有订阅、整理历史、下载历史、媒体服务器条目、站点这几类"账本表"，真正的媒体实体在文件系统 + 远端元数据源里，进程内用一个 `MediaInfo` 对象临时聚合。

### 1. 条目身份与跨库 ID 映射

**唯一规范身份是一个二元组 `(media_source, media_id)`，而不是某个具体站点的 ID 列。**

- 表示层：`app/schemas/media.py` 是身份表示规则的唯一出口。`MediaSource` 枚举覆盖 TMDB / Douban / Bangumi / AniList / IMDb / TVDB / MusicBrainz / TheAudioDB / DoubanMusic；`MEDIA_SOURCE_ALIASES` 把 `themoviedb`、`doubanmusic` 等字符串收敛到枚举；`build_media_key()` 生成对外 API 用的 **带来源前缀的复合键** `tmdb:123` / `douban:30000`，`parse_media_key()` 反向解析。`resolve_media_identity()` 是唯一的"从一个媒体对象里取出主身份"的入口，规则是：来源与 ID 必须**成对**、去空白、拒绝 `"0"`，否则整体返回空身份。
- 持久层：**没有任何一张表存 `douban_id` / `tmdb_id` / `imdb_id` 列**（在 `app/db/models/` 全量 grep 这三个名字，命中数为 0）。所有带媒体身份的表都只有 `media_source: String` + `media_id: String` 两列，并且 `app/db/models/_constraints.py` 给每张表加了同名 DB 级 `CHECK` 约束 `ck_<table>_media_identity`：两列必须同时为空或同时非空、`media_source` 必须是 lower(trim) 后的无空格无冒号串、`media_id` 去空白后非空且不为 `'0'`。约束被抽成 SQL 常量，注释明确说明 **Alembic 脚本必须自带该常量的副本而不能 import**，因为"迁移是历史快照"。
- 写入侧不变量：`app/db/models/_identity.py` 把"成对、非零、去空白"下沉为 SQLAlchemy `Mapper` 级 `before_insert` / `before_update` 事件，监听 `Mapper` 类本身而非某个基类，从而**连仓外插件自建的模型也一并覆盖**。关键取舍写在 docstring 里：持久化侧遇到"半对身份"时**清空两列 + 打一条告警**而不是抛错，因为这六张表都是记账性写入，"丢一条整理历史意味着那个文件可能被重复整理"；而 DTO 侧（`OptionalMediaIdentityMixin` / `RequiredMediaIdentityMixin`）在用户输入边界上**仍然抛错**。同一文件还专门解释了为什么用 `mapper.class_.__name__` 而不是 `local_table.name`。
- 辅助 ID 的地位：`MediaInfo`（`app/domain/context.py:1552` 起）确实挂着 `tmdb_id` / `imdb_id` / `tvdb_id` / `douban_id` / `bangumi_id` / `anilist_id` / `anidb_id`，但字段上方的注释是决定性的：**「数据源返回的辅助 ID，仅作为元数据输出，不参与通用身份传递或持久化」**。也就是说豆瓣 ID 与 TMDB ID 在 MoviePilot 里不是并列的候选主键，而是"某个来源的详情对象"投影出来的附属字段；主键由 `media_source`/`media_id` 承担，`__post_init__` 会先应用各来源投影、然后**再跑一次** `resolve_media_identity()` 收口。
- 多库 ID 不一致时怎么归一（这是本问的核心）：分三层。
  1. **跨源身份转换（按需计算，不落地）**：`app/chain/media/projection.py` 有一张封闭的规则表 `_PROJECTION_RULES: {(source, target): Rule}`，内置四条 —— `Douban→TMDB`、`Bangumi→TMDB`、`TMDB→Douban`、`Bangumi→Douban`；查不到的组合落到 `_ProjectionRule.EVENT`，改发 `ChainEventType.MediaRecognizeConvert` 事件交给官方插件。执行流程固定为三段：`_load_projection_source`（读来源详情）→ `_build_projection_match`（**纯函数**，从来源详情算出目标源所需的 `names/year/media_type/season/imdb_id`）→ `_apply_projection_match`（在目标源里做标题搜索并返回详情）。例如 `TMDB→Douban` 会把 TMDB 的 `external_ids.imdb_id` 一并带给豆瓣匹配器；`Douban→TMDB` 用 `MetaInfo` 拆出原标题/中文名/英文名，按来源优先级 `_unique_names()` 去重后逐个当候选名。数值型 ID 的来源组合还会先校验 `media_id.isdigit()`，非数字直接不进入该规则。**注意：转换的结果不写回任何映射表，只写回本次请求的 MediaInfo。**
  2. **资源→meta 的匹配**：`app/chain/search/result.py::_match_source()` 的判定顺序是「显式外部 ID 优先 → 标题证据 → 消歧」：先 `resolve_media_identity(media=torrent)`，若种子带的就是 `IMDb` 身份且等于 `mediainfo.imdb_id` 则直接命中并返回 `"imdb"`；否则走 `TorrentHelper.match_torrent()`（类型一致 → 站内分类一致 → 年份/季年份集合 → 标题/原标题/别名 `names` 与种子中文名/英文名做 `normalize_upper` 集合求交）；命中后若 `requires_identity_disambiguation()` 判定"这是一条无年份、仅靠别名命中的电视剧种子"，则再进 `_same_work_matched()`。
  3. **证据制消歧（最值得抄的一块）**：`app/application/torrent/download.py::match_same_work_evidence()` 返回 `(bool, str)`，第二条是**人类可读的判定依据**。正向证据依次为：`resolve_media_identity` 完全相等 → 种子年份落在目标 `{year} ∪ season_years.values()` 集合内 → 在 `(imdb_id, tvdb_id, douban_id, bangumi_id)` 四个辅助 ID 上有**任意一个共同值** → 共同首播年份 → 共同原始标题。负向证据（短路拒绝）为：首播年份冲突、`original_title` 冲突、`original_language` 冲突。全部拿不准时返回 `False` 并给出"资源无年份，候选与目标也没有可核验的共同元数据"。辅助 ID 在这里、也仅仅在这里被当作身份证据使用。
- **是否存在显式的 cross-id 缓存/映射表？答案是：DB 里没有，进程内有一份标题键的识别缓存。** `app/modules/themoviedb/cache.py::TmdbCache` 是一个 `TTLCache(region="__tmdb_cache__")` + `FileCache` 双层结构，条目形如 `{id, title, year, type}`，键由 `__get_key(meta)` 从标题解析结果生成；整表用 `pickle` 以 `{"version": PERSISTENCE_VERSION, "items": {...}}` 落盘（region `recognize`、key `tmdb`），启动时 `_restore()` 只接住未过期项，并且**能读旧版本盘位**（`_legacy_file_cache`，`_legacy_cache_found` 置位后立刻 `_dirty = True` 触发一次格式升级写回）。所以它是一个"标题→TMDB 身份"的加速器，不是"豆瓣 ID→TMDB ID"的映射表。推断：这样设计的原因是豆瓣→TMDB 的映射本质上靠标题+年份匹配，带不确定性，缓存不确定结果会污染身份；而标题→识别结果同一问一答，缓存安全。

### 2. 元数据源抽象与字段优先级

- **来源是声明式可插拔能力，而不是 if/else。** 每个元数据源模块目录下有一份 `capability.toml`：`[metadata] type = "mediarecognize"`、`subtype = "TMDB"`、`priority = 1`，`[activation] policy = "bootstrap"`、`watch = ["PROXY_HOST", "TMDB_API_DOMAIN", "TMDB_API_KEY", "TMDB_LOCALE"]`。`watch` 意味着这些配置项一变就重建该能力实例；`app/runtime/capabilities/registry.py` + `app/runtime/extensions/module/` 负责装载与派发。`get_priority()` 的语义在代码注释里写明："数字越小优先级越高，**只有同一接口下**优先级才生效"，实测值 TMDB=1、Douban=2、Bangumi=3、AniList=4。
- **来源按"角色"而不是全局唯一地选择。** `app/runtime/config.py` 里有三个平行设置：`SEARCH_SOURCE`（搜索时允许哪些来源）、`RECOGNIZE_SOURCE`（识别用哪个）、`SCRAP_SOURCE`（刮削用哪个），默认都是 `"themoviedb"`，且都接受同一批来源名的逗号分隔列表。运行时判定收敛在 `app/domain/media.py::is_media_source_enabled()`：请求级 `media_source` 参数 > 全局 `SEARCH_SOURCE` > 都不配则全开。注意 `app/domain/media.py` 这个模块的自我定位注释：只留"按什么规则挑来源"这类**依赖运行期配置的领域策略**，表示规则一律搬到 `app/schemas/media.py`，且"不做 re-export：同一个符号只应有一条 import 路径"。影视/音乐的主来源是 `app/chain/media/facade.py` 上的两个 ClassVar：`_video_primary_source = MediaSource.TMDB`、`_music_primary_source = MediaSource.MusicBrainz`；`_native_recognition_plan()` 把"媒体类型 + 显式来源 + 默认来源"归并成一个冻结的 `_NativeRecognitionPlan`，同步与异步入口共用同一份决策，杜绝两版逻辑漂移。
- **原始 provider payload 不被持久化到库表。** 遍历 `app/db/models/` 里所有 `mapped_column(JSON)`，命中的是订阅的 `note/sites/filter_groups/episode_priority/downloaded_tracks`、整理的 `src_fileitem/dest_fileitem/checkpoint_payload/execution_payload`、媒体服务器的 `seasoninfo`、插件的 `config_data` —— **没有一列存 TMDB/豆瓣原始详情**。原始详情只以 `MediaInfo.tmdb_info / douban_info / bangumi_info / anilist_info` 四个 dict 存在于内存对象里，以及以 pickle 形式存在于 `TmdbCache` 的文件缓存里。推断：这样库表结构与任一 provider 的字段演化解耦，代价是"离线可重放原始响应"这件事做不到，重新识别必须打远端。
- **字段优先级靠"投影（projection）+ 只填空位"实现，而不是靠 merge 时的 diff。** `app/domain/projection/mapping.py::ProjectionBuilder` 是全部来源共享的机制：持有 `current` 字段快照与 `_changes`，提供 `get/set/set_missing/fill_missing/build`，`set` 会 `_copy_value()` 递归深拷以隔离可变来源对象；`fill_missing` 除了"仅当当前为空"外还额外要求 `type(current_value) is type(value)`（或当前为 None）才写入，避免把不同语义的字段串台；并且只填 `name in self._current` 的字段，即**来源无法凭空新增领域字段**。
- 两条投影规则体现了刻意的不对称，这就是实际的字段优先级：
  - `app/domain/projection/douban.py::project()` 几乎全是守卫式写入：`if not builder.get("title")` / `set_missing("title", ...)`、年份缺失时才从 `pubdate[0]` 正则抠 `\d{4}-\d{2}-\d{2}`、再从 `overview` 里正则兜一个 4 位年份、海报按 `pic.large > cover_url（做 URL 改写归一为 w/500/h/750/webp）> cover.url > cover.large.url` 的响应形态优先级选、`_aliases()` 会剥掉豆瓣别名里的 `(豆友译名)`/`(港台译名)` 标记。它同时**无条件**写 `media_source = MediaSource.Douban` 与 `media_id = str(info["id"])`。
  - `app/domain/projection/tmdb.py::project()` 反过来对 `title/original_title/overview/vote_average/genre_ids/adult/directors/actors/names/release_dates/season_info/seasons/season_years` **无条件 `set`**，但对身份两列用的是 `set_missing`，旁边注释是本案最重要的一句：**「辅助 TMDB 详情可能合并到豆瓣/Bangumi 主对象，不能覆盖其稳定身份」**。
  - `MediaInfo.__post_init__` 的应用顺序固定为 `tmdb_info → douban_info → bangumi_info → anilist_info`。推断：综合两条规则的实际效果是"谁先被投影谁定业务字段，但身份列由第一个真正声明它的来源占据、后续来源只能补空"，所以一个 `douban:xxx` 主对象被合入 TMDB 详情后会得到 TMDB 的标题/评分，但仍然是豆瓣身份。多语言/别名方面：`MediaInfo` 有 `title/en_title/hk_title/tw_title/sg_title/original_title/original_name/names` 一整套字段，`names` 是"所有别名和译名"的统一容器，被 `_match_torrent` 当集合参与匹配、被 `_unique_names()` 当跨源匹配候选序列。

### 3. 已有库的导入 / 整理 / 迁移

- **整理（转移）是一条可持久化、可重放的任务队列，不是一次 for 循环。** `app/db/models/transferpending.py`（41KB）就是队列本体：`(storage, src_path)` 定位源文件，`state ∈ {accepted, planned, provider_pending, ...}` 是规划生命周期，`execution_state ∈ {not_started, running, retry_wait, manual_review, ...}` 是执行生命周期，两套状态分开。幂等靠三样东西：
  1. `input_version` + `planning_input`（JSON）+ `input_fingerprint` —— 对"规划输入"做 `json.dumps(..., sort_keys=True, allow_nan=False)` 再 sha256，指纹不变即可复用已算好的计划；
  2. `checkpoint_version` + `checkpoint_payload` + `planned_at` —— 规划结果原子落盘，只有"已取得 checkpoint"的任务才允许进入执行；
  3. `lease_owner/lease_token/lease_expires_at/heartbeat_at` + `attempt_count/retry_generation/retry_due_at` —— 租约式互斥，所有状态跃迁都是**带全条件谓词的单条 UPDATE**（`state.in_(("planned","provider_pending")) AND checkpoint_payload IS NOT NULL AND execution_state IN (...) AND lease_token = :token AND lease_expires_at > :now`），拿 `rowcount` 判断是否抢到了，而不是先读后写。
  - 整理完成后写 `transferhistory`，其上有 **`ux_transferhistory_src_storage` 唯一索引 `(src, src_storage)`**（`app/db/models/transferhistory.py`）。这是"重复整理同一个源文件"在 DB 层的最后一道防线；`app/db/models/_identity.py` 的注释也呼应："丢一条整理历史意味着那个文件可能被重复整理"。
- **网盘与本地被抽象为同一个 `storage` 维度。** `app/modules/filemanager/storages/` 下 `local / alist / alistgo / rclone / smb / alipan / u115` 平级，`transhandler.py` 负责跨存储搬运；`transferhistory` 因此同时有 `src/src_storage/src_fileitem` 与 `dest/dest_storage/dest_fileitem`，`media_source/media_id` 与它们正交。文件命名/目录结构由 Jinja 模板字符串决定（`MOVIE_RENAME_FORMAT` / `TV_RENAME_FORMAT` / `MUSIC_RENAME_FORMAT`，默认值就是 `{{title}} ({{year}})/Season {{season}}/{{title}} - {{season_episode}}{{fileExt}}` 这种模板），`app/chain/transfer/format.py` 负责渲染，并可被插件事件 `ChainEventType.TransferRename` / `TransferRenameBuild` 改写。
- **自动识别失败时的人工确认（manual_review）是一个 durable 状态，不是一个弹窗。** `TransferPending.mark_execution_manual_review()` 把"执行结果未知"的任务隔离出来：**同时清空 `lease_owner/lease_token/lease_expires_at/heartbeat_at`**（释放自动调度，防止后台反复重试一个结果未知的操作），写入 `last_error`，置 `execution_state="manual_review"`；配套的 `resolve_manual_review()` 在 `app/db/models/transferpending.py:675` 与 `transferexecutionstep.py:390`，用 `cls.state == "manual_review"` 做 CAS 谓词，并把 **`manual_review_revision` 自增**（乐观并发：两次确认只有一次生效）。`app/chain/transfer/queue.py` 另有一条收口逻辑："收口未取得租约的队列项，避免 waiting 残影持续阻挡手动重整"。
- **人工直接指定身份走的是另一个入口，且要求提供规范身份。** `app/chain/transfer/history.py:235`：手动整理若未同时给出 `media_source` 与 `media_id` 就直接拒绝（"手动整理需要同时提供 media_source 和 media_id"），而不是猜一个。`app/chain/transfer/execution.py:572` 注明手动整理场景"忽略源目录匹配，使用指定目录匹配"。`app/chain/transfer/request.py:177` 有"应用手动传入的季集覆盖和自定义识别格式"，并特意防止 `_build_path_meta` 已应用过偏移后再偏移一次（注释点名了"集数偏移翻倍"这个 bug）。转移动作本身也可插拔：`app/chain/transfer/plan.py` 的 `__execute_legacy_transfer_providers()` 用 `invoke_provider_sequence()` 让插件按序接管 rename/move，事件包含 `TransferIntercept`、`TransferOverwriteCheck`。
- **订阅（持续追更已入库内容）的复用单位也是规范身份。** `app/db/models/subscribe.py::_identity_condition()` 构造"按统一媒体身份优先"的查询条件，`exists()` / `exists_by_username()` 一律是 `(media_source, media_id) [+ season] [+ episode_group] [+ music_type]`；音乐 recording 类型的条件写成 `or_(music_type == 'recording', music_type.is_(None))`，即**把历史无 `music_type` 的行当作 recording 接纳**，这是为兼容旧数据留的口子。表上有 `ix_subscribe_media_identity` 复合索引和 `ck_subscribe_media_identity` 约束。`search_imdbid` 是个开关（该站是否用 imdb 号搜索），`custom_words` 存自定义识别词（识别词能改写 `MetaBase.org_string`，匹配时会记一条"应用识别词后发生改变"的日志）。
- **数据库迁移工具与幂等性。** 启动走 `app/startup/initializers/database.py::prepare_database()`：①`_validate_migration_lineage()` 要求脚本**必须只有一个 head**、库上 current heads 不得多于一个、且 current 必须落在 `walk_revisions(base='base', head=target)` 的祖先集合里，否则直接 `RuntimeError` 拒绝启动；②若需要迁移且 `DB_BACKUP_ENABLE && DB_BACKUP_ON_UPGRADE`，先 `build_database_governance().create_backup()` 再动结构；③`migrate_before_create` 的分支处理了一个很具体的坑——已标记的旧库必须**先**沿 Alembic 升级再 `create_all()`，"当前元数据可能包含依赖新列的外键，提前 create_all 会在 PostgreSQL 上因父表仍是旧结构而失败"；④`verify_database_revision()` 在 readiness 检查里再确认一次已到 head。单个 revision 也是自守卫的：`database/versions/c2f8a4d6e1b3_3_0_14.py` 先 `sa.inspect(op.get_bind())` 取当前列集合、跳过已存在的列，再用 `sqlalchemy.table(...)` 重新声明表做 backfill，并为老数据合成 `"schema_version": 1` 的最小 `planning_input`（`options: {legacy_replan: True}`），即"宁可强制重规划一次，也不假装旧数据是新的"。
- **未发现 sqlite → postgres 的数据搬迁脚本**（全仓 grep `DB_TYPE` 与 `sqlite`，只有引擎构建、journal mode、`get_id_column()` 的主键方言分叉，以及 `app/adapters/system/backup/database.py` 里 `sqlite3` backup API 与 `pg_dump`/`pg_restore` 两套并列的备份/校验/恢复实现）。推断：v3 的立场是把 SQLite→PostgreSQL 当作"指向新库跑 Alembic + 自行搬数据"的运维动作，仓内只保证同一套模型在两种方言上都能建能迁。本次未检查 `v2` 分支是否曾有过自动搬迁。

### 4. 刷新与增量同步

- **定时刷新分两条正交的链：元数据刷新（`app/chain/subscribe/metadata.py::_check_subscription`）与集数/完成度刷新（`app/chain/subscribe/refresh.py`）。** 前者遍历所有订阅（可按 `mtype` 过滤），每一轮开头建一个 `FreshFactLease()`，同一个订阅本轮只打一次远端识别（`cache=False` 强制新鲜），识别失败只 `logger.warning` 带标题+来源+ID 然后 `return None`，**不会把订阅置为错误态也不会清空已有字段**。
- **刷新写入的是一份显式白名单，而不是"把 MediaInfo 覆盖到行上"。** `update_data` 恰好是：`name, year, vote, poster, backdrop, description, media_source, media_id, total_episode`（音乐订阅再加 `music_type, total_tracks`），然后 `update_data.update(progress_update)`（`lack_episode` 等进度字段）。**用户侧字段完全不在这份名单里** —— `filter/filter_groups/include/exclude/quality/resolution/effect/sites/downloader/save_path/custom_words/season/episode_group/media_category_id` 全部保留。写入统一过 `_SubscribeChain__apply_subscribe_update()` → `sync_subscription_mutation_scope()` → `mutation.update(id, payload, SubscriptionActor(name="chain", is_superuser=True), existing=subscribe, scene="metadata_refresh")`：带 actor、带 `existing` 前像、带 scene 标签，便于审计"是谁、在什么场景下改了哪一列"。
- **用户手填数据被明确保护，且是"只增不减"。** `total_episode` 有三重护栏：`if subscribe.manual_total_episode: return current_total`（用户手填总集数后远端不再改动，见 `refresh.py:424`、`metadata.py:134`）；`__apply_episodes_refresh` 里插件事件回传的新值还要再 `max(current_total, result.total_episode)` 夹一次；确实要下调时走 `__resolve_total_episode_decrease()`，它拿 `replace(subscribe, total_episode=old_total)` 造快照、把已确认下载的集数从候选里减掉，再 `max(candidate_total, max(confirmed))`，防止远端临时少返几集就把用户已入库的集数抹掉。**但要注意一处身份漂移**：`update_data` 里包含 `media_source/media_id`（取自刚识别出的 `mediainfo`），因此远端识别若改判了主身份，订阅行的主键会被刷新——这是"已入库内容跟着远端走"的有意设计，代价是身份不是不可变锚点。
- **已入库变化的传播靠"全量重扫 + 时间戳差集删除"，不是增量事件流。** `app/db/models/mediaserver.py` 每行是一个 `(server, item_id)`（唯一索引 `ux_mediaserveritem_server_item_id`），额外带 `media_source/media_id`（同样受 `ck_mediaserveritem_media_identity` 约束）+ `path` + `seasoninfo` JSON + `lst_mod_date`。同步时给本轮所有被 upsert 的行打上同一个 `sync_time`，然后 `delete_stale(server, sync_time)` 用 `WHERE lst_mod_date IS NULL OR lst_mod_date != :sync_time` 把本轮没再出现的行删掉 —— 一次全量同步的残影自清理，天然幂等且不需要删除事件。另有 `delete_excluded_servers(servers)` 处理"用户把某个 Emby 关掉了"。查"是否已入库"提供两个入口：`exist_by_media_identity(media_source, media_id, mtype)`（走 `ix_mediaserveritem_media_identity_type` 复合索引）和 `exists_by_title(title, mtype, year)`（回退到标题年份，用于还没身份的老数据）。
- **订阅侧的完成对账复用同一份新鲜事实**：`check_and_reconcile()` 就是 `check(reconcile_completion=True)`，刷新完元数据后紧接着 `reconcile_subscription_completion()`，不再重打一次远端；`resolve_subscribe_missing()` 有一条纯只读路径（文档明确写"不更新 lack_episode、不发送事件、不修改数据库"）供预览/计算用。刮削（写 NFO/海报）走批量：`app/chain/transfer/scrape.py` 只在 `transferinfo.need_scrape` 且 `_is_primary_media_file()` 为真时发 `metadata.scrape` 事件，并用 `_register_scrape_batch_task/_close_scrape_batch` 聚合成批，避免每个文件一次刮削。

### 5. 凭据与元数据的边界

凭据与元数据在 MoviePilot 里是**两套完全不相交的表**，边界靠表划分而非字段过滤维持。站点凭据独占 `Site` 表（`app/db/models/site.py`）：`cookie`、`ua`、`apikey`、`token`、`proxy`、`limit_interval/limit_count/limit_seconds` 流控参数都挂在 `domain` 上，媒体记录里**没有任何一列指向 Site 的外键**，唯一索引只在 `(domain)`，凭据轮换不影响任何一条媒体行。站点"统计与用户态"又拆到 `SiteUserData`（按 `domain` 关联，存积分/上传量/做种数/未读消息 JSON），与 `SiteStatistic`、`SiteIcon` 各自一张表。插件侧同理：插件实例的可变配置在 `PluginInstance.config_data`（JSON，按 `instance_id`），插件自己的 KV 数据在 `PluginData(plugin_id, key, value)`（`ix_plugindata_plugin_id_key` 复合索引），插件的**能力注册与安装态**又各自独立成 `PluginIdentity` / `PluginInstallation` 两张表；插件与宿主之间只通过命名事件通信（`media.recognize`、`media.recognize.convert`、`name.recognize`、`transfer.rename`、`transfer.intercept`、`transfer.overwrite.check`、`subscribe.episodes.refresh`、`subscribe.completion.check`），插件永远拿不到"往媒体行里加一列"的能力。全局密钥（`SECRET_KEY`、`RESOURCE_SECRET_KEY`）走 `Settings` 且默认 `secrets.token_urlsafe(32)` 随机生成，不落媒体表。推断：对 AssetMesh 的直接启示是——把"来源账号状态"从 canonical 记录里彻底剥出来、只靠一个字符串 `source` 枚举 + `source_native_id` 相连，是让 canonical 数据可以安全导出/同步/进版本库的最省事做法；MoviePilot 用 DB 级 CHECK 约束（而不是应用层校验）来保证这一对的完整性，这一点尤其值得照搬。

### 关键证据清单

（raw URL 均 pin 在 commit `3428aed4903e514459a757ff8dfdfe0440ce42c7`）

- [`app/schemas/media.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/schemas/media.py) — 身份表示规则唯一出口：来源别名表、`tmdb:123` 前缀键的 build/parse、`resolve_media_identity()` 的成对/非零/去空白语义，以及 DTO 侧抛错、持久化侧降级的双 Mixin。
- [`app/db/models/_identity.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/_identity.py) — 把身份不变量下沉为 `Mapper` 级 `before_insert/before_update` 事件；注释解释了记账性写入为何选"清空+告警"。
- [`app/db/models/_constraints.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/_constraints.py) — 六张表共用的 `ck_<table>_media_identity` CHECK SQL；并说明 Alembic 脚本为何必须自带副本。
- [`app/domain/context.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/domain/context.py) — `MediaInfo`：`media_source/media_id` 为主身份，`douban_id/tmdb_id/imdb_id/...` 上方注释明示"仅作为元数据输出，不参与通用身份传递或持久化"；`__post_init__` 的投影应用顺序与二次身份收口。
- [`app/chain/media/projection.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/media/projection.py) — 唯一的跨库 ID 映射机制：`_PROJECTION_RULES` 封闭规则表 + 未知组合落到 `MediaRecognizeConvert` 插件事件；"读来源详情 → 纯投影算匹配参数 → 目标源匹配"三段式，结果不落地。
- [`app/domain/projection/mapping.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/domain/projection/mapping.py) — `ProjectionBuilder` 的 `set_missing/fill_missing`（含类型一致性守卫、禁止凭空新增字段）。
- [`app/domain/projection/tmdb.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/domain/projection/tmdb.py) — 业务字段无条件覆盖但身份列用 `set_missing`，注释"辅助 TMDB 详情不能覆盖其稳定身份"。
- [`app/domain/projection/douban.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/domain/projection/douban.py) — 豆瓣侧全守卫式写入 + 多响应形态兜底（海报 URL 归一、从简介正则补年份）+ 别名剥离 `(港台豆友译名)`。
- [`app/application/torrent/download.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/application/torrent/download.py) — `match_same_work_evidence()` 的证据制消歧（正向：身份相等/年份集合/共同 imdb·tvdb·douban·bangumi；负向短路：年份·原始标题·原语种冲突），以及 `match_torrent()` 的标题/别名集合匹配。
- [`app/chain/search/result.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/search/result.py) — 资源→meta 的三段匹配顺序与 `_disambiguation_key=(cn_name,en_name,year)` 的候选识别复用缓存。
- [`app/modules/themoviedb/cache.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/modules/themoviedb/cache.py) — 唯一跨进程存活的识别缓存：TTL+LRU+FileCache、版本化 pickle 载荷、旧盘位自动升级；证明"映射不入库、只缓存标题→识别结果"。
- [`app/modules/themoviedb/capability.toml`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/modules/themoviedb/capability.toml) / [`app/modules/douban/capability.toml`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/modules/douban/capability.toml) — 元数据源以声明清单注册：`type=mediarecognize`、`subtype`、`priority`（1 vs 2）、`activation.watch` 绑定到 `TMDB_API_KEY` 等配置项。
- [`app/domain/media.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/domain/media.py) — 来源选择的领域策略：请求级来源 > 全局 `SEARCH_SOURCE` > 全开；模块注释说明表示规则为何迁出、为何不做 re-export。
- [`app/runtime/config.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/runtime/config.py) — `SEARCH_SOURCE / RECOGNIZE_SOURCE / SCRAP_SOURCE` 三角色分开的来源配置；`MOVIE/TV/MUSIC_RENAME_FORMAT` 的 Jinja 模板默认值；`SECRET_KEY` 随机默认。
- [`app/db/models/subscribe.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/subscribe.py) — 订阅行只有 `media_source/media_id` 两列身份；`_identity_condition()` 的复用单位与"旧数据无 music_type 视为 recording"的兼容口子；`manual_total_episode`/`search_imdbid`/`custom_words`/`episode_group` 全在此表。
- [`app/chain/subscribe/metadata.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/subscribe/metadata.py) — 定时刷新的字段白名单（含 `media_source/media_id` 会被刷新，即身份可漂移），以及识别失败仅告警不落错误态。
- [`app/chain/subscribe/completion.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/subscribe/completion.py) — `__apply_subscribe_update()`：所有订阅写入过 `sync_subscription_mutation_scope` 并携带 `SubscriptionActor` + `existing` 前像 + `scene`；下载事实回写时以 `resolve_media_identity` 相等为前置门槛。
- [`app/chain/subscribe/refresh.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/subscribe/refresh.py) — `manual_total_episode` 护栏、`max(current, new)` 只增不减、`__resolve_total_episode_decrease()` 用已确认下载集数防止下调、只读预览路径显式不写库不发事件。
- [`app/db/models/mediaserver.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/mediaserver.py) — `(server,item_id)` 唯一索引 + `lst_mod_date` 本轮时间戳 + `delete_stale()` 差集删除，即"已入库变化"的幂等全量同步模型；`exist_by_media_identity` 与 `exists_by_title` 的双入口。
- [`app/db/models/transferpending.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/transferpending.py) — `input_fingerprint`/`checkpoint_payload` 幂等规划、租约式状态机、`mark_execution_manual_review()` 释放租约并转人工、`resolve_manual_review()` 用 `manual_review_revision` 自增做 CAS。
- [`app/db/models/transferhistory.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/transferhistory.py) — `ux_transferhistory_src_storage (src, src_storage)` 唯一索引与 `ix_transferhistory_media_identity`，配合 `src_storage/dest_storage` 证明网盘与本地共享同一 storage 维度。
- [`app/startup/initializers/database.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/startup/initializers/database.py) — Alembic 单 head 与祖先链校验、迁移前自动备份、`migrate_before_create` 规避 PostgreSQL 外键建表顺序坑、启动后再验一次 revision 已到 head。
- [`database/versions/c2f8a4d6e1b3_3_0_14.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/database/versions/c2f8a4d6e1b3_3_0_14.py) — 一个可参考的自守卫迁移：`sa.inspect` 探测已有列、`sqlalchemy.table` 重声明做 backfill、为旧数据合成 `schema_version:1` + `legacy_replan:True` 强制重规划。
- [`app/adapters/system/backup/database.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/adapters/system/backup/database.py) — SQLite 与 pg_dump/pg_restore 并列的 create/verify/restore 三件套；证明"换后端"在 v3 里是备份-恢复动作而非在线搬迁。
- [`app/db/models/site.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/site.py) / [`siteuserdata.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/siteuserdata.py) / [`plugindata.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/db/models/plugindata.py) — `cookie/apikey/token/ua` 只存在于 Site 表、账号统计另置一张表、插件数据以 `(plugin_id,key)` 命名空间另置一张表；三张表与媒体身份列均无外键关系。
- [`app/chain/transfer/history.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/transfer/history.py) / [`execution.py`](https://raw.githubusercontent.com/jxxghp/MoviePilot/3428aed4903e514459a757ff8dfdfe0440ce42c7/app/chain/transfer/execution.py) — 手动整理入口要求"同时提供 media_source 和 media_id"，且手动整理走指定目标目录匹配而非源目录猜测。

**未能验证 / 明确不存在**

- 不存在任何"豆瓣↔TMDB"映射表或 ID 对照表；跨源身份转换一律按需计算（见 `projection.py`）+ 标题键识别缓存。
- 不存在 provider 原始响应落库（`app/db/models/` 无任何存 `tmdb_info`/`douban_info` 的 JSON 列）。
- 未发现 sqlite → postgres 的数据搬迁脚本；未检查 v2 分支是否曾提供过。
- 未逐行核对 41KB 的 `transferpending.py` 与 82KB 的 `scraping.py` 的全部状态跃迁分支，`state` 枚举的完整取值集合未穷举（仅取代码中实际出现的字面量）。
- 站点 cookies 在 API 响应层是否被脱敏（如前端读取站点列表时是否回显明文 cookie）未验证，仅确认了表结构层面的隔离。
- `MetaTube`、`ffmpegtrans`、`MetaTV`/`NAStool` 在 v3 树中均无对应模块目录（MetaTube 属插件生态，不在主仓 `app/modules/` 内）；因超出回合预算，未做与 NAStool/MetaTV 的对比。

> 说明：任务指定的 `MediaTracker/MediaTracker` 与 `kitsu-core/kitsu` 两个仓库路径在 GitHub 上均为 404（不存在）。经 API 检索后确认真实仓库分别为 **`bonukai/MediaTracker`**（同名项目，org 归属不同）与 **`hummingbird-me/kitsu-server`**（"Rails API server for Kitsu"，即 `hummingbird-me/hummingbird` 拆分后的现役服务端；旧 `hummingbird` 已改名为 `kitsu-tools` 并停更于 2023-12）。本文按这两个真实仓库撰写，未启用备选仓库。

## 项目：bonukai/MediaTracker

- Stars / 最近提交 / 仓库状态：934 stars；`pushed_at` 2025-02-20；HEAD `22aedac26329893b6441d74d12be9a48e190ca9c`（commit 时间 2025-02-04）；`archived: false`；默认分支 `main`；主语言 TypeScript（monorepo：`client/` React + `server/` Express + `docs/`）。近 8 个月无提交，处于低活跃/准停更状态但未归档。
- 数据载体：PostgreSQL + **Knex migration**（`server/src/migrations/*.ts`，时间戳命名，无 ORM；`repository/*.ts` 是手写 Repository 类 + `entity/*.ts` 手写类型 + 显式列白名单数组如 `mediaItemColumns`）。早期存在 TypeORM 痕迹（`metadataProviderCredentials` 表被 `20220222153600_removeMetadataProviderCredentials.ts` 删除）。
- **重要纠偏（前提不成立）**：MediaTracker **没有任何"声明式 YAML integration 文件"**。全仓 YAML 只有 `codecov.yml` / `heroku.yml`（git tree 全量枚举验证）；`server/src/metadata/provider/` 下只有 4 个 **TS class 适配器**（`tmdb.ts`/`igdb.ts`/`openlibrary.ts`/`audible.ts`）。仓库里所谓 "Integrations" 指的是 **Plex/Jellyfin/Kodi 的播放态回传集成**（client 路由页 + `controllers/plexController.ts`），不是元数据源声明。因此 MediaTracker 与 AssetMesh 的 Rust provider adapter **属于同一种设计（代码内适配器），只是抽象层级更低**——这是一个与设计预期相反的结论。

### 1. 实体与用户记录建模

**canonical identity**：`mediaItem` 是**全局共享的单表目录**，主键 `id`（serial），没有任何 `userId`/owner 外键（对 `owner`、`owners` 在全仓 `.ts` 检索为 0 命中）——即同一部剧全站只有一行，多个用户共享，用户态全部挂在别的表上。

**type discriminator + 宽表**：`mediaType` 为字符串判别列，取值来自 `export type MediaType = 'tv' | 'movie' | 'book' | 'video_game' | 'audiobook'`。所有类型的字段都摊平在同一张 `mediaItem` 表的可空列里（`numberOfPages`、`authors`、`narrators`、`developer`、`platform`、`network`、`numberOfSeasons`、`numberOfEpisodes`、`runtime`、`status`…）。**既没有 per-type 表，也没有 JSON/EAV 明细表**。类型专属语义靠"哪些列非空"来表达。

**外部 ID = 直接列，不是映射表**：`ExternalIds` 声明 9 个 provider id 列 `tmdbId / imdbId / tvdbId / tvmazeId / igdbId / openlibraryId / audibleId / traktId / goodreadsId`，全部落在 `mediaItem`（并按层级下沉到 `season`、`episode`）。`source` 列记录"这一行由哪个 provider 产出"（`tmdb`/`igdb`/`openlibrary`/`audible`/`user`/`goodreads`/`trakt`），既是溯源也是权威标记（见第 4 节）。

**seasons / episodes（用户最关心的缺口，此处是完整答案）**：
- `season` 与 `episode` 是**两张独立的一等表**。`season(tvShowId → mediaItem.id, seasonNumber, title, description, numberOfEpisodes, isSpecialSeason, releaseDate, tmdbId, tvdbId, traktId, poster)`；`episode(tvShowId, seasonId → season.id, seasonNumber, episodeNumber, seasonAndEpisodeNumber, title, description, releaseDate, runtime, tmdbId, imdbId, tvdbId, traktId, isSpecialEpisode)`。
- **episode 同时持有 `seasonId`（外键）与 `seasonNumber`（冗余）**，再加一个把两者压成单值的可排序整数 `seasonAndEpisodeNumber`（并建索引），用于跨季列表排序，避免多表 join 排序。
- 唯一约束：`season` 上 `unique(tvShowId, seasonNumber)`；`episode` 上 `unique(tvShowId, seasonNumber, episodeNumber)`（迁移名即 `uniqueSeasonAndEpisodeNumbers`）→ **季/集身份天然幂等，重复写入只会命中同一行**。
-  specials（花絮/OVA）用布尔位 `isSpecialSeason` / `isSpecialEpisode` 从主时间线里摘出去，前端用 `TvSeasonFilters.nonSpecialSeason`、`TvEpisodeFilters.nonSpecialEpisodes` 过滤。
- 读取时 `seasonsWithEpisodes()` 一次性把 episode 按 `seasonId` groupBy 挂到 season 上（`_.groupBy` + 内存装配），不做嵌套 SQL。

**people / credits：不存在。** init 迁移建立的表全集是 `user, accessToken, mediaItem, season, episode, seen, userRating, watchlist, session, metadataProviderCredentials, notificationPlatformsCredentials, notificationsHistory, sessionKey, configuration`（后续增 `list`、`listItem`、`progress`），**没有 person / cast / crew / character 表**。演职人员只是 `mediaItem` 上的扁平文本列（`authors`、`narrators`、`developer`、`network`）。所以 MediaTracker 无法回答"这个导演还拍过什么"，也无法对同一个人做归一。

**lists / collections**：`list(id, userId, name, description, privacy ∈ public|private|friends, sortBy ∈ my-rating|recently-added|recently-watched|recently-aired|next-airing|release-date|runtime|title, sortOrder, allowComments, displayNumbers, isWatchlist, traktId)` + `listItem(id, listId, mediaItemId, seasonId?, episodeId?, addedAt)`。**watchlist 被折叠成一个带 `isWatchlist` 标记的特殊 list**（迁移 `20220427212000_watchlistToList.ts` 把老的 `watchlist` 表搬进 `list`/`listItem`），因此"想看"与"用户自定义清单"共用一套存储；`listItem` 可以指向整部剧、某一季或某一集（三级颗粒）。

**用户自己的记录：不存在统一的 status 枚举行。** 一张 `library_entry` 式的表在这里**没有**，用户态被拆成 4 张互相独立的表：
- `seen(id, userId, mediaItemId, seasonId?, episodeId?, date, duration)`：**看过的事件流**（可重复、可精确到集）。`SeenFilters` 用 `episodeId` 是否为空区分"整片已看"与"单集已看"。
- `userRating(id, userId, mediaItemId, seasonId?, episodeId?, rating, review, date)`：**评分/评论可挂在剧/季/集三个层级上**，`UserRatingFilters` 就靠 `seasonId`/`episodeId` 的空非空来区分三种。
- `progress(id, userId, mediaItemId, episodeId?, date, progress, duration, action ∈ playing|paused, device)`：**续播位置 + 设备 + 播放/暂停动作**，是"在看"的物证而非状态位。
- `list/listItem`（含 watchlist）：**想看**。
于是"某用户对某片的状态"是**由这四张表推导出来的视图**，不是存下来的 enum。`mediaItemRepository.items({userId, onlySeenItems, onlyOnWatchlist, onlyWithUserRating, onlyWithoutUserRating, onlyWithProgress, onlyWithNextAiring, onlyWithNextEpisodesToWatch})` 就是这组推导的入口（`controllers/items.ts` 两个只读路由 `/api/items`、`/api/items/paginated`）。

**"同一 item 跨 provider 归一为一行"**：靠 `findByExternalId(params: ExternalIds, mediaType)` —— 在同 `mediaType` 约束下，对所有已填的 external id 列做 **`OR` 匹配**（`where mediaType = ? AND (tmdbId = ? OR imdbId = ? OR tvdbId = ? OR …)`，`.first()`），命中即视为同一实体，否则新建。批量版 `findByExternalIds()` 接受每列的 id 数组，<100 个 id 时一条 OR 查询、≥100 时按列 chunk(100) 并行事务查询。数据库层用 8 条 `unique(<idCol>, mediaType)` 复合唯一索引兜底（见第 2 节）。**注意其语义是"任一外部 id 相同即同一 item"**，所以一旦 provider 侧 id 打错就会被合并，没有置信度/评分。

### 2. 外部元数据源与 ID 映射

**provider 如何声明（真实形态：抽象类 + 常量注册数组）**：
```ts
export abstract class MetadataProvider<Name extends string = string> {
  public abstract readonly name: Name;
  public abstract readonly mediaType: MediaType;
  public abstract search(query: string): Promise<MediaItemForProvider[]>;
  abstract details(ids: ExternalIds): Promise<MediaItemForProvider>;
}
```
一个 provider **必须且只需提供 4 项**：`name`（字符串，同时写回 `mediaItem.source`）、`mediaType`（它服务哪种类型）、`search(query)`、`details(ids)`。返回类型统一为 `MediaItemForProvider = Omit<MediaItemBase, 'id' | 'lastTimeUpdated'>`（provider 不许自带内部 id/时间戳），season/episode 同理有 `TvSeasonForProvider`/`TvEpisodeForProvider`（剥掉 `id`/`tvShowId`/`seasonId`）→ **provider 只负责"外部世界→统一 DTO"的映射，id 归一由仓库层做**。

**注册与优先级**：`const providers = <const>[new IGDB(), new Audible(), new OpenLibrary(), new TMDbMovie(), new TMDbTv()]`，构造成 `Map<mediaType, Map<name, provider>>`。`get(mediaType, name?)`：**给了 name 就精确取；不给就取该 mediaType 下 Map 的第一个值**（`values().next().value`）。所以 **provider 优先级 = 数组声明顺序 + keyBy 插入顺序**，是隐式的、没有显式 weight/priority 字段。`details()` 分发用 `metadataProviders.get(mediaItem.mediaType, mediaItem.source)`，即**行的 `source` 列决定后续刷新走哪个 provider**。

**原始 provider 响应是否落库：不落。** 全仓没有 raw-response JSON 列；`mediaItem` 上只保留 `externalPosterUrl` / `externalBackdropUrl`（外部图片 URL）+ 本地化后的 `posterId` / `backdropId`，`overview` 是纯文本。`updateMetadata.ts` 里 `downloadNewAssets(old, updated)` 才把外部图片搬到本地存储。可迁移性后果：provider 改字段语义时无法用历史 raw 重放，只能重新拉取。

**别名 / 本地化标题**：只有 `title` + `originalTitle` 两个平面列，加 `language`；本地化不在 per-item 层，而是**实例级全局设定**：`configuration` 表有 `tmdbLang`（约 130 个 locale 枚举）、`audibleLang`（13 国）、`serverLang`、`enableRegistration`、`igdbClientId`、`igdbClientSecret`。`config.ts` 启动时校验 `TMDB_LANG ∈ tmdbLang`。→ 一次安装只服务一种内容语言，**没有多语言标题 map**。

**"映射表"的列与唯一约束（此处映射即列）**：`mediaItem` 上一次性加了 8 个复合唯一索引 `unique(tmdbId, mediaType)`、`unique(imdbId, mediaType)`、`unique(tvmazeId, mediaType)`、`unique(traktId, mediaType)`、`unique(igdbId, mediaType)`、`unique(openlibraryId, mediaType)`、`unique(audibleId, mediaType)`、`unique(goodreadsId, mediaType)`，`season` 上 `unique(tmdbId)`、`episode` 上 `unique(tmdbId)` + `unique(imdbId)`；加索引前先做数据清洗：把空串 `''` 的 `imdbId`/`openlibraryId`/`audibleId` 归一为 `NULL`（否则唯一索引会把多个 `''` 判为冲突）。这是很实用的反面教训：**external id 列必须禁空串**。

**coalesce 顺序（外部源解析时的 provider 择优）** 见 `metadata/findByExternalId.ts`：`findMediaItemByExternalId` 先查本地（`mediaItemRepository.findByExternalId` 的 OR 匹配），miss 才 `findMediaItemByExternalIdInExternalSources`；后者的分支顺序是 —— tv：`TMDbTv.findByTmdbId → findByImdbId → findByTvdbId`；movie：`TMDbMovie.findByTmdbId → findByImdbId`；audiobook：`Audible.findByAudibleId`；book：`OpenLibrary.details({openlibraryId})`；每步 `try/catch {}` 静默吞异常、失败仅 `logger.error` 后继续下一步。**注意 `video_game` 完全没有 external-id 解析分支**（IGDB 只有 `search`/`details`，未接入 `findByExternalId`），也无 traktId/goodreadsId 分支 → 推断：游戏与通过 trakt/goodreads 外部 id 进来的条目必须先手动 search 建库才能被解析。集级解析 `findEpisodeByExternalId` 支持 imdb/tmdb/tvdb 三种入参，本地 `episode` 表 miss 时走 `TMDbTv.findByEpisodeImdbId/TvdbId` 拿到 `tvShowTmdbId + seasonNumber + episodeNumber`，再递归回整剧解析路径。

### 3. 导入 / 迁移已有数据

三条真实入口路径：

**(a) Trakt 全量导入（外部 tracker → 本站）—— external-id-first + 批量解析 + 显式 unmatched 集合**（`controllers/import/traktTv.ts`，1226 行）。核心是 `getMediaItemsByTmdbIds(ids, mediaType)`：一次性把 `{trakt, tvdb, imdb, tmdb, tvrage}` 五元组列表喂给 `findByExternalIds()`（OR 批量匹配），返回 `{existingItems, missingItems}`，其中 `missingItems = ids.filter(item => !existingItemsMapByTmdbId[item.tmdb]).uniqBy(item => item.trakt)`。随后对每个 missing 项调 `findMediaItemByExternalIdInExternalSources()`（即上面的 tmdb→imdb→tvdb coalesce，命中即建 `mediaItem`），已有且需要刷新的 TV 剧走 `updateMediaItem()`；最后 `_.keyBy([...existing, ...foundMovies, ...updatedTvShows, ...foundTvShows], 'tmdbId')` 得到统一映射表再写用户态。**未匹配项被静默 `catch(e){}` 丢弃并计入进度条**（`onProgress(currentItem/total)`），没有 dry-run、没有人工复核队列。

**(b) Goodreads RSS 导入（CSV/RSS → 本站）**（`controllers/import/goodreads.ts`）：`POST /api/import-goodreads {url}`，拉 RSS → `fast-xml-parser` → 直接映射成 `MediaItemBase[]`（`source:'goodreads'`、`goodreadsId: item.book_id`、`authors`、`numberOfPages`、`externalPosterUrl`…）→ **`mediaItemRepository.mergeSearchResultWithExistingItems(items,'book')`**：先按"external id 至少有一个非空"过滤（一个都没有的直接丢弃），再在同一事务内做 OR 匹配 existing → 命中就返回旧行、否则 insert（`returning '*'`，并预生成 `posterId`/`backdropId`）。这是**幂等 upsert 的最小实现样板**。之后按 Goodreads 语义分派用户态：`user_shelves === 'to-read'` → `listItemRepository.addItem({watchlist:true})`；`'currently-reading'` → `progress` 行（`progress: 0`）；`''`（read）→ `seen` 行；其他 shelf → 以 `Goodreads-<shelf>` 命名建 `list` 并逐条 addItem（**同名的 Goodreads 清单会被复用**：`findOne({name}) || create({name})`）。幂等性靠**仓储层的 `createManyUnique(rows, uniqueByFn)`**：`seenUniqueBy = {mediaItemId, date}`、`progressUniqueBy = {mediaItemId}`（一本书只留一条进度）；`userRatingRepository.createMany(rating)` **不去重**（重跑会重复插评分，可推断为已知弱点）。返回 `{read, toRead, currentlyReading, ratings}` 计数摘要。

**(c) Plex / Jellyfin / Kodi webhook 导入"已看"**（`controllers/plexController.ts`）：`POST` 解析 payload 得到 `{imdbId, tmdbId, tvdbId, duration}`（`externalIds.tmdb` 等），交给 `findMediaItemOrEpisodeByExternalId({mediaType:'tv', id:{...}, seasonNumber, episodeNumber})` 解析成 mediaItem+episode，再 `seenRepository.create({..., duration})`。**TV 场景下"集"是靠 S/E 号 + 剧集外部 id 双条件定位的**，这正对应用户"从本地文件/播放器反向入库"的需求。若目标 mediaItem 只是占位（`needsDetails` 为真），解析路径会先就地 `updateMediaItem(mediaItem)` 补齐 details 再找 episode —— 即**"惰性补全"内嵌在匹配路径里**。

**人工复核 / "no match" 队列：不存在。** MediaTracker 的"人工"体现在另一处：搜索页 `controllers/search.ts` 用 `metadataProviders.get(mediaType)` 的 `search(q)` 返回候选，由**用户当场挑一条**入库（`source` 记为该 provider；挑不到时可建 `source:'user'` 的手建条目）。**`source:'user'` 与 `needsDetails` 是两个关键开关**：手建条目不参与自动刷新（见第 4 节），`needsDetails` 的条目在解析时补全且**不发通知**（`if (!oldMediaItem.needsDetails) await sendNotifications(...)`）。

**从文件扫描媒体库（Radarr 式 existing-library import）：不存在**（无文件名解析、无 `video辨识` 逻辑），只有 webhook 推送。

### 4. 增量同步

**调度**：`server.ts` 在 production 下 `await updateMetadata()` 后 `setInterval(async () => { await catchAndLogError(updateMetadata); await catchAndLogError(sendNotifications) }, durationToMilliseconds({hours: 1}))` —— **每小时一次、全库扫描**，无 cron、无分片、无 per-item next-check 时间列。`updateMetadata` 与 `sendNotifications` 都用 `createLock(...)` 包裹（`server/src/lock.ts`）做进程内/DB 互斥。

**水位与选择（watermark = `lastTimeUpdated`）**：候选集由 `mediaItemRepository.itemsToPossiblyUpdate()` 决定 —— `mediaItem` LEFT JOIN `seen`/`listItem`/`userRating`，要求**至少有一条用户态记录**（`whereNotNull(seen.id).orWhereNotNull(listItem.id).orWhereNotNull(userRating.id)`），并 `whereNot('source','user').whereNot('source','goodreads')`。→ **只有"有人在用"的行才刷新，且纯手建条目和 Goodreads 导入的书籍永不自动刷新**（authoritativeness：用户输入 > 外部源）。节奏由 `shouldUpdate(mediaItem)`：`timePassed = now - lastTimeUpdated`；若**非 tv 且 releaseDate 已过且早于上次更新时间**（即作品已完结、信息稳定）→ 阈值 **30 天**；否则 **24 小时**。`lastTimeUpdated`（BigInteger epoch ms，`notNullable`）是本项目唯一的水位概念，**没有 per-field 时间戳、没有 ETag/hash 对比**。

**字段级 diff 与"谁权威"**：非 tv 走 `{...newMediaItem, lastTimeUpdated: now, id: old.id}` —— **外部源整体覆盖本地字段**（provider 权威）。TV 走 `margeTvShow(old, new)`（原文如此拼写）：先 `seasonsWithEpisodes(old)`，然后
```ts
const merge = (oldMediaItem, newMediaItem) => ({
  ...newMediaItem, id: oldMediaItem.id, lastTimeUpdated: Date.now(),
  seasons: newMediaItem.seasons.map(season => ({
    ...season, id: seasonsMap[season.seasonNumber]?.id, tvShowId: mediaItemId,
    episodes: season.episodes.map(ep => ({ ...ep,
      id: episodesMap[ep.episodeNumber]?.id, tvShowId, seasonId })),
  })),
})
```
→ **重识别完全靠自然键 `seasonNumber` / `episodeNumber`（而非外部 id）**，把新 payload 贴上旧主键，从而保留 `seen`/`userRating`/`listItem` 的引用完整性。**所有元数据字段由 provider 覆盖，本地不保留用户编辑**（唯一的保护是 `source`/`needsDetails` 的候选集过滤与 `lockedAt`）。

**删除 / 墓碑：硬删除 + 引用完整性护栏，无 tombstone。** `getItemsToDelete(old, updated)` 用 `_.difference` 按 `seasonNumber` / `episodeNumber` 求"本地有、外部源没有"的季/集，然后在一个事务里：**先检查这些 episodeId 是否被 `seen` / `progress` / `listItem` / `userRating` 引用，被引用则 `throw` 中止整次删除**（日志原文 `failed to delete local episodes, there are seen entries with those episodes`）；检查通过才级联 delete（顺带清 `notificationsHistory`）并删 `episode`/`season` 行。删除失败时的降级路径是**直接返回 `{...newMediaItem, seasons: old.seasons}`，即保留旧季/集结构只更新头部** —— 这是很值得抄的一手："外部源结构变更若与用户记录冲突，宁可留下过期的结构也不破坏用户数据"。

**并发与幂等护栏**：`mediaItemRepository.lock(id)` 是 `UPDATE mediaItem SET lockedAt = now WHERE id = ? AND lockedAt IS NULL`（CAS 式占用），配合 `unlock()` 与 `unlockLockedMediaItems()`（把 `lockedAt <= now-1d` 的僵尸锁强制释放），`lockedAt` 不出现在任何对外响应类型里（`Omit<..., 'lockedAt'>`）。通知去重靠 `notificationsHistory` 表按 `(mediaItemId|episodeId, …)` 记账。

### 关键证据清单

- [server/src/entity/mediaItem.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/mediaItem.ts) — `MediaType` 五值判别、9 个 external id 列、宽表字段与 `source`/`needsDetails`/`lockedAt`，以及 `mediaItemColumns` 白名单。
- [server/src/migrations/20210818142342_init.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/migrations/20210818142342_init.ts) — 全部建表语句：mediaItem 无 userId、season/episode 独立表、seen/userRating/watchlist 的用户态拆分、无 person/cast 表。
- [server/src/migrations/20230205000000_uniqueExternalIds.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/migrations/20230205000000_uniqueExternalIds.ts) — 8 个 `unique(idCol, mediaType)` 复合唯一索引 + 建索引前的空串→NULL 清洗。
- [server/src/migrations/20220406165800_uniqueSeasonAndEpisodeNumbers.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/migrations/20220406165800_uniqueSeasonAndEpisodeNumbers.ts) — 季/集自然键唯一约束（幂等重灌的基础）。
- [server/src/entity/tvseason.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/tvseason.ts) · [entity/tvepisode.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/tvepisode.ts) — 季/集各自带外部 id、`isSpecial*`、episode 冗余 `seasonNumber` + `seasonAndEpisodeNumber`。
- [server/src/entity/list.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/list.ts) · [entity/seen.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/seen.ts) · [entity/progress.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/progress.ts) · [entity/userRating.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/userRating.ts) — `isWatchlist` 折叠清单、评分/已看/进度三级颗粒（mediaItem|season|episode）拆分。
- [server/src/metadata/metadataProvider.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/metadata/metadataProvider.ts) — provider 抽象类的 4 个必须成员（**证明是 TS class，不是 YAML**）。
- [server/src/metadata/metadataProviders.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/metadata/metadataProviders.ts) — 注册数组顺序即优先级、`get(mediaType, name?)` 的隐式 coalesce。
- [server/src/metadata/findByExternalId.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/metadata/findByExternalId.ts) — 本地 OR 匹配→外部源 tmdb/imdb/tvdb coalesce→miss 即 create；video_game 无分支。
- [server/src/repository/mediaItem.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/repository/mediaItem.ts) — `findByExternalId(s)` 的 OR 归一、`mergeSearchResultWithExistingItems` 的 upsert、`itemsToPossiblyUpdate` 的"仅刷新被使用且非 user/goodreads"选择、`lock/unlock` CAS。
- [server/src/updateMetadata.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/updateMetadata.ts) — `shouldUpdate` 24h/30d TTL、TV `merge()` 用 seasonNumber/episodeNumber 重贴主键、`getItemsToDelete` 的用户记录引用护栏与降级。
- [server/src/controllers/import/goodreads.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/controllers/import/goodreads.ts) — Goodreads RSS 到 seen/progress/watchlist/自定义 list 的分派与 `createManyUnique` 幂等。
- [server/src/controllers/import/traktTv.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/controllers/import/traktTv.ts) — external-id 批量匹配 + `missingItems` 逐个走外部源 + 进度上报、无复核队列。
- [server/src/controllers/plexController.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/controllers/plexController.ts) — 由播放器 webhook 用 `{imdbId|tmdbId|tvdbId}+S/E` 定位集并写 `seen`。
- [server/src/server.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/server.ts) — 每小时 `setInterval(updateMetadata)` 的全库轮询与 `createLock` 互斥。
- [server/src/entity/configuration.ts](https://raw.githubusercontent.com/bonukai/MediaTracker/22aedac26329893b6441d74d12be9a48e190ca9c/server/src/entity/configuration.ts) — provider 凭证与语言是实例级单行配置（`tmdbLang`/`audibleLang`/`igdbClientId`），非 per-item 本地化。

## 项目：hummingbird-me/kitsu-server（替代不存在的 `kitsu-core/kitsu`）

- Stars / 最近提交 / 仓库状态：189 stars；`pushed_at` 2026-06-20；HEAD `e6575ed9fd73ba8cccb920fe2e3f1ef873a71333`；`archived: false`；默认分支 **`the-future`**；Ruby。组织内相关仓库：`kitsu-tools`（旧 monorepo `hummingbird` 改名，2023-12 停更，2144★）、`kitsu-web`（客户端）、`kitsu-mobile`、`api-docs`。Kitsu 是长活的动漫/社交目录 + 追踪器。
- 数据载体：PostgreSQL + **Rails ActiveRecord + `db/migrate` + `db/schema.rb`**（读 schema 一次性拿到全部列/索引/唯一约束，成本极低）。重度使用 PG 特性：`hstore`（多语言标题、rating 分布）、`jsonb`（描述、图片元数据）、`citext`（slug 大小写无关唯一）、数组列、partial index、expression index（`index_anime_on_wilson_ci`）。检索侧同时维护 **Chewy/Elasticsearch `MediaIndex` 与 Algolia `AlgoliaMediaIndex`**（模型里 `update_index` + `update_algolia`），异步任务用 **Sidekiq**（`queue: 'now'|'soon'|'eventually'`、`debounce: true`），文件用 Shrine，审计用 paper_trail（`versions` 表）。

### 1. 实体与用户记录建模

**canonical identity：没有统一 `media` 表**。schema 里存在的是**per-type 表** `anime`、`manga`、`dramas`，各自主键 `id serial` + `slug citext UNIQUE`。统一发生在**应用层**：三个模型都 `include Media`（`app/models/concerns/media.rb`，一个 ActiveSupport::Concern，聚合 `Titleable / Rateable / Rankable / Trendable / Sluggable / Mappable / DescriptionSanitation` + 图片 uploader），**数据库层则靠"多态二元组" `(media_type, media_id)` 把它们缝合**——`episodes`、`castings`、`media_staff`、`media_characters`、`media_productions`、`streaming_links`、`installments`、`favorites`、`reviews`、`quotes`、`media_ignores`、`mappings`、`media_relationships` 全部是 `t.string "media_type", null: false` + `t.integer "media_id", null: false` 并配 `index [media_type, media_id]`。校验用 `validates :media, polymorphism: { type: Media }`，**以"是否 include 了 Media concern"作为类型白名单**。这与 MediaTracker 的"一张宽表 + mediaType 字符串"是两种极端：Kitsu 每类型可独立加字段/加索引，代价是所有跨类型查询必须走多态列。

**状态字段是算出来的**：`Media#status` 由 `start_date`/`end_date` 推导为 `:tba / :unreleased / :upcoming(3 个月内) / :current / :finished`，并配套同名 scope；库里另有 `tba` 字符串列与 `subtype`（TV/movie/OVA/…枚举）、`episode_count`、`episode_count_guess`、`release_schedule`（`serialize :release_schedule, IceCube::Schedule` —— 把 RRULE 式排期序列化进 text 列）。

**seasons / episodes**：**没有 season 表**。`episodes(id, media_id, media_type, number, season_number, relative_number, airdate, length, filler, status, titles hstore, canonical_title, description jsonb, thumbnail_data jsonb)`，`season_number` 只是一个整型列（`validates :season_number, presence: true`，`before_validation` 里 `self.season_number ||= 1`），"季"是对 episodes 做 group-by 的结果。`Episode` 用 `scope :for_progress, ->(p) { reorder(:season_number, :number).limit(p) }` 与 `for_range` 来把"progress=N"翻译成"看过哪些集"。`status` 枚举里有 **`validated`**：`Episodic` concern 提供 `has_many :validated_episodes, -> { where(status: :validated) }`，即**机器抓取来的集默认未验证，人工/系统确认后才进入用户可见集合**。`MediaTotalLengthCallbacks.hook(self)` 与 `recalculate_episode_length!`（取集长的**众数**，众数占比不足一半则改用均值）负责统计回填。
manga 侧则是**另一套层级形状**：`volumes(id, manga_id, number, chapters_count, isbn[], published_on, titles jsonb)` 与 `chapters(id, manga_id, volume_id, volume_number, number, length, published, filler, status)`，即"卷-话"用真实 FK（`volume_id`）+ 冗余 `volume_number` 双写；进度上限对 manga 走 `volumes_owned`。→ 结论：**两套层级不要试图统一成一棵树**，Kitsu 用 `Episodic`（episodes）与 manga 自己的 volume/chapter 两套 concern，靠 `LibraryEntry#unit` → `media.unit(progress)` 这一层薄多态来统一"第 N 个单位"的概念。

**people / credits**：`people(id, name, names jsonb, canonical_name, other_names[], birthday, slug citext UNIQUE, description jsonb, image_data jsonb)` + `characters`（同样多语言 name）。信用挂在两张多态表上：`castings(media_id, media_type, person_id, character_id, role, voice_actor bool, featured bool, order int, language)` —— 校验"person 与 character 至少有一个"（`validates :person, presence: true, unless: :character`），`order` 保序、`voice_actor`+`language` 区分配音版本；以及 `media_staff(media_id, media_type, person_id, role)` 管幕后。此外仍有**per-type 旧表** `anime_castings`、`anime_staff`、`anime_characters`、`manga_characters`、`manga_staff`、`drama_castings`、`drama_staff`、`dramas_media_attributes` —— 说明它正处在"per-type 关联 → 多态关联"的迁移中途，两套并存。
另有 `media_attributes(title, high_title, neutral_title, low_title, slug)` 这一**受控词表 + 四档大小写显示名**，配 `anime_media_attributes`/`manga_media_attributes`/`dramas_media_attributes` 关联表，以及 `media_attribute_votes`（用户对属性正确性投票）。

**lists / collections（分三层）**：① 目录级 `franchises`（多语言 titles hstore）+ `installments(media_id, media_type, franchise_id, release_order, tag, alternative_order)` —— **集合成员带两种排序（发行序 / 推荐观看序）**；② `media_relationships(source_id, source_type, destination_id, destination_type, role)` —— **带类型边的同源关系**（续作/前传/alterative setting…，role 为整型枚举）；③ 目录分类 `categories`（含 `parent`、`total_media_count`）+ 多态 `media_categories`、`genres` HABTM、`favorite_genres_users`、`favorites(item_type,item_id,user_id)`；④ 用户侧自定义清单**不在服务端 schema**（推断：由 `Post`/`pinned` 等社交对象承担），Kitsu 的"用户集合"实际是 `library_entries` 上的 status 分桶 + `media_ignores`（"不想看"，`(user_id, media_type, media_id)`）+ `not_interesteds`。

**用户记录：一张 `library_entries`，这是本项目最值得借鉴的部分**。列：`user_id NOT NULL`、`media_id NOT NULL` + `media_type NOT NULL`、**`status NOT NULL` 枚举 `{current:1, planned:2, completed:3, on_hold:4, dropped:5}`**（正好对应用户的 想看=planned / 在看=current / 看过=completed，另含 on_hold/dropped）、`progress`（默认 0，NOT NULL）、`rating`（整型 **2..20**，`VALID_RATINGS = (2..20)`，即 10 分制×2 的定点存储，避免浮点）、`notes`、`private`、`nsfw`、`volumes_owned`、`reconsume_count`（重看次数，上限 50，报错文案 `'just... go outside'`）、`reconsuming`、`time_spent`、`started_at`、`finished_at`、`progressed_at`、`media_reaction_id`、`reaction_skipped`。约束：`unique index (user_id, media_type, media_id)` —— **一个用户一部作品恰好一行**；评论是**1:1 挂在 library entry 上**（`has_one :review`），不是独立内容对象。
值得注意的**遗留双写**：该表同时有 `anime_id`、`manga_id`、`drama_id` 三个可空 FK（并各带 partial index `where: (anime_id IS NOT NULL)`，注释 `Prevent pkey scans on small limits`）与多态 `(media_type, media_id)`；`before_validation` 里明确写着 `# TEMPORARY: If media is set, copy it to kind_id, otherwise if kind_id is set, copy it to media!`，并用 `validate :one_media_present` 强制"两者只能恰好一个"。→ 这是**从 per-type FK 迁移到多态键的真实过渡做法：加新列 + 双写 + 一致性校验 + 保留旧列做索引优化**。
自动状态机（`before_save`）：`progress == media.progress_limit` 且 status 未显式改 → 自动置 `completed`；status 变 completed → `progress` 顶到上限、`finished_at ||= now`；`current` 时 `started_at ||= now`；`progressed_at` 在 status/progress 变化时刷新；`progress` 变化而 status 未变 → **自动改成 `current`**。全部这段都被 `unless imported` 包住 —— 见第 3 节。
派生统计：`recalculate_time_spent!` 用 `media.episodes.for_progress(progress).sum(:length) + media.total_length * reconsume_count`；`after_save` 里维护 `media.rating_frequencies` hstore 计数、`media.update_unit_count_guess(progress+1)`（**用用户进度众数反推总集数猜测，写入 `episode_count_guess`**）；`LibraryEntryDiff` 把一次保存拆成 `progress_diff` / `reconsume_diff` / `time_diff` / `became_completed?` / `became_uncompleted?` 供 feed 与 stats 使用。

**"同一 item 跨 provider 归一为一行"**：靠 `mappings` 表 + `Mapping.lookup`（见下节），而不是靠 OR 匹配列。

### 2. 外部元数据源与 ID 映射

**外部身份映射表（本项目的核心概念）**：
```ruby
create_table "mappings" do |t|
  t.string "external_site", null: false
  t.string "external_id",   null: false
  t.integer "item_id",      null: false
  t.string "item_type",     null: false
  t.string "issue"                       # 人工标注该映射存在问题的说明
  t.index %i[external_site external_id item_type item_id], unique: true
  t.index %i[item_type item_id]
end
```
模型侧：`validates :item_id, uniqueness: { scope: %i[item_type external_site] }`，注释原文 `Right now, we want to ensure only one external id per item per site`；`Mapping.lookup(site, id)` 支持 **find-or-create 块**（`Mapping.lookup(site, id) { yield }` 找不到就建）。`Mappable` concern 提供 `has_many :mappings, as: :item, dependent: :destroy` + `accepts_nested_attributes_for :mappings`（GraphQL/JSONAPI 写入时可直接嵌套建映射）；`Media::class_methods#by_mapping(site, id)` 用手写 `LEFT OUTER JOIN mappings m ON m.item_type = '<Type>' AND m.item_id = <type>.id` 做 join 查询（**注意：该 SQL 里 `anime.id` 是硬编码的，属潜在 bug**），`find_by_mapping` / `mapping_for(site)` 是其包装。
`site` 的取值是一份**开放字符串词表**（DB 无 FK/枚举，GraphQL 层有枚举）：`myanimelist/anime|anime|character|people|producer`、`anilist/anime|manga`、`thetvdb`、`thetvdb/series`、**`thetvdb/season`（季也有独立外部身份）**、`imdb/episodes`（集同理）、`anidb`、`animenewsnetwork`、`mangaupdates`、`hulu`、`aozora`、`trakt`、`mydramalist`。→ **映射粒度可以细到季/集，且 site 名带命名空间（`provider/granularity`）**，与 MediaTracker"每类型一列"的写法正相反：Kitsu 加一个新源不需要 DDL，但失去数据库级唯一性/类型检查。

**provider 如何声明：没有 provider 插件接口。** 元数据源被拆成三种不同机制，**没有一处统一的"源"抽象**：① `app/graphql/types/enum/mapping_external_site.rb` 之类的常量词表；② `app/services/the_tvdb_service.rb` + `TheTvdbDaily/WeeklyWorker`（唯一成体系的 push 型外部源）；③ `app/services/list_sync/*`（拉/推 MAL）与 `app/models/list_import/*`（导入）；④ `app/embedders/`、`app/services/oembed_embedder/providers_list.rb`（URL 展开）。**catalog 元数据主要由内部 wiki 流程人工维护**（`wiki_submissions` + `wiki_submission_logs` + `media_attribute_votes`），而非 provider adapter 拉取。

**原始响应是否持久化：部分**。`scrapes(id, target_url NOT NULL, scraper_name, depth, max_depth, parent_id, original_ancestor_id, status)` 是一张**爬取任务/血缘表**（记录 URL、抓取器名、深度与父子链），而不是 raw JSON blob；`list_imports.input_file_data jsonb` 保存上传文件的解析产物；`videos.embed_data jsonb` 保存第三方播放器返回的 embed 元数据。→ 推断：Kitsu 没有"整包 raw response 存档"，重放能力靠 `scrapes` 的 URL 血缘而非快照。

**别名 / 本地化标题（这块是 Kitsu 最强的设计）**：每张 media 表（含 `episodes`、`chapters`、`volumes`、`franchises`、`people`）都有
```
t.hstore "titles", default: {}, null: false      # locale -> 标题（en, en_jp, ja_jp, ru, …）
t.string "canonical_title", default: "en_jp"     # 指向 titles 的哪个 key 作为规范名
t.string "abbreviated_titles", array: true       # 常用简称/缩写别名
t.string "original_title"                        # titles 中"原名"的 locale key（不是值！）
t.string "romanized_title"                       # titles 中"罗马音"的 locale key
```
`Titleable` concern 把 `canonical_title` / `original_title` / `romanized_title` 重定义为**间接寻址**（`def canonical_title; titles[self[:canonical_title]]; end`），并校验：有 titles 就必须有 canonical_title、若声明了 `original_title_key` 则该 key 必须存在于 titles、**且 `titles` 里必须至少有一个 `en*` 键**（`has_english_title`）。`titles_list` 组装出 `TitlesList` 值对象（canonical/original/romanized/alternatives）供搜索与显示。→ 对 AssetMesh 的直接启示：**把"哪个 locale 是规范名"做成一个列（指针），把名字全部塞进一个 locale-keyed map**，比多加 `title_cn` / `title_en` 列或再建 alias 表都轻。people 同构：`names jsonb` + `canonical_name` + `other_names[]`。
描述与图片也用同一模式：`description jsonb`（locale-keyed）、`poster_image_data` / `cover_image_data` jsonb（Shrine 附件元数据内联，避免单独 attachments 表）。

**优先级 / coalesce**：**Kitsu 没有 provider 优先级机制**（不存在"多源择优"）。跨源择优只发生在**导入时的解析顺序**（第 3 节）与 `Media#status` 之类派生规则里。

### 3. 导入 / 迁移已有数据

**统一抽象：`ListImport`（STI）+ 每源一个 `Row` 解析器**。表 `list_imports(id, type NOT NULL, user_id NOT NULL, strategy NOT NULL, input_file_* (Shrine 四列), input_text, status default 0, progress, total, error_message, error_trace, input_file_data jsonb)`。`type` 必须是 `ListImport::` 命名空间下的子类（`validate :type_is_subclass` 用 `type.safe_constantize <= ListImport` 判断），子类有：`ListImport::Anilist`、`MyAnimeList`、`MyAnimeListXml`、`AnimePlanet`、`Aozora`、`TaigaXml`。`strategy` 枚举 = **字段级冲突策略**：`{greater: 0, obliterate: 1}`。
```ruby
def merged_entry(entry, data)
  case strategy.to_sym
  when :greater   # 比较 [completions, progress] 元组，谁大用谁
    theirs = [data[:completions] || 0, data[:progress] || 0]
    ours   = [entry.reconsume_count || 0, entry.progress || 0]
    entry.assign_attributes(data) unless (theirs <=> ours).negative?
  when :obliterate then entry.assign_attributes(data)
  end
  entry.progress = [entry.progress, progress_limit].min   # 强制夹到作品长度
  entry
end
```
**这就是"重跑导入不能覆盖用户更大进度"的正式解法**：不是 LWW，而是**按业务意义的偏序取 max**。

**执行/幂等**：`each` 由子类实现并 `yield row.media, row.data`；`apply` 逐行 `LibraryEntry.where(user_id:, media:).first_or_initialize` → `le.imported = true` → `merged_entry` → `le.save! unless le.status.nil?`。整段包在 `Retriable.retriable on: [ActiveRecord::RecordNotUnique]`（**并发/重跑下唯一索引冲突即重试，等价于 upsert**）与 `Chewy.strategy(:atomic)`（批量期间延迟索引写入）；单行异常被 `rescue StandardError` 捕获、上报 Sentry 并把本次 import 标为 `partially_failed`（带 `error_message` / `error_trace` 落库），**继续跑完剩余行**。`apply!(frequency: 20)` 每 20 行才把 `status/progress/total` 写回 DB（进度节流），跑完重算 6 个 per-user `Stat::` 聚合。触发方式：`after_commit(on: :create) { apply_async! }` → `ListImportWorker`（`queue: 'now'`）；`retry_async!` 把状态重置为 `queued` 再入队（**重试入口 = 显式状态机 `queued/running/failed/completed/partially_failed`**）。`ListImportWorker` 里 `return if import == ListImport::MyAnimeList` —— 显式禁用一种源（推断：因 MAL 改为走 `ListSync` 授权流）。
`imported` 是 `attr_accessor`（**不入库的运行时旗标**），作用正是关掉 `LibraryEntry#before_save` 的自动状态机与 feed/统计副作用 —— "导入模式"与"用户手改模式"共用一套模型、用旗标切换行为。

**匹配策略：external-id 三级 fallback（这是本项目回答用户"如何对上已有数据"的核心）**，`ListImport::Anilist::Row#media`：
1. `Mapping.lookup("anilist/<type>", node['media']['id'])` —— **本源自带 id 优先**；
2. miss 则 `Mapping.lookup("myanimelist/<type>", media_data['idMal'])` —— **借助源间交叉 id 再试一次**；并且一旦命中，**立刻回写一条 `anilist/<type>` 映射**（`Mapping.create(item: mal_mapping, external_site: anilist_key, external_id: media_data['id'])`）——**"从交叉引用自举映射"**，下次同一行不再需要二跳；
3. 再 miss 才 `Mapping.guess(type, media_info)`，其中 `media_info = {title:, subtype:, episode_count:, chapter_count:}.compact`，`title` 自身按 `romaji → english → native → userPreferred` 顺序 coalesce；`Mapping.guess` 的实现是 **Algolia 全文检索 + 过滤 `kind:<type>` + `episodeCount:(n-2) TO (n+2)`**（用集数 ±2 窗口做轻量打分/剪枝），取 `.first`。
**未匹配行直接丢弃**：`apply` 里 `next if media.blank?` —— **没有 no-match 队列、没有 dry-run、没有人工复核界面**（`ListImportsController` 整个文件只有一行 `class ListImportsController < ApplicationController; end`）。这是明确的缺失项。
其他源：`ListImport::MyAnimeList` 只接受公开用户名（`validates :input_text, presence: true` + `validates :input_file_data, absence: true`），创建时同步请求 `https://myanimelist.net/{anime,manga}list/<user>` 检查 403/404 并把错误挂到 form error（**"导入前先验证源可访问"这一 UX 前置检查**），分页 300 条 + `sleep 2` + 429 时 `sleep 10; redo`；它还有一个**日期格式嗅探** `date_format`：扫任一行 `start_date_string`，若首段 >12 判 `%d-%m-%y`、次段 >12 判 `%m-%d-%y`（应对 MAL 用户区域不一致）。`MyAnimeListXml`/`Aozora`/`TaigaXml`/`AnimePlanet` 分别处理 XML 上传与 HTML 页面粘贴。
**从本地媒体文件导入：不存在**。批量目录灌入是运维脚本形态：`lib/anidb_category_import/media_importer.rb` 从 `https://media.kitsu.app/import_files/*.json` 拉预生成文件，**以 MAL id 列表为锚**（`unfiltered_anime[:mal_ids].each { Mapping.lookup('myanimelist/anime', mal_id); break if found }`）在既有 catalog 里定位 anime，然后 `kitsu_anime.mappings.where(external_site:'anidb', external_id: ...).first_or_create` 追加新源映射，并用 genre→category 映射文件回填分类（`Chewy.strategy(:bypass)` + 关 AR logger 以提速）。→ **catalog 本体不是从外部源灌的，外部源只负责"补映射 + 补分类"**；这一点和 AssetMesh「Douban 行即 catalog 种子」方向一致。

### 4. 增量同步

- **对外写回（Kitsu → MAL）**：这是最完整的一条增量同步实现。`LibraryEntry` 上 `after_commit(on: :create|:update|:destroy, if: :sync_to_mal?)` → `LibraryEntryLog.create_for(:create|:update|:destroy, self, myanimelist_linked_account)` → `ListSync::UpdateWorker.perform_async(account_id, entry_id)`（destroy 走 `ListSync::DestroyWorker(media_type, media_id)`）。`sync_to_mal?` 条件：`media_type in %w[Anime Manga]` && `!imported` && 存在 `LinkedAccount::MyAnimeList` 且 `sync_to: true`。→ **用 outbox 表（`library_entry_logs`）记录待推送变更并回写 `sync_status: :success`**，`ListSync::SyncWorker` 用 `sidekiq_options retry: false, queue: 'soon', debounce: true` 做全量刷（按 `%i[anime manga]` 分批建 log 后 `linked_account.list_sync.sync!(kind)`），`capture_sync_errors` 收集失败。`ListSync::MyAnimeList` 下有 `login`/`cookie_jar`/`xml_downloader`/`xml_uploader`/`library_updater`/`library_remover`/`mechanized_edit_page` —— 即 MAL 无写 API 时**用 Mechanize 模拟表单页写入**。权威侧：**Kitsu 为权威、MAL 为镜像**（单向 push）。
- **元数据定时刷新（外部源 → Kitsu）**：`TheTvdbDailyWorker#perform → TheTvdbService.new(TheTvdbService.currently_airing).import!`（每天，只处理"当前在播"集合）、`TheTvdbWeeklyWorker`。→ **不做全表 TTL 轮询，而是按"仍在变化"的状态子集来选目标**（对比 MediaTracker 的"每小时扫全库 + lastTimeUpdated 24h/30d"）。
- **水位 / 字段级 diff**：**catalog 侧没有 watermark 列**，靠 `updated_at` + `after_save` 触发 Chewy/Algolia 重建索引；用户记录侧靠 ActiveRecord 脏属性（`saved_change_to_progress?` / `progress_changed?` / `status_changed?`）与 `LibraryEntryDiff` 做**字段级差分**，再决定 feed、`time_spent`、`rating_frequencies`、`trending_vote`、stats 重算。**没有软删除/墓碑**（`destruction_worker.rb` / `user_content_reparent_worker.rb` 是异步硬删与内容重挂），删除事件通过 outbox log 显式推给 MAL。
- **一致性/回填类 job**：`AverageRatingUpdateWorker`、`UpdateRatingFrequencyWorker`、`RankingUpdateWorker`（`index_anime_on_wilson_ci` 支持 Wilson 下界排序）、`CounterCacheResetWorker`、`GlobalStatRecalculationWorker`、`AirningNotificationSchedule/SendWorker`、`ShrineDerivativeWorker`、`PostgresSequenceWorker`。人工纠错通道是 `wiki_submissions`（带 `wiki_submission_logs`）与 `media_attribute_votes`，**即"目录变更需审核"而非"provider 覆盖一切"**。

### 关键证据清单

- [db/schema.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/db/schema.rb) — 全库表/列/索引唯一真相：无 `media` 表、per-type `anime`/`manga`/`dramas`、`mappings` 唯一索引、`library_entries` 唯一 `(user_id, media_type, media_id)`、episodes 的 `season_number`、hstore `titles`、`scrapes`、`list_imports`、`versions`。
- [app/models/mapping.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/mapping.rb) — 外部身份映射的模型、每站点单 id 约束、`Mapping.lookup` find-or-create、`Mapping.guess` 的 Algolia + episodeCount ±2 模糊回退。
- [app/graphql/types/enum/mapping_external_site.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/graphql/types/enum/mapping_external_site.rb) — 18 个 `provider`/`provider/granularity` 形式的站点词表（含 `thetvdb/season`、`imdb/episodes`）。
- [app/models/concerns/mappable.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/concerns/mappable.rb) · [app/models/concerns/media.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/concerns/media.rb) — concern 即"是否是一种 Media"的多态白名单；`by_mapping`/`find_by_mapping`；franchise/installment/relationship/favorites 的挂载全景。
- [app/models/concerns/titleable.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/concerns/titleable.rb) — `canonical/original/romanized_title` 是**指向 titles map 的 locale 指针**而非文本，并要求至少一个 `en*` 标题。
- [app/models/library_entry.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/library_entry.rb) — status 五态枚举、rating 2..20、progress 上限校验、`imported` 开关、自动状态机、`anime_id/manga_id/drama_id` 与多态键双写的 `one_media_present` 校验、MAL outbox 钩子。
- [app/services/library_entry_diff.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/services/library_entry_diff.rb) — 把一次保存拆成 progress/reconsume/time/became_completed 差分，驱动 stats 与 feed。
- [app/models/list_import.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/list_import.rb) — STI 导入框架、`strategy = greater|obliterate`、`merged_entry` 的 `[completions, progress]` 偏序取大、`Retriable on RecordNotUnique`、`partially_failed` 状态与 error 落库、`apply_async!`/`retry_async!`。
- [app/models/list_import/anilist/row.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/list_import/anilist/row.rb) — 三级解析（anilist id → MAL id 交叉 → guess）+ **命中后回写新映射**+ 字段名归一（`repeat→reconsume_count`、`planning→planned`、100 分制→20 分制 `ceil(/5)` 且夹到 ≥2）。
- [app/models/list_import/my_anime_list.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/list_import/my_anime_list.rb) — 公开列表可访问性预校验（403/404 转 form error）、分页 300 + 限速重试、日期格式嗅探。
- [app/workers/list_import_worker.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/workers/list_import_worker.rb) · [app/controllers/list_imports_controller.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/controllers/list_imports_controller.rb) — 异步入口（并显式跳过 MAL）；controller 为空壳，**证明没有 dry-run/预览/人工复核端点**。
- [app/models/concerns/episodic.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/concerns/episodic.rb) · [app/models/episode.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/episode.rb) — 无 season 表、`season_number` 默认为 1、`progress_limit = episode_count` 别名、`validated` 状态门控、集长众数/均值回算、`create_defaults` 按数量补/删集。
- [app/models/casting.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/models/casting.rb) — "person 或 character 二选一"的多态信用约束。
- [app/services/list_sync/my_anime_list.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/services/list_sync/my_anime_list.rb) 目录（`login`/`cookie_jar`/`xml_uploader`/`library_updater`/`mechanized_edit_page`） · [app/workers/list_sync/sync_worker.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/workers/list_sync/sync_worker.rb) — 出站同步：debounced Sidekiq、outbox log + `sync_status`、错误捕获、Kitsu 为权威侧。
- [app/workers/the_tvdb_daily_worker.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/app/workers/the_tvdb_daily_worker.rb) — `TheTvdbService.new(TheTvdbService.currently_airing).import!`：按"在播子集"而非全表 TTL 做刷新。
- [lib/anidb_category_import/media_importer.rb](https://raw.githubusercontent.com/hummingbird-me/kitsu-server/e6575ed9fd73ba8cccb920fe2e3f1ef873a71333/lib/anidb_category_import/media_importer.rb) — 批量灌入的匹配方式：以 MAL id 为锚 `Mapping.lookup`、`first_or_create` 追加 anidb 映射、`Chewy.strategy(:bypass)`。

## 对 AssetMesh 的启示

本节对照本仓库的真实代码与前文五个项目的一手结论。AssetMesh 侧的事实来自 `crates/core/src/domain/{asset,media,external_ref,relation}.rs`、`crates/core/src/application/{import_media,portable}.rs`、`migrations/0001_core_media_v1.sql`，以及当前桌面库 `~/Library/Application Support/com.assetmesh.desktop/assetmesh.db` 的实际内容：**785 条 `media.*` 资产（movie 335 / anime 241 / game 138 / tv 71）、648 条 `douban` 外部引用，relations 0 条、tags 0 条、software 0 条、service 0 条。**

### A. 已经比它们更严的地方（不要为了"跟业界一致"而放松）

| AssetMesh 现状 | 对照结论 |
|---|---|
| `external_refs` 是 `UNIQUE(namespace, external_id)` + `asset_id NOT NULL REFERENCES assets(id)` + `idx_external_refs_asset` | **比全部五个项目干净。** MediaTracker 用 9 个可空列摊在宽表上，且建唯一索引前必须先做一次"空串 → NULL"清洗（否则多个 `''` 互相判为冲突）；bangumi 只有一个非唯一的 `subject_uid varchar(20)` 混装 isbn/imdb。我们的列式约束天然没有空串问题 |
| `MediaStatus::can_transition_to` 状态机 + `validate()` 的不变量，且**导入路径只走 `validate()`、交互式变更才走转移矩阵**（`media.rs` 注释明确区分） | 比 Kitsu 干净。Kitsu 的自动状态机（`progress == progress_limit` 自动置 completed、`progress` 变化而 status 未变则自动改 `current`）在导入时必须靠一个**不入库的 `attr_accessor :imported` 运行时旗标**整段 `unless imported` 关掉——同一个模型两套行为。我们用"哪条写路径生效哪些规则"来表达，比运行时旗标可审计 |
| `Progress { current, total, unit }` 结构化，且 `unit` 允许 `chapter`/`volume`/`percent`，`current`/`total` 可全空（游戏常无进度） | 比 bangumi 的 `interest_ep_status`（纯整数计数）和 Kitsu 的 `progress` + `progress_limit`（绑死"集"）都通用 |
| `Asset.revision` + `LifecycleState::Merged` + `merged_into` 墓碑重定向 | 比 Ryot 的 `merge_metadata` 可解释。Ryot 合并 = 把 `seen`/`review`/`collection_to_entity` 搬到新行然后**删掉旧行**，不留 redirect；我们的墓碑让历史 activity 永远可解释 |
| 导入的四级匹配优先级 + `dry_run` 完全不写库 + "heuristic 匹配只报告、从不写入" | **严格程度超过全部五个项目。** MediaTracker 无 dry-run、无人工复核队列，Trakt 导入里未匹配项被 `catch(e){}` 静默丢弃；Kitsu 的 `ListImportsController` 是一个空壳文件，`apply` 里 `next if media.blank?` 直接丢行；Ryot 有 `import_report` + `ImportFailStep` 四阶段失败分类但没有预览 |
| 不存 provider 原始 payload（ADR 0009） | 与五个项目全部一致。这条不用再讨论 |

### B. 别人有、我们缺（按性价比排序，给到表/字段级）

**B1. 分集进度 —— 优先做，影响面最大。**
785 条里 tv + anime = 312 条。现状：`media_records` 每个资产只有一组 `progress_current/total/unit`，所以"第 3、7 集跳过、第 8 集看到一半"无法表达，只能压成 `8/28`。
最小改动（新 migration `0005_media_episodes_v1.sql`，media 模块 `SCHEMA_VERSION` 1→2，符合 ADR 0008 的模块独立版本）：

```sql
CREATE TABLE media_episodes (
    id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    season_number  INTEGER NOT NULL DEFAULT 1,
    episode_number INTEGER NOT NULL,
    title TEXT, airdate TEXT,
    UNIQUE (asset_id, season_number, episode_number)   -- 幂等重灌的全部依据
);
CREATE TABLE media_episode_views (
    episode_id TEXT NOT NULL REFERENCES media_episodes(id) ON DELETE CASCADE,
    viewed_at TEXT NOT NULL,
    provenance TEXT NOT NULL CHECK (provenance IN ('manual','imported')),
    PRIMARY KEY (episode_id, viewed_at)                -- 允许"重看"成为事件流
);
```

两条必须一起抄的安全规则，来自 MediaTracker（它在这件事上是五个项目里唯一做对的）：
- 重识别时**用自然键 `season_number/episode_number` 把新 payload 贴回旧行主键**，绝不"删了重建"，否则 `media_episode_views` 的引用会指向不存在的集；
- 删集前**先查是否被用户记录引用，被引用则中止整次删除并降级为"只更新条目头部、保留旧集结构"**。

`progress_current` 保留为「派生计数 + 用户可显式覆盖」的双轨，照抄 MoviePilot 的 `manual_total_episode` + `max()` 护栏（用户手填后远端不再改动；下调时用已确认的最小值夹住）。

**B2. 人物与 credits —— 决定库能不能回答问题。**
现状：导演/演员/声优/原作作者完全没有落点，`notes` 是唯一去处。反面教材是 MediaTracker：因为演职人员只是宽表上的扁平文本列，它永久无法回答"这个导演还拍过什么"、也无法对同一个人做归一。
不要一步做成完整图谱。最小方案：`media_credits(asset_id, person_name, role, character_name, position INTEGER)`，先把事实记下来（bangumi 的 `prsn_appear_eps`、Kitsu 的 `castings.order` + `voice_actor` + `language` 证明这四个字段就够了）；归一留到下一步，做法参考 Kitsu 而非 Ryot——**Kitsu 把人物归一当作映射问题**（`mappings` 表 + `external_site` 取值 `myanimelist/people`、`anilist/...`），**Ryot 干脆不合并**（按 `(identifier, source)` 各存一行）。AssetMesh 已有 `external_refs`，走 Kitsu 这条路几乎零成本。
若要把人物提升为 `AssetKind`：注意 `AssetKind::module()` 目前只返回 `media`/`software`/`services` 三个字面量，新增 `media.person` 需要同时改 `module()`、`AssetKind::ALL` 数组长度、以及所有 `match` 的穷尽分支——是一等公民级改动，不是加个枚举值。

**B3. 片单/集合 —— 解锁 0 条 relations 的闲置面。**
现状只有 `tags`/`asset_tags`（0 条），是弱归属：不能排序、不能描述、不能表达"第 N 部看"。
最小方案：`collections(id, name UNIQUE, description, created_at)` + `collection_items(collection_id, asset_id, position INTEGER, note, added_at, PRIMARY KEY(collection_id, asset_id))`。抄 bangumi 的两点：**成员是行不是 JSON 数组**、带 `idx_rlt_order` 式的人工排序列。
额外收益来自 Ryot 的一个设计：`collection_name == Monitoring` 这个内置具名片单**反过来定义了"哪些条目值得联网核对"**，即增量范围是片单而不是全库。AssetMesh 若将来做豆瓣刷新，应当用同样的方式界定范围，而不是学 MediaTracker 每小时扫 785 行。

**B4. 多语言标题 —— 豆瓣数据里标题语言本来就混。**
抄 Kitsu 的指针列模式，不要加 `title_cn`/`title_en`，也不要另建 alias 表：`assets` 上加 `name_locale TEXT NOT NULL DEFAULT 'zh'`，标题变体存 `asset_names(asset_id, locale, name, kind)`（`kind ∈ {canonical, original, romanized, alias}`），唯一键 `(asset_id, locale, kind)`。Kitsu 还强制"至少要有一个 `en*` 键"，这个约束可以按本地语言调整。Ryot 的 `alternate_names TEXT[]` 是更轻的降级版。
注意 `validate_namespace` 只允许 lowercase ASCII / digit / `_` / `-`，所以 `locale` 用 `zh_cn`、`ja_jp` 而不是 `zh-CN`。

**B5. 进度上限对账。**
`MediaRecord::validate` 现在只校验 `current <= total`，而 `total` 可空——所以"12 集的剧标成 200/200"能写进库。补两条：B1 落地后 `total` 应与 `media_episodes` 行数对账（Kitsu：`entry.progress = [entry.progress, progress_limit].min`）；无 episode 数据时保留用户值但**在 UI 标为未核验**，不要静默改数（Kitsu 用 `episode_count_guess`，其猜测来源是**用户进度众数**，很适配个人库）。

**B6. 增量同步：不需要 cron，但需要「偏序合并」。**
local-first 单用户，不该引入调度器。真正缺的是"我又导出了一份完整豆瓣列表"时的合并语义。现状已核实：`commit_update` 的策略是**导入有值即覆盖、`None` 保留**（`import_media.rs:353` 起的字段策略注释与实现），于是重跑一份进度更小的导出会把 `8/28` 覆盖成 `3/28`。
改成 Kitsu 的 `strategy: greater`——按业务偏序取大而不是 last-write-wins：`progress.current` 单调不减，除非候选显式携带 `status` 变更意图。Kitsu 的实现只有四行（比较 `[completions, progress]` 元组，`(theirs <=> ours).negative?` 则不赋值）。**这一条已实施，见 C2。**
同时抄 MoviePilot 的一点：**刷新写入的是显式字段白名单，用户侧字段完全不在这份名单里**，并且每次写入带 actor + 前像 + scene 标签便于审计"谁在什么场景改了哪一列"。我们已有 activity 事件，缺的是"哪些字段属于刷新可碰的范围"这个常量。

### C. 两个已核实的真实风险（本次已修复）

1. **~~`external_refs.metadata` 是一条未设防的密钥外泄路径~~ → 实为 ADR 0009 的边界违规（已修复）。**
   核实过程里我把这条说重了，更正如下：
   - `source_url` **不是**缺口。`AssetExternalRef::validate()` 已经对它调用了与 Services 同级的 `validation::url_shape()`（`external_ref.rs:52-62`），userinfo、凭据型 query、OAuth fragment 全部拒绝；`external_ref.rs` 自己的测试也覆盖了这几条。
   - 真正的缺口只有 `metadata` 一个裸 `TEXT` 列：`portable.rs` 的 `PortableExternalRefV1` 把它原样搬进 bundle。而 ADR 0009 的措辞正是"Provider cache is excluded by default"、"Cache must be deletable/rebuildable without damaging the canonical library"——所以这是**可移植边界把本地缓存带出去了**，不是 ADR 0010 的密钥边界漏了。
   - 而且当前**没有任何生产写入路径**会填这个字段：`AssetExternalRef::new()` 硬编码 `metadata: None`，media/software/services 三个 IPC 命令传的都是 `external_refs: Vec::new()`，唯一能写入非 `None` 的路径就是 bundle 自己（`into_domain` 读进来、`from_domain` 再搬出去）。也就是说它当时是"零数据的错架构"，不是正在漏数据的 bug——这也是它能被安静修掉的原因。
   - **处置**：从 `PortableExternalRefV1` 移除 `metadata` 字段（导出不再写、导入固定为 `None`），列与 domain 字段保留为 ADR 0009 意义上的本地缓存并在文档注释里标明"local provider cache, excluded from export"。旧 bundle 里残留的 `"metadata"` 键仍能解析（struct 未加 `deny_unknown_fields`）且值被丢弃——`legacy_bundle_ref_metadata_key_is_ignored_on_import` 锁住了这条兼容路径。
2. **`commit_update` 的进度合并是 last-write-wins（已修复，见 B6）。**
   `import_media.rs` 的字段策略"导入有值即覆盖"对文本字段是对的，对**计数**是错的：重跑一份更早的导出会把 `8/28` 覆盖成 `3/28`，而"看到第几集"恰恰是只有用户自己知道的增量事实。
   **处置**：`progress.current` 改为按最大值合并（Kitsu 的 `strategy: greater`），`total` 因描述的是"作品"而非"用户的状态"仍可双向修正，`unit` 跟随候选；合并结果若违反 `current <= total`（或出现只有 unit 没有数值），**整体拒绝并保持原值**，同时写入 `progress_merge_refused:<reason>` 变更标记进 activity 事件——不用 clamp 解决，因为 clamp 会静默改写用户填的数。
3. **`imported` 语义与不可变资产的交互值得写进文档。**（未处理，属文档项）Ryot 用 `MediaSource::Custom => return err()` 在架构层面把"用户手建条目"排除在 provider 刷新体系外，避免远端覆盖手建数据。AssetMesh 已有等价保护（`Asset::ensure_mutable()` 挡住 archived/merged 资产，且**导入路径同样调用它** —— `commit_update` 里 `asset.ensure_mutable()?` 在 `import_media.rs:365`），但这条保护没有出现在 `docs/08` 的导入契约叙述里。这是一条真实的既有优势，应该显式化。

**同类但未处理的项**（同一处代码、同一种病，改动会牵扯状态机语义，留给有明确产品决策的那一次）：`status`、`started_at`、`completed_at` 在重跑旧导出时仍会倒退（`in_progress → planned`）。它们不像计数那样"只增"，所以不能套用 max；要治本得引入 per-asset 的来源水位（Ryot 的 `is_partial` / Kitsu 的 `imported` attr_accessor 那一类"这批写入属于哪个源"的标记），而不是在合并函数里再加一条特例。

### D. 反面教训（我们恰好避开的坑）

- MediaTracker 的 `findByExternalId` 语义是"**任一**外部 id 列相同即同一 item"（`tmdbId = ? OR imdbId = ? OR tvdbId = ? …` 取 `.first()`），没有置信度。provider 侧一个错 id 就会把两部作品永久合并。我们的"必须精确匹配 `(namespace, external_id)`、heuristic 只报告"从设计上不存在这个失效模式。
- MoviePilot 的刷新白名单里包含 `media_source/media_id`，因此**远端识别改判会让订阅行的主身份漂移**——身份不是不可变锚点。AssetMesh 的 `AssetId` 是 UUIDv7 且从不从外部值推导，这一点比它稳。
- bangumi 的 `subject_type_id` 枚举是 `1/2/3/4/6`，**5 是历史空洞**；`AssetKind` 用字符串而非递增整数，避免了这类不可回收的编号腐蚀。
- Ryot 把季/集整份 JSON 覆盖写，换来 provider 结构零转换，代价是**单集无法参与行级查询/统计**。这是 B1 必须落表的最强反方证据。
