# 双语术语导入流程与审计规范（2026-10-23）

以后会反复导入双语文件（租户手册、合同、进度计划、术语在线查询…）。这篇定目录、定格式、定审计方式，
目标是：**任何一次修订都能回答「哪份原件、哪天、谁改的、改了什么、有没有被篡改」**。

数据只在本机 `data/terms/`（`/data` 已 gitignore）；客户文档与抓取结果**不进仓库、不进发布包**。
流程脚本随数据放在 `data/terms/pipeline/pipeline.py`（本机副本），仓库里只留这份设计说明。

## 分工（2026-10-23 定）

用户只做两件事：**把文件丢进 `data/terms/inbox/`**（或直接给路径）、**在 `reviewed.xlsx` 上审阅并说一声**。
其余（建批次、抽取、生成修订、清洗拆分、合并、快照、校验、刷新镜像）由 agent 执行；
命令行入口是给 agent 用的，用户不需要敲。

`inbox` 目录只放待处理件；agent 用 `inbox` 子命令列出未入批次的文件并给出建议批次号。

## 目录结构

```
data/terms/
├── audit.log                     # 追加写、哈希链；唯一不可重算的历史
├── batches/
│   └── <YYYYMMDD>-<slug>/        # 一次导入 = 一个批次，永不修改 id
│       ├── source/               # 原始文档，只读（docx / pdf / csv）
│       ├── manifest.json         # 来源哈希、抽取参数、产物哈希、状态
│       ├── extracted.txt         # 按文档顺序抽出的纯文本（人工核对行号用）
│       ├── pairs.tsv             # 机器抽取的对照（**生成物，不手改**）
│       ├── skipped.tsv           # 判为对不齐、没采纳的行（不硬塞）
│       ├── reviewed.xlsx         # **唯一可人工编辑的文件**
│       ├── revisions.tsv         # 由 reviewed.xlsx 与 pairs.tsv 的差异生成
│       ├── vocab.tsv             # 清洗/小写/拆分后的词汇（生成物）
│       ├── candidates.tsv        # 更细粒度候选（生成物，带要/不要）
│       └── decisions.md          # 规则性决定：拆分方式、弃用理由、命名统一
├── unified/
│   ├── unified.tsv               # 唯一权威术语表
│   ├── manual-overrides.tsv      # 人工核对译名（压过一切来源）
│   ├── 术语总表.xlsx
│   ├── 冲突与新增.tsv
│   ├── manifest.json             # 当前版本哈希 + 备份清单
│   └── backups/unified-<时间戳>.tsv
├── runtime/                      # 交给输入法的运行态文件（由 unified 生成）
│   ├── user-terms.seed.tsv
│   └── user-glossary-en.tsv
├── legacy/                       # 迁移前的散落文件（只读留档）
└── pipeline/
    ├── pipeline.py               # init / status / ingest / review / publish / merge / snapshot / verify
    ├── normalize.py              # publish 调它：清洗、小写、拆分
    ├── merge_unified.py          # merge 调它：重建统一表
    └── VERSION
```

批次 id 只由「日期 + 来源短名」组成，一旦生成不改；同一来源再导入就是新批次（可对比新旧）。

## 文件格式（列固定，UTF-8 无 BOM，制表符分隔）

| 文件 | 列 |
|---|---|
| `pairs.tsv` | 中文 / English / 类型（表·段）/ 行号 |
| `revisions.tsv` | 时间 / 词 / 字段 / 旧值 / 新值 / 备注 |
| `vocab.tsv` | 中文 / English / 类型 |
| `candidates.tsv` | 类型 / 中文 / English / 出现次数 / 说明 / 要·不要 |
| `audit.log` | 每行一个 JSON 事件（见下节） |

`reviewed.xlsx` 不规定列，但脚本按位置读：第 1 列中文、第 2 列 English、名为「备注 / 你的修订 / 修订」的列
视为人工意见；其余列忽略。

## 流程（每次导入照这个走）

Windows 上不用管 Python：`data/terms/pipeline/run.cmd` 会用 DSH 自带的解释器，**在任意目录都能跑**
（脚本按自身位置定位数据目录）。先 `run.cmd init` 一次，之后每次：

