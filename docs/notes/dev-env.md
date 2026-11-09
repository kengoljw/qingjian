# 本机开发环境

2026-10-28 记录。这台 Windows 机器的 Rust 是**仓库内便携安装**，`cargo` 不在系统 `PATH`；
DSH 沙箱下每开一个新 shell 都要先设环境变量。

## Rust 工具链

- 版本：`1.96.0-x86_64-pc-windows-msvc`（`rust-toolchain.toml` 指定）。
- 位置：`<仓库>/.toolchain/`
  - `cargo/`：`CARGO_HOME`（`bin/` 与 `registry/` 缓存）
  - `rustup/`：`RUSTUP_HOME`（toolchains、settings）
- `.toolchain/` 不进 git；现成脚本是 `.toolchain/env.ps1`。

## 每次开新 shell

```powershell
$env:CARGO_HOME="$PWD\.toolchain\cargo"
$env:RUSTUP_HOME="$PWD\.toolchain\rustup"
$env:PATH="$env:CARGO_HOME\bin;$env:PATH"
cargo --version   # cargo 1.96.0
```

或者 dot-source `env.ps1`（执行策略被拦时先 `Set-ExecutionPolicy -Scope Process Bypass`）；
它顺带设了 `127.0.0.1:7890` 代理，只在要联网拉依赖时用，离线构建可以不管。

## 构建与测试

```powershell
cargo test -p qingjian-core
cargo clippy --all-targets -- -D warnings
cargo build --release -p qingjian-windows-server -p qingjian-settings
```

- 只做 Windows：macOS / Linux 壳不编译（见 `CLAUDE.md`「仓库现状」）。
- 发布只出 `dist\青简-<日期>-<序号>\` 热更新包，不打压缩包、不签名。
- **要发给别人装**（整包装机，不是热更新）走 `tools\windows-package\build-package.ps1`：
  它只依赖 Windows 自带的 IExpress 与 `tar.exe`，不需要 Inno Setup，出单文件自解压 exe。
  正式发版仍用 `apps\windows\installer\build.ps1`（Inno，产出可签名的 setup）。详见
  [windows-package.md](windows-package.md)。

## 部署与真机

- 热更新只换 Server / 设置程序：`hot-server.cmd`、`hot-settings.cmd`；
  DLL 抢跑时用管理员 `install-hot.cmd` 覆盖安装目录，`revert-hot.cmd` 还原。
- DSH 沙箱禁止命名管道，Agent 起不了 Server（exit 101）；最后一步由用户双击。

## 评测数据

- 生成物在 `data/generated/`（gitignore），没有时 CLI 会退回样例词库，评测数字没有意义。
- 日常语料：`data/eval/daily-v1.txt`；v2 的 dev / holdout 见 `data/eval/daily-v2-*.txt`（2026-10-28）。
- 常用命令：

```powershell
.\target\debug\qingjian-cli.exe --eval-text data/eval/daily-v2-holdout.txt --user-dir "$env:APPDATA\Qingjian"
```
