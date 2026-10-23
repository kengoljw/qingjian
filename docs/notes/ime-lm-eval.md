# 把 chinese-ime-lm 接进青简：同池离线评测（2026-10-22）

上游 [`metasequoiaime/chinese-ime-lm`](https://github.com/metasequoiaime/chinese-ime-lm) 的两个字级重排模型
（4.5 MB / 25.5 MB）能不能给青简的整句候选带来净收益。结论先给：**不能，按决策门停在这里**。

本文所有数字都是 **A 类（本次实测）**，复现命令见文末。

## 结论

| 判断 | 结果 |
|---|---|
| 4M 模型同池重排 | 首选 **−3.88 个百分点**（26.85% → 22.97%），McNemar p = 1.1e-56 |
| 25M 模型同池重排 | 首选 **−1.55 个百分点**（26.85% → 25.30%），McNemar p = 1.9e-11 |
| 对照：青简现役 P2C 重排 | 首选 **+4.09 个百分点**（26.85% → 30.94%），McNemar p = 5.5e-103 |

按移交单 T3 的决策门（「top-1 提升 ≥ 0.5pp 且 McNemar p < 0.05」才进 P1）：

- 两个模型都是**显著变差**，不是「不显著」，符号方向与门槛相反 → **停**。
- 不做 `ExtScorer`（T4），不做按键级双层（T5）。
- 保留本轮产出的评测资产：候选事实字段、同池重排工具、冻结句子集。这部分零风险，正是移交单里
  判断「值得接」的第二层。

为什么变差，看失败案例那节就够了：模型**只吃汉字、不吃拼音**，训练语料是 C4 + LCCC 的通用中文；
青简的评测集是本仓库的技术文档，行话密。模型会把技术词换成更「常见」的同音词。

## 这次做了什么

1. **补候选事实**（`apps/cli/src/eval/mod.rs`）：`--eval-details` 现在为每条候选多写
   `candidate_facts`，含 `kind`、`syllables`、`answers_key`、`trusted_dictionary_hit`。
   后两个是上游 `CandidateFacts` 要的，且**只能由引擎回答**——`xian` 读作 `现` 或 `西安`，字数不同却
   都吃满整段按键，用字数猜必错。
2. **vendor 上游参考实现**（`vendor/chinese-ime-lm/`）：固定 commit、只收 `reference/`、
   保留 Apache-2.0 与语料署名 NOTICE。
3. **同池离线重排工具**（`tools/ime-lm-rerank/`）：读 `base.jsonl` 的候选与事实，调上游
   `Reranker::best_where` 只做「把模型认为更好的那条提到首位」，**不重新生成候选**；输出 `ext.jsonl`。
4. **冻结句子集**：58 个 `docs/*.md` 按标点切句、按词库读音转全拼，**13,622 句**，
   存 `data/eval/sentences.tsv`（`data/` 已被 gitignore），后续任何排序改动都在同一份集上比。

## 数字

同一批候选、同一份 13,622 句；指标从各记录的 `candidates` 现算，不用记录里的 `position`
（那只在原始顺序下有意义）。

| 配置 | 首选 | 前三 | 字准确率 | 相对 base |
|---|---|---|---|---|
| base（静态：二元 LM + 个人 n-gram，冷启动） | 26.85% | 32.29% | 70.39% | — |
| **p2c（含章·通变重排，青简现役）** | **30.94%** | 34.41% | 73.12% | **+4.09pp**，p = 5.5e-103 |
| ext 4M（上游 4.5 MB） | 22.97% | 32.29% | 67.20% | −3.88pp，p = 1.1e-56 |
| ext 25M（上游 25.5 MB） | 25.30% | 32.29% | 68.88% | −1.55pp，p = 1.9e-11 |

配对 McNemar 精确检验（首选正确与否，对 base）：

| 配置 | 翻好 | 翻坏 | 净 | p |
|---|---|---|---|---|
| p2c | 652 | 95 | **+557** | 5.5e-103 |
| ext 4M | 309 | 837 | **−528** | 1.1e-56 |
| ext 25M | 383 | 593 | **−210** | 1.9e-11 |

几点读法：

- **前三完全不动**（都是 32.29%）。重排只在同池内换首位，不改变候选集合，所以前三、前五不可能变——
  这本身是个自检：如果前三动了，说明工具偷偷重排了候选集合，数字就不可信了。
- 25M 比 4M 好但仍为负，与上游「25M 精度到顶」的自我描述一致（它确实比 4M 强），
  只是**这个「顶」仍在青简的静态排序之下**。
- base 首选只有 26.85%，是文本性质决定的：评测集是本仓库的技术文档，行话密、冷启动无个人 n-gram；
  与本仓库历史记录里 `docs/` 那一档的 27.8% 吻合，说明这把尺子没跑偏。

## 延迟

单句（一句一次决策）。

| 配置 | p50 | p95 | 最慢 | 口径 |
|---|---|---|---|---|
| base（引擎查询） | 2.9 ms | 7.5 ms | 27.9 ms | CLI 自带计时 |
| p2c（引擎查询含重排） | 139.9 ms | 350.9 ms | 6184.6 ms | CLI 自带计时 |
| ext 4M（离线重排） | 38.8 ms | 67.7 ms | 231.2 ms | `ime-lm-rerank` 逐句计时 |
| ext 25M（离线重排） | 237.6 ms | 504.3 ms | 4675.4 ms | `ime-lm-rerank` 逐句计时 |

- ext 的延迟是**离线工具**的进程内耗时（含前文编码与批量打分），不等于产品里异步接法的墙钟延迟；
  但量级已经远超移交单的 20 ms 门槛：4M 的 p95 是 base 的九倍，25M 是六十七倍。
- 也就是说，即使首选有提升，**同步接法也不成立**，只可能走「停键后异步」——而青简那个位置已经
  被更强的 P2C 占着。

## 失败案例（4M，翻坏 837 条里抽 12 条）

全部是同音/近音下的「常见词压倒技术词」：

| 原句（正确） | 模型选的 | 拼音 |
|---|---|---|
| 必须平台无关 | 必须平台**五官** | bixupingtaiwuguan |
| 只做两件事 | 制作**两件是** | zhizuoliangjianshi |
| 翻译调用或文本变换 | …文本**变化** | fanyidiaoyonghuowenbenbianhuan |
| 这类多语言并列的数据结构 | …数据**结果** | zheleiduoyuyanbingliedeshujujiegou |
| 任何为学习功能增加的延迟 | …增加的**演出** | renheweixuexigongnengzengjiadeyanchi |
| 英切换始终是布尔 | …始终是**不二** | yingqiehuanshizhongshibuer |
| 与自由函数 | 与自由**汉书** | yuziyouhanshu |
| 绝不用文件名前缀分组 | …前缀**分子** | juebuyongwenjianmingqianzhuifenzu |
| 结构体与构造留在 | …构造**六载** | jiegoutiyugouzaoliuzai |
| 不专门清扫 | 不专门**请到** | buzhuanmenqingsao |
| 这类不用 | 这类**不同** | zheleibuyong |
| 为不透明 | 为不**又名** | weibutouming |

对照 P2C 的翻好方向（同一把尺子）：`同因此→同音词`、`京太累→静态类`、`就错→纠错`、
`蜜月→密钥`、`美剧→枚举`、`挡子弹→当字典`——它**看得见拼音**，所以能选对行话。

这条差异是根因，不是调参能修的：`reference/` 的 `encode()` 只把汉字映射成 token，
拼音根本不进模型；`answers_key` 只能保证「这些候选读的是同一串键」，不能告诉模型哪个读法是对的。

## 未验证 / 口径说明

- **没有用上移交单里的 8,322 句 NAS 冻结集**——那批数据不在仓库、也不在 `data-v7` 包里。
  本轮用的是本仓库 `docs/` 自建的 13,622 句。两者**不可直接比数**（移交单里 P2C 的 41.80%→42.49%
  是在 NAS 集上量的），但本轮四个配置是**同一份集**，相对关系有效。
- `answers_key` 是**近似实现**：按 `engine::commit` 的真实消耗语义分类，再用「候选音节数 == 整段切分
  音节数」比对。精确做法要走 `align()` 的字节消耗比较，本轮未做；误差方向是「把吃满键的候选判成没吃满」，
  只会让重排更保守、不会放大收益。
- `trusted_dictionary_hit` 近似为「`kind == Chinese` 且吃满键 且没有纠错」。**模糊音未纳入**：
  开了模糊音时，词典命中的可能是模糊扩展出来的变体，上游明确说这种情况不能算可信命中。
  本轮评测集所有句子都能精确切分，模糊音未开，所以不影响结论；接产品前要补。
- 上游的两个测量陷阱（按字归一、同池比较）已遵守；**没有**测「不做重排、直接把模型当解码器」——
  上游自己说那条路还不如它本想改进的引擎，不值得测。
- 只测了 CPU 单线程。上游提到的 int8 量化、投机打分等优化都没做——首选已经跑输，优化没有意义。

## 复现

从零到出表（这台机器上 `cargo` 的 TLS 需要绕开，见下节）：

```bash
# 1. 数据（144,104,815 字节，SHA-256 e259f8bc…d79e40）
tools/release/data-fetch.sh          # 或手工解开 qingjian-data.tar.gz 到仓库根

# 2. 冻结句子集（13,622 句）
./target/release/qingjian-cli --eval-text data/eval/corpus.md \
  --eval-save data/eval/sentences.tsv --misses 0

# 3. 三条基线/对照组（同池）
./target/release/qingjian-cli --eval-text data/eval/sentences.tsv \
  --eval-details data/eval/base.jsonl --misses 0
./target/release/qingjian-cli --eval-text data/eval/sentences.tsv \
  --eval-p2c data/models/hanzhang-tongbian/hanzhang-tongbian-small.qjm \
  --eval-details data/eval/p2c.jsonl --misses 0

# 4. 上游模型重排同一批候选
./target/release/ime-lm-rerank <model.safetensors> data/eval/base.jsonl data/eval/ext-4M.jsonl

# 5. 汇总 + McNemar
python tools/ime-lm-rerank/analyze.py \
  base=data/eval/base.jsonl p2c=data/eval/p2c.jsonl \
  ext4M=data/eval/ext-4M.jsonl ext25M=data/eval/ext-25M.jsonl
```

模型权重（Apache-2.0）：
`https://huggingface.co/metasequoiaime/pinyin-ime-reranker-4M` 与 `…-25M` 的
`sentence-model.safetensors`。

## 改动清单

| 文件 | 行为影响 | 回滚 |
|---|---|---|
| `apps/cli/src/eval/mod.rs` | `--eval-details` 多写一个 `candidate_facts` 字段；旧字段不变，产品路径不涉及 | 删掉该字段与两个私有函数即可 |
| `tools/ime-lm-rerank/` | 新增离线工具，不参与任何产品二进制 | 删目录 + 从根 `Cargo.toml` 的 members 摘掉 |
| `vendor/chinese-ime-lm/` | 第三方参考实现，只在上面那个工具里被引用 | 删目录 |
| `Cargo.toml` | members 加一项、`exclude = ["vendor"]` | 同上 |
| `Cargo.lock` | 多 `chinese-ime-lm` 一项 | 随之回滚 |

**产品默认路径零改动**：不装模型、不加开关时，`qingjian-cli` 的行为与改前一致（只有
`--eval-details` 的输出多一个字段）。

## 环境备注（这台机器的坑，复现时会再遇到）

1. **沙箱里 Windows 原生 TLS 不可用**。进程以 Low 完整性级别跑时，schannel 取不到用户证书存储凭证，
   `curl.exe`、`.NET`、`winget`、rustup 的 reqwest 全部失败（`SEC_E_NO_CREDENTIALS`）。
   只有自带 OpenSSL 的 `git` 和 `Python` 能发 HTTPS。提权到 Medium 后 curl 恢复，但下面两条依然成立。
2. **cargo 的 TLS 对 `static.crates.io` 稳定失败**（`index.crates.io` 反而正常）。解法不是改配置，
   而是用 Python 按 `Cargo.lock` 把 567 个 `.crate` 预下载进 `CARGO_HOME/registry/cache/`，
   让 cargo 只读缓存。
3. **链接器写不了系统 TEMP**：`link.exe` 报 `LNK1104`，把 `TMP`/`TEMP` 指到仓库内目录即可。
4. **rustup 装不上**（同第 1 条），改用 USTC 镜像的**明文 HTTP** `RUSTUP_DIST_SERVER` 绕过 TLS 后成功。
5. **代理对大文件限速约 40 KB/s，且会返回「假完整」的数据**（长度对、尾部是零填充）。
   并行分段（16 路）下载再整份校验 SHA-256 才可靠。
6. `--eval-text` 读的文件里**只要有一个制表符，整份就会被当成冻结的三列格式**。拿 markdown 当语料时
   必须先把 tab 换成空格，否则只会抽出个位数的「句子」。

这六条都是环境问题，不是青简的缺陷；但换机器复现时值得先扫一眼。
