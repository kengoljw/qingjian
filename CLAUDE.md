# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 仓库现状

跨平台输入法，Core 平台无关，各平台只做壳。**2026-10-28 起按 Windows 单平台开发**：
只编 Windows（`apps/windows` + 共用的 `apps/settings` / Core），macOS 与 Linux 的壳不编译、不验证、不加功能；
发布只出 `dist\青简-<日期>-<序号>\` 热更新包（server + settings + update.cmd + 说明），不做签名 / 安装包 / CI 发版。
口径来自用户 2026-10-23「只考虑 Windows，不用考虑其他平台」与 2026-10-28「完全不管 / 只用热更新包」。

代码结构仍是跨平台（Core 平台无关）；当前版本 `0.2.2-dev`，上一次发版 0.2.1（2026-10-14），
五笔 86、混输、辅码、主题、符号面板等已在 0.2.0 / 0.2.1 上线。
阶段与已完成项见 `docs/plan/roadmap.md`，待办见 `docs/plan/todo.md`，本机进度见 `PROGRESS.md`。

## 目录地图

一行一个，只说它是什么、入口在哪；实现要点（数据文件、常数、生成命令）在 `docs/notes/crate-notes.md`，改了实现要同步那里。

- `crates/qingjian-core`：引擎。`Engine` 是对外唯一门面，`Translator` / `Learner` 等 trait 在 `engine` 模块；拼音解析、纠错、候选、排序、整句、双拼、注音、英文模式、繁体输出、emoji 与符号都在这里。
- `crates/qingjian-dictionary`：词库（TSV 或 `.qj` mmap），按音节位置二分查询。
- `crates/qingjian-translate`：释义表 `Glossary`、词汇等级表 `LevelTable`。
- `crates/qingjian-learning`：用户侧落盘：词频 / 用户词 / 个人 n-gram / 敲错表（`FrequencyLearner`）、输入日志（`InputLog`）、输入统计（`UsageStats`）、词汇记录（`VocabularyBook`）。
- `crates/qingjian-predict`：云联想 `CloudPredictor`（OpenAI 兼容接口）与释义兜底 `CloudGlossFiller`；`PredictConfig` 是 `[predict]` 分节。
- `crates/qingjian-lm`：整句转换的 bigram 语言模型 `BigramModel`。
- `crates/qingjian-neural`：字级 Transformer 本地推理 `CharScorer`（candle），给整句前几条路径重打分。
- `crates/qingjian-format`：`.qj` 数据容器（mmap 读、零拷贝视图、写入器、哈希索引）。
- `crates/qingjian-platform`：平台层共用：`Config`（TOML 配置）、`extra_dictionaries`、Windows Server ↔ DLL 的 `protocol` 类型。
- `crates/qingjian-render`：候选窗与状态条：帧数据 + 主题 → 场景树，画在 bamboo-slip 上，各平台只贴图。见 `docs/design/rendering.md`。
- `crates/bamboo-slip`：自绘 UI 引擎（场景树、布局、文字、动画 → 位图，另有控件库与窗口宿主），候选窗与设置程序共用；不放任何青简的概念，约定见该目录的 `CLAUDE.md`。双许可，见该目录的 `LICENSE.md`。
- `crates/qingjian-update`：检查更新：读官网的 `releases.json`、验 ed25519 签名、按平台与渠道挑新版本，只提示不安装；发版侧的签名工具是 `tools/release-sign`。见 `docs/design/update.md`。
- `apps/settings`：设置程序（共用代码，画在 bamboo-slip 上，读写同一份配置文件；当前只编 Windows），页面与设置项按 `docs/design/settings.md`；Windows 与 Server 同目录。`--png <目录>` 拷一份配置按脚本截图。
- `apps/cli`：Core 的验证工具：查询、逐键计时、输入日志回放、整句评测、常数扫描。排序 / 整句 / 纠错的改动先跑它再合。
- `apps/macos`：IMK 壳，按 `app / host / imk / candidates / menubar / preferences` 分目录（`preferences` 是旧版设置，新版 `apps/settings` 为主、旧版只作兜底）；`scripts/bundle.sh --install` 装到本机，`--pkg` 出分发包。**当前不维护：不编译、不验证。**
- `apps/windows`：`server`（Server 进程：Engine + IPC + 自绘候选窗与状态条）+ `tsf`（TSF DLL）+ `settings`（旧版设置，WinUI 3，只作兜底，新版 `apps/settings` 为主）+ `installer`（Inno）。DLL 不能带 Engine 的依赖树，所以是两个 package。
- `apps/linux`：`server`（Engine + Unix socket）+ `fcitx5` / `fcitx4`（框架事件、默认面板）+ `scripts`（用户级安装卸载、`package.sh` 打 Fcitx5 预编译包）；Server 手动启动，见 `docs/notes/linux-fcitx5.md` 和 `docs/notes/linux-fcitx4.md`。**当前不维护：不编译、不验证。**
- `tools/dict-convert`、`tools/gloss-gen`、`tools/corpus`：产品数据生成（词库 / 语言模型 / 释义表 / emoji / 英文词表），输出到 `data/generated/`（gitignore）。
- `assets/`：随包数据源与样例，各目录有 README 写来源与许可。雾凇拼音（GPL）已彻底移除，不要再引入。

`docs/` 分四类（索引在 `docs/README.md`）：`design/` 设计与决定、`plan/` 路线与待办、`notes/` 工程记录（性能、复盘、踩坑、crate 实现要点）、
`user/` 用户文档（官网构建时拉取渲染，约定见 `docs/user/README.md`，措辞面向用户、不出现实现词）。

## 常用命令

本机 Rust 在仓库内 `.toolchain`（便携安装，`cargo` 不在系统 PATH），先设环境变量：

```powershell
$env:CARGO_HOME="$PWD\.toolchain\cargo"; $env:RUSTUP_HOME="$PWD\.toolchain\rustup"; $env:PATH="$env:CARGO_HOME\bin;$env:PATH"
```

```bash
cargo build                                   # 整个 workspace
cargo test                                    # 全部测试；-p <crate> 单个，加测试名过滤
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo run -p qingjian-cli -- <拼音>...          # Core 的主要验证方式；--replay / --eval-text 见 crate-notes
cargo build --release -p qingjian-windows-server -p qingjian-settings   # 热更新包的两个 exe
```

Windows 真编译与真机测试就在本机做；部署见 `docs/notes/dev-env.md`（`hot-server.cmd` / `install-hot.cmd`）。
macOS / Linux 的构建与验证不再执行（见「仓库现状」）。

## 开发前先看

- 改 UI / 数据 / 构建脚本之前，先扫一眼 [docs/notes/pitfalls.md](docs/notes/pitfalls.md)：
  这一轮术语页反复踩过的坑（布局把滚动区撑爆、拖拽命中顺序、译名两处来源、热加载漏用户数据、
  中文引号与编码、沙箱管道与 TEMP 等）都写清了根因与规矩。
- 用户报的现象先写一条端到端复现（真实 Runner 走事件链，或真实 Server 日志），修完那条必须转绿。

## 约定

架构约束、代码组织、版本号、提交信息、文档同步、提交前检查与发版都在 `docs/contributing.md`，随本文件一起载入：

@docs/contributing.md

交流用中文。
