# vendor/chinese-ime-lm

上游 [`metasequoiaime/chinese-ime-lm`](https://github.com/metasequoiaime/chinese-ime-lm) 的
`reference/` 目录，原样收录，供青简做**离线同池重排对比**用。

| 项 | 值 |
|---|---|
| 上游仓库 | https://github.com/metasequoiaime/chinese-ime-lm |
| 固定 commit | `4e745be72f1c49e9c7b5dadea819fbb1fcda0e13` |
| 收录范围 | 只收 `reference/`（Rust 推理参考实现）与仓库根 `NOTICE` |
| 代码许可 | **Apache-2.0**，见 `LICENSE`（上游对 `reference/` 单独授权，与仓库根 GPL-3.0 区分） |
| 语料与权重署名 | 见 `NOTICE`：C4（ODC-BY）、LCCC（MIT） |

## 为什么不进 workspace.members

根 `Cargo.toml` 的 `members` 是显式列表，`workspace.lints` 会套到每个成员上；上游代码
自带一套 lint 约定（`unsafe_code = "forbid"`，对 `chunks_exact_to_as_chunks` 显式放行），
收进成员会被青简的 workspace lint 误伤。以 path 依赖方式引用同样能编译，故列入 `exclude`。

## 只读，不要改

这是第三方代码。需要改动行为时，在青简侧包一层，不要直接改这里——上游一更新，
改动就会在合并时丢掉。升级时整目录替换并更新上面的 commit 号。
