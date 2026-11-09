# 译文候选格

2026-11-03。需求：**在候选框里直接选到译文**。选中形态由用户定：译文自己占候选格，↑↓ / 空格 / 数字 / 鼠标都能选它，多条义项各占一格。

> **状态：未实现，先不做（2026-11-03）**。同一天用户改了方向：现有的「组句中 `Ctrl+数字` 上屏第 N 格的第一条译词、`Ctrl+Shift+数字` 第二条、鼠标点右侧译词」已经满足需求，不再改候选布局。本文留作备选——将来若要做「像候选一样 ↑↓ 高亮、空格 / 鼠标选译文」，按这里的设计与坑走。

本文是设计稿；实现后按 `docs/contributing.md` 的规矩同步 `candidate-ui.md`、`crate-notes.md` 与用户文档。

## 现状

- 译文是候选的 annotation：`Candidate::translation`（`crates/qingjian-core/src/candidate/translation.rs`），最多两条义项（`Translation::MAX_SENSES`），画在候选右侧（`docs/design/candidate-ui.md`）。
- 上屏译文只有两条路：`Ctrl+数字` / `Ctrl+Shift+数字`（`apps/windows/server/src/dispatch/key/shortcut.rs`）、鼠标点右侧译词（`qingjian-render` 的 `HitTarget::Translation`）。
- 译文不进高亮、不占候选格：空格、方向键、数字键都轮不到它，也没法在两条义项之间用键盘挑。
- 分页只有一份实现：Core 的 `CandidateLayout`（`crates/qingjian-core/src/candidate/layout/`）负责每页几格、云端词补第一页末尾、矩阵视口（`Grid`）。壳只按它给的 `Cell` 画、按格号算键。

## 目标

有译文的候选后面紧跟它的译文，各占一格；选中译文格与「选了那个词再要它的译文」**完全同义**——直接复用 `Engine::commit_translation`，拼音消耗、学习、输入日志、词汇「用过这条义项」全部照旧。

## 交互

`kaifa`，学习语言英语，打开本功能：

```text
1 开发
2 develop
3 开
4 open
5 开始
6 begin
```

- 空格上屏高亮格，数字上屏第 N 格，↑↓ 逐格移动，翻页键按页翻，鼠标点哪一格选哪一格。
- 一条候选有两条义项就占两格（上限仍是 `MAX_SENSES` = 2）。
- `Ctrl+数字`（译词键）语义不变：取那一格**所属词**的第 1 / 2 条义项。
- 选译文的既有口径照旧（`commit_translation` 今天的已知差异不改）：日志 `text`、输入统计的汉字数、`history` 记的是中文候选，译文只进 `source = translation` 与 `used_sense`；译文内部不补中西文空格；不清符号面板。

## 页与格

- `page_size` 不变，含义从「每页几个候选」变成「每页几格（词或译文）」，窗口高度不变。
- 代价：打开后每页能放的中文候选变少——一条两义项的词占 3 格，9 格时一页约 3–5 个词。这是这个形态的固有代价，所以缺省关。
- 云端词仍占第一页末尾 `[predict] slots` 格，v1 **不展开**、注解照旧画。
- 横排展开成矩阵时照格排（矩阵的一行就是一页），`columns` 仍等于 `page_size`；列宽按格里的文字估，译文格受 `MAX_CELL_EMS`（4 字宽）截尾（`devel…`）——已知取舍，需要完整译文时用竖排。

## 为什么放在 Core 的 `CandidateLayout` 里

```text
Engine::query() → CandidateList（词，不带译文）
        │  打开本功能时对整份候选 annotate（分页要知道每条词占几格）
        ▼
CandidateLayout::new(items, page_size, cloud_slots).with_translations(true)
        │  cells() 把可展开的词摊成 Local + Translation…（同一个分页器，items 一律不动）
        ▼
壳：page(page) → Vec<Cell> → 帧里一行一格 → 画 / 命中 / 上屏
```