```bat
run.cmd ingest --batch 20261030-xxx --title "某双语文件" --kind docx ^
   --source 原始文件.docx --text 抽取.txt --pairs pairs.tsv ^
   --skipped skipped.tsv --reviewed reviewed.xlsx ^
   --vocab vocab.tsv --candidates candidates.tsv
run.cmd review  --batch 20261030-xxx    :: xlsx → revisions.tsv
run.cmd publish --batch 20261030-xxx    :: 清洗/小写/拆分 → vocab.tsv
run.cmd merge   --note "并入 20261030 批次"   :: 重建统一表 + 快照
run.cmd verify                          :: 每次收工前必跑
```

`run.cmd` 在 `data/terms/pipeline/` 里；从仓库根跑就写 `data\terms\pipeline\run.cmd`，
或者进那个目录在地址栏输入 `cmd` 回车后直接 `run.cmd ...`。

`merge` 从批次里读输入（个人术语库批次、术语在线批次、进度计划批次、各批 `vocab.tsv`），
不再依赖 `legacy/`。合并前自动 `snapshot`，产物写到 `unified/` 与 `runtime/`。

顺序固定：**抽取 → 人工审阅 → 生成修订 → 归一化词汇 → 合并进统一表 → 快照 → 校验**。
每一步都会追加审计事件；任何一步失败，不进入下一步。

## 审计模型

- 每个事件一行 JSON：`seq / ts / event / batch / actor / inputs / outputs / summary / prev / hash / pipeline`
- `hash = sha256(prev + 规范化JSON)`，`prev` 是上一条事件的 `hash`：**改中间任何一条，后面全部对不上**
- `outputs` 记产物 sha256，`verify` 重算磁盘文件与清单比对
- 事件类型：`init / ingest / review / seal / snapshot / merge`（后续再加 `publish`）
- 想查某个词怎么来的：在 `revisions.tsv` 找词 → 看批次 `reviewed.xlsx` 的哈希（在 `review` 事件里）
  → 回到 `source/` 原件与 `pairs.tsv` 行号

## 人工编辑边界（流程纪律）

1. `source/` 只读：原件永不修改；发现原件有错，在 `decisions.md` 写「原件第 N 行应为 X」，不改原件。
2. 只有 `reviewed.xlsx` 可以手改；`pairs.tsv` / `vocab.tsv` / `unified.tsv` 都是生成物，改了会在下次重建时丢。
3. `unified.tsv` 合入前必须先 `snapshot`，备份按时间戳留档，不删旧版本。
4. 同一来源的多次审阅：新导出一份 `reviewed.xlsx` 并作为新批次导入，不覆盖旧批次的审阅件。

## 与仓库、发布的关系

- 数据目录 `data/` 已 gitignore；仓库里只有本文与代码，**没有任何客户文档或词表**。
- 以后若某个批次要作为产品随包数据，另走「自建核心表 → 审核 → assets/」的路，不直接从 `data/terms` 拷。
- 个人术语表运行时落 `%APPDATA%\Qingjian\user-terms.tsv`（由 `unified.tsv` 生成），仍只是本机数据。

## 本次迁移记录（2026-10-23）

| 批次 | 内容 |
|---|---|
| `20261023-tenant-fitout-guide` | 租户装修指南 docx：721 条对照、323 条词汇、265 条候选、9 条人工修订 |
| `20261023-schedule02` | Gucci 太古汇进度计划 PDF：100 行中英对照 |
| `20261023-termonline-seed` | 408 条领域种子 + 术语在线逐词查询结果 |
| `20261023-personnel-glossary` | 你原来的 `personnel\glossary\术语表.csv`（历史权威，已转留档） |

审计链从 `#1 init` 开始；本次迁移收尾后共 20 条事件（含两次 `migrate` 整理、两次 `merge`），`verify` 通过。
旧路径（`data/terms/docs`、`data/terms/pdf`、根目录散落的 tsv/xlsx）已归入 `legacy/` 只读留档。

**仓库外镜像**：`D:\...\personnel\glossary\terms\`（81 个文件、约 22 MB）。真源仍是仓库 `data/terms`，
镜像只作备份；重新镜像后要同步更新 `MIRROR.md` 里记录的 `audit.log` 哈希。

## 后续可加

- `publish` 子命令：把 `vocab.tsv` 与 `unified.tsv` 的合并做成一步（现在是外部脚本）
- 批次对比：两个批次的 `vocab.tsv` diff（新增/改译/删除）
- 多语言列：`vocab.tsv` 扩成 `中文 / en / ja / es / fr / ko`，为译文生成做准备
- 审阅状态机：`draft → reviewed → approved → merged`，`merge` 只接受 approved