- **`CandidateList.items` 一律不动**。派生条目若进 `items`，会同时踩到 15 处「一条 = 一个真实候选」的假设：排序（`show_settled_first`、`keep_first_word`、`whole_key_word`）、emoji 跟随、自定义短语固定位、云槽位去重、辅码筛码、词频学习与自动造词、输入日志 `index`/`top`、神经重排、繁体还原表、`annotate` 二次标注……这是本设计最重要的一条边界。
- 分页、云端补位、矩阵视口、翻页键、数字键下标**全部以「格」为单位**，而分页只有 `CandidateLayout` 一份实现。展开写进 `cells()`，其余（`len` / `pages` / `page` / `Grid` / 壳里的 `highlight / page_size`）自动成立。
- 译文是「晚到、可能没有」的（学习语言 `off`、释义兜底还在路上），而 `items` 是一次 query 定死、还要进日志与排序的：**只有布局层的视图能容纳不等长的东西**，`items` 不能。

## 不新增 `CandidateKind` 变体

派生行在帧里就是一个普通 `Candidate`：`text` = 这一条义项的显示文本（词性 + 译文 + 假名，跟 `translation_pos` 走），`kind` 沿用 `Chinese`，`translation` 为空，`syllables` 为空。

理由是协议：`Candidate` 是线上格式，**给它加 `kind` 变体会让没重启的旧 DLL 整条帧解不出来**（`crates/qingjian-platform/src/protocol/mod.rs` 的注释：加变体必须 +1 并重装 DLL，否则连警告都没有）。而派生行除了「渲染时不画注解」之外没有任何特殊语义——`translation` 为空就已经不会画注解、不会有云朵、不会被 `note_displayed` 记成一条译词。为了一个可读性多升一次协议、让所有人重装 DLL 不划算。

同理也不给 `Frame` 加 `slots` 字段：帧里 **一行一格、行号就是格下标** 是仓库的隐性不变量，被五处同时依赖：

| 位置 | 用法 |
|---|---|
| `dispatch/key/input.rs` 的 `slot_index` | 数字键 → 跨页格下标 |
| `dispatch/key/shortcut.rs` 的 `candidate_on_page` | 修饰键 + 数字 → 跨页格下标 |
| `dispatch/composed/mod.rs` 的 `highlight − first` | 报给 DLL / 渲染器的高亮行号 |
| `qingjian-render` 的 `RowContext.index`（译词节点） | 行号就是候选下标 |
| `qingjian-render` 的 `candidate_nodes`（行 → 点击命中） | 点击回传行号 |

一格里一行、行序等于格序，这条不变量原样成立；换成「帧里另加一份 slots 描述、`candidates` 只放词」，上面五处加 `render_data` 的行装配都要各自改一遍，最容易出现「点候选点上邻居」「数字键选错」。

## 帧与上屏

`composed_frame` 按当前页的 `Cell` 生成帧，一格里一行：

1. 先取当前页的**词**（`Cell::Local` / `Cell::Cloud`），对它们做现有的 `annotate`（云端词就是靠这一步拿到注解；`annotate_sentence_translation` 照旧）；
2. 词行：`Cell::Local` 且**被展开过**（`Chinese` / `Code` 且有译文）→ 把 `translation` 摘掉，注解不再画；其余（云端词、整句、emoji、英文）保留注解，与现在一样；
3. 译文行：`Cell::Translation { word, sense }` → 用第 1 步里那条词的义项拼派生候选（词性 + 译文 + 假名）。

上屏只有一个漏斗 `Router::commit_index`：词 → `Engine::commit`，译文格 → `Engine::commit_translation(word, sense)`。数字键、空格、鼠标点击、标点先上屏全都经过它。

生词曝光：`note_displayed` 改从 **layout 当前页的词**取（词行的 `translation` 被摘掉、译文行没有 `translation`，看帧会漏记）。这是本设计唯一一处「帧不再够用」的地方，注释里要写清。

## 哪些候选展开

| 来源 | 展开 | 理由 |
|---|---|---|
| `Chinese`（词库词）、`Code`（五笔词） | 是 | 译文是学习语言、同步可得 |
| `Cloud`（云联想词） | 否 | 它占的是第一页预留格，展开会顶出这一页；云端词一般也没有释义（用户词库译文另立） |
| `Sentence`（整句） | 否 | 译文是壳异步拿的、通常很长，一格放不下 |
| `English`（中英混输） | 否 | 它的 `translation` 是中文释义，方向相反 |
| `Emoji` / `Kaomoji` / `Symbol` | 否 | 注解是所属词的译文，展开会重复 |
| `Shortcut` / `Custom` / `Generated` | 否 | 没有译文 |

另外：

- **辅码筛码态不展开**：码是选择依据，译文格没有码，展开会让每词多两格还没法按码挑。
- 学习语言 `off`、没有对应释义表、或译文本轮没查到 → 没有译文就没有格，什么都不变。
- 展开开启时自定义短语（`CandidateKind::Custom`）的固定位置不再生效：它按「第 N 格」定位，展开后格号含义变了，v1 按普通候选排。

## 显示

- 词行：序号 + 词，不再画右侧注解（译文已占格）。
- 译文行：序号 + 译文，不画注解、不画云朵，用候选的字体与颜色（它就是可选的候选）。
- 已知取舍（v1）：生词橙色、术语 `◆` 标记都挂在词行注解上，展开模式下看不到。后续给译文行加色调（`Row` 需要新的主题绑定）再补。
- 释义兜底（云端补的译文）在展开模式下要等下一次按键才变成格：布局在 query 时定，`annotate` 的晚到结果不影响已建的格。这是可接受的（缺的译文本来就属于「这次没有」）。

## 配置与设置

- 配置：`[general] translation_candidates`，缺省 `false`。
  打开会改变数字键语义、每页词数变少，属于会改手感的开关，与 `horizontal_grid` 同类：缺省关、设置里给开关、文档写清。
- 设置程序「候选」页加一行「译文也作为候选（可以直接选到译文）」；旧版 WinUI 设置只作兜底，不补。
- 用户文档：`docs/user/learning/translation.md` 加一段；`docs/user/settings/preferences.md` 的表格加一行。

## 性能

- 打开后每次按键要把整份候选 annotate 一遍（`query` 上限 `MAX_CANDIDATES` = 500，常见几十条），而不是只标当前页。释义表是 mmap + 哈希（`docs/notes/performance.md`：每次几十纳秒），`annotate` 另有 `mark_fresh` 的词汇查询。
- 门槛：`cargo run -p qingjian-cli -- kaifa --typing` 的前缀总耗时不超过原来的 1.5 倍、绝对值 < 1 ms。超了按顺序回退：
  1. 布局只展开前 N 页（后面的词按一格算，页数可能少算，`footer` 接受）；
  2. 给 `Glossary` 加 `senses_count(text)` 之类的快查，布局只问条数、不拼 `Translation`。
- 缺省关时零开销：不 annotate、不展开。

## 改动清单

### Core（`crates/qingjian-core`）

| 位置 | 改动 |
|---|---|
| `candidate/layout/cell.rs` | `Cell` 加 `Translation { word: &'a Candidate, sense: usize }`；`candidate()` 对译文格返回**所属词**（矩阵的判空 / 最近的格继续成立）；新增 `display_text()`（译文格返回义项文本，其余返回候选词） |
| `candidate/layout/candidate_layout.rs` | 加 `translation_slots: bool` 与 `with_translations()`；`cells()` 展开；`len()` / `pages()` / `page()` 按格；展开时 `capacity()` 不预留自定义短语固定位；新增 `cell(index)` |
| `candidate/layout/grid.rs` | `column_ems` 用 `display_text()`；其余靠 `candidate()` 自动成立 |
| `candidate/layout/tests.rs`、`grid.rs` | 新单测：分页、云端补位、自定义位、矩阵移动、列宽 |
| `apps/cli` | `--translation-candidates`：打印展开后的格，Core 侧不开 Windows 就能验 |

### Windows 壳（`apps/windows/server`）

| 位置 | 改动 |
|---|---|
| `dispatch/config.rs` | `translation_candidates` 进 `RouterConfig` |
| `dispatch/composed/mod.rs` | `recompose` / `rescore` 共用的 `build_layout`：整份 annotate + `with_translations(开关 && !辅码态)`；`composed_frame` 按上面的三步生成行（词行摘注解、译文行派生）、`highlight` / `first` / `column_ems` 按格；`commit_index` 分流 |
| `dispatch/candidates/mod.rs` | `note_displayed` 改从 layout 当前页取词（没有 layout 时退回帧里的候选） |
| `dispatch/rescore/mod.rs` | 重建后若高亮落在译文格，收口到所属词（重排会让格的内容变） |
| `tests/engine_loop/` | 见验收 |

### 配置、设置、文档

- `crates/qingjian-platform/src/config/general.rs` + `config/mod.rs` 的 `TEMPLATE`：新键与注释。
- `apps/settings/src/view/pages/candidates.rs`（+ `support` 表）：开关。
- 本文件、`docs/README.md` 索引、`docs/design/candidate-ui.md`、`docs/notes/crate-notes.md`、`docs/design/settings.md`、`docs/user/learning/translation.md`、`docs/user/settings/preferences.md`、`docs/plan/todo.md`、`CHANGELOG.md` 下一版。

## 验收

- Core：布局单测全绿；`cargo test -p qingjian-core`。
- 壳：`cargo test -p qingjian-windows-server`，至少覆盖：
  1. 数字 / 空格 / 鼠标点译文格 = 与 `commit_translation` 等价（词频、这个输入串下的选择、输入日志 `source = translation`、`used_sense`）；
  2. ↑↓ 能停在译文格，翻页按格算；
  3. 云端词不展开、仍占第一页末尾预留格、注解照旧；
  4. 学习语言 `off` / 辅码态不展开；
  5. `Ctrl+数字` 语义不变；
  6. 矩阵帧里译文格在位，点击落在对的那一格；
  7. `note_displayed` 传的是词，不带译文文本（`user-vocab.tsv` 轮次不被污染）。
- CLI：`cargo run -p qingjian-cli -- kaifa --translation-candidates` 打出 `1 开发 / 2 develop`（词性开时是 `2 v. develop`）。
- 真机：`hot-server.cmd` 装热更新，记事本 / Edge 打 `kaifa`，数字 2 上屏 develop；连打两个词看每页词数；翻页；Esc 收窗；`[general] renderer = "gdi"` 下也画得出来（GDI 本来就点不了，只能用键盘）。
- 回放：`--replay` 不经过 layout，数字与改前一致。

## 分期

| 期 | 内容 | 估时（单人，含测试） |
|---|---|---|
| P1 | Core：`Cell` / `CandidateLayout` / `Grid` + 单测 + CLI 开关 | 0.5 天 |
| P2 | Windows 壳：配置、帧、上屏分流、曝光、重排重建 + 测试 | 0.5–1 天 |
| P3 | 设置页 + 用户文档 + `CHANGELOG` + 真机 + `--typing` 数字 | 0.5 天 |

## 没做的事、后续

- 生词橙色、术语标记在译文格上（要 `Row` 的色调绑定）。
- 云端词、整句候选的译文格：等「用户词库译文」与整句翻译稳定后再定。
- 译文格的视觉区分（淡色、小一号）——先按普通候选画，看实际用起来再定。
- 「只第一页展开」之类的折中：先看打开的人怎么用。

## 风险

1. **每页词数减半**，打开的人可能觉得中文候选变少。缓解：缺省关、文档写清、设置页一句话说明。
2. **整份 annotate 的性能**：按性能一节的尺子量，超了按回退方案做。
3. **矩阵里译文被截尾**（4 字宽封顶）与**横排单行时窗口变宽**：两个已有机制的已知代价，文档写清。
4. **GDI 渲染器点不了**：GDI 画法本来就没有点击区域（`ui/candidates/click.rs` 的注释），译文格在 GDI 下只能用键盘选；两套画法都要画得出来。
5. **神经重排后高亮可能换格**：重建布局会保留 `highlight`，而格的内容随排序变了，可能指到译文格——重建后收口到词（见改动清单）。
6. **`Cell` 的两种语义**（「格」与「格里的词」）容易用错，约定：要上屏 / 学习，用 `candidate()` 拿词；要显示 / 判空 / 算宽度，用格或 `display_text()`。
7. **跨平台壳**：`Cell` 加变体后，macOS / Linux 壳里对 `Cell` 的穷尽匹配要跟着改（现在按仓库现状不编译、不验证，恢复时记得同步；它们不打开本功能，行为不变）。
