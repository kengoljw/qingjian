//! 双机同步的**编排层**：一天跑一轮，所有平台壳共用这一个入口。
//!
//! 决议 2026-11-06：一天一次、不要求实时、不做 manifest/epoch 水位。
//! 一轮动作：找对端快照 → 原地合并 → flush 写回活动目录 → 导出本机快照 →
//! 人维护表整文件 LWW。全部通过 [`Learner`] trait 与文件系统完成，不碰 IPC/DLL/协议，
//! 也不依赖任何平台壳的类型，所以 Windows / macOS / Linux 壳可以调用同一个函数。
//!
//! 隐私边界：只按白名单搬文件，`input-log.jsonl` / `config.toml` / `.env` /
//! `usage*` / `api-usage*` / `update.json` / `*.bak` / `*.tmp` 一概不碰。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::Learner;

/// 参与同步的人维护表（整文件 LWW，不按行合并）。
/// `dicts/` 单独处理（目录里逐文件）。
pub const MANUAL_FILES: [&str; 2] = ["user-terms.tsv", "user-terms-pending.tsv"];
pub const MANUAL_DIR: &str = "dicts";

/// 一轮同步需要知道的路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPaths {
    /// 青简活动目录（`%APPDATA%\Qingjian`），`flush()` 写回这里。
    pub data_dir: PathBuf,
    /// `sync-box/` 根，下面按设备名分目录。
    pub box_dir: PathBuf,
    /// 本机设备名（`%COMPUTERNAME%`），用来跳过自己的快照。
    pub device: String,
}

impl SyncPaths {
    /// 本机快照目录。
    pub fn own_dir(&self) -> PathBuf {
        self.box_dir.join(&self.device)
    }
}

/// 一轮同步的结果，给日志与设置页提示用。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    /// 合并改了多少条目（0 = 对端没有新东西或还没导出过）。
    pub merged: usize,
    /// 是否找到了对端快照。
    pub peer_found: bool,
    /// 是否已经导出本机快照。
    pub exported: bool,
    /// 人维护表整文件同步了几份。
    pub manual: usize,
}

/// 找对端快照目录：`box_dir` 下除本机外的那个子目录。
/// 没有 = 对端还没导出过（首次引导，只导出不合并）；多于一个 = 配置有问题，报错。
fn peer_dir(paths: &SyncPaths) -> io::Result<Option<PathBuf>> {
    if !paths.box_dir.is_dir() {
        return Ok(None);
    }
    let mut others = Vec::new();
    for entry in fs::read_dir(&paths.box_dir)? {
        let path = entry?.path();
        if path.is_dir() && path.file_name().and_then(|n| n.to_str()) != Some(paths.device.as_str())
        {
            others.push(path);
        }
    }
    match others.len() {
        0 => Ok(None),
        1 => Ok(Some(others.remove(0))),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "sync-box 下有多个对端目录：{:?}；请手工清理后再同步",
                others
            ),
        )),
    }
}

/// 整文件 LWW：谁的文件修改时间新就留谁。取不到 mtime 时按「有比没有新」。
fn newer(src: &Path, dst: &Path) -> io::Result<bool> {
    if !dst.exists() {
        return Ok(true);
    }
    let a = fs::metadata(src)?.modified()?;
    let b = fs::metadata(dst)?.modified()?;
    Ok(a > b)
}

/// 原子复制：先写同目录临时文件再改名，避免 Syncthing 扫到半份文件。
fn copy_atomic(src: &Path, dst: &Path) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = fs::read(src)?;
    // 内容没变就不重写：保持 mtime，避免两机来回抖动。
    if dst.exists() && fs::read(dst).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let tmp = dst.with_extension(format!("qj-sync-tmp-{}", std::process::id()));
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, dst)?;
    Ok(())
}

/// 人维护表双向整文件同步：对端快照 → 活动目录，活动目录 → 本机快照。
/// 两边都是 LWW（单人使用、一天一次，不存在并发编辑）。
fn sync_manual(peer: Option<&Path>, own: &Path, data: &Path) -> io::Result<usize> {
    let mut count = 0;
    for name in MANUAL_FILES {
        let local = data.join(name);
        if let Some(peer) = peer {
            let from_peer = peer.join(name);
            if from_peer.is_file() && newer(&from_peer, &local)? {
                copy_atomic(&from_peer, &local)?;
                count += 1;
            }
        }
        if local.is_file() {
            copy_atomic(&local, &own.join(name))?;
        }
    }
    // dicts/ 下逐文件 LWW；本机独有的词库也要导出给对端。
    let local_dir = data.join(MANUAL_DIR);
    let peer_dirs = peer.map(|p| p.join(MANUAL_DIR));
    let own_dirs = own.join(MANUAL_DIR);
    let mut names = Vec::new();
    for dir in [Some(local_dir.clone()), peer_dirs.clone()] {
        let Some(dir) = dir else { continue };
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let Some(name) = path.file_name() else {
                continue;
            };
            let valid = name
                .to_str()
                .is_some_and(|n| !n.ends_with(".bak") && !n.ends_with(".tmp"));
            if path.is_file() && valid && !names.contains(&name.to_owned()) {
                names.push(name.to_owned());
            }
        }
    }
    for name in names {
        let local = local_dir.join(&name);
        if let Some(peer_dirs) = &peer_dirs {
            let from_peer = peer_dirs.join(&name);
            if from_peer.is_file() && newer(&from_peer, &local)? {
                copy_atomic(&from_peer, &local)?;
                count += 1;
            }
        }
        if local.is_file() {
            copy_atomic(&local, &own_dirs.join(&name))?;
        }
    }
    Ok(count)
}

/// 跑一轮同步。由壳在「开机/登录一次」或「用户点状态条的立即同步」时调用。
///
/// 顺序很重要：先合并进内存 → `flush()` 写回活动目录 → 导出快照，
/// 这样导出的快照包含本轮的合并结果，对端下一轮能收敛。
pub fn run_once(learner: &mut dyn Learner, paths: &SyncPaths) -> io::Result<SyncReport> {
    let own = paths.own_dir();
    fs::create_dir_all(&own)?;
    let peer = peer_dir(paths)?;
    let mut report = SyncReport {
        peer_found: peer.is_some(),
        ..Default::default()
    };

    // 对端快照不存在或还不完整时，整轮跳过合并（只导出本机）。
    // `merge_snapshot` 内部会校验 9 张必备表齐备；失败不覆盖本机数据。
    if let Some(peer) = &peer {
        match learner.merge_snapshot(peer) {
            Ok(merged) => report.merged = merged,
            Err(error) => {
                // 首次引导 / 对端写到一半：都不算故障，记警告后继续导出本机快照。
                tracing::warn!(%error, peer = %peer.display(), "对端快照不可用，本轮跳过合并");
            }
        }
    }

    // 合并结果先落回活动目录（learner 自己知道路径），再导出快照。
    learner.flush();
    learner.export_snapshot(&own)?;
    report.exported = true;

    report.manual = sync_manual(peer.as_deref(), &own, &paths.data_dir)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个只数 merge/export/flush 调用次数的桩，验证编排顺序与首次引导行为。
    #[derive(Default)]
    struct Stub {
        merged: usize,
        flushed: usize,
        exported: Vec<PathBuf>,
    }

    impl Learner for Stub {
        fn record(&mut self, _candidate: &crate::candidate::Candidate) {}
        fn weight(&self, _text: &str) -> u32 {
            0
        }
        fn merge_snapshot(&mut self, _dir: &Path) -> io::Result<usize> {
            self.merged += 1;
            Ok(3)
        }
        fn export_snapshot(&mut self, dir: &Path) -> io::Result<()> {
            self.exported.push(dir.to_owned());
            Ok(())
        }
        fn flush(&mut self) {
            self.flushed += 1;
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qingjian-sync-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_run_exports_without_merging() {
        let root = scratch("first");
        let paths = SyncPaths {
            data_dir: root.join("data"),
            box_dir: root.join("box"),
            device: "T450".to_owned(),
        };
        fs::create_dir_all(&paths.data_dir).unwrap();
        let mut stub = Stub::default();
        let report = run_once(&mut stub, &paths).unwrap();
        assert!(!report.peer_found);
        assert_eq!(report.merged, 0);
        assert_eq!(stub.merged, 0, "对端还没导出过，不能合并");
        assert_eq!(stub.flushed, 1);
        assert_eq!(stub.exported, vec![paths.own_dir()]);
        assert!(report.exported);
    }

    #[test]
    fn peer_found_runs_merge_flush_export_in_order() {
        let root = scratch("peer");
        let paths = SyncPaths {
            data_dir: root.join("data"),
            box_dir: root.join("box"),
            device: "T450".to_owned(),
        };
        fs::create_dir_all(paths.box_dir.join("G5")).unwrap();
        fs::create_dir_all(&paths.data_dir).unwrap();
        let mut stub = Stub::default();
        let report = run_once(&mut stub, &paths).unwrap();
        assert!(report.peer_found);
        assert_eq!(report.merged, 3);
        assert_eq!(stub.merged, 1);
        assert_eq!(stub.flushed, 1);
        assert_eq!(stub.exported, vec![paths.own_dir()]);
    }

    #[test]
    fn multiple_peer_dirs_are_rejected() {
        let root = scratch("multi");
        let paths = SyncPaths {
            data_dir: root.join("data"),
            box_dir: root.join("box"),
            device: "T450".to_owned(),
        };
        fs::create_dir_all(paths.box_dir.join("G5")).unwrap();
        fs::create_dir_all(paths.box_dir.join("OLD")).unwrap();
        let mut stub = Stub::default();
        let err = run_once(&mut stub, &paths).unwrap_err();
        assert!(err.to_string().contains("多个对端目录"), "{err}");
        assert_eq!(stub.merged, 0, "报错前不得合并");
    }

    #[test]
    fn manual_files_flow_both_ways() {
        let root = scratch("manual");
        let paths = SyncPaths {
            data_dir: root.join("data"),
            box_dir: root.join("box"),
            device: "T450".to_owned(),
        };
        let peer = paths.box_dir.join("G5");
        let dicts = paths.data_dir.join("dicts");
        fs::create_dir_all(&dicts).unwrap();
        fs::create_dir_all(&peer).unwrap();
        fs::write(
            peer.join("user-terms.tsv"),
            "词\t读音\n对端术语\tdui duan\n",
        )
        .unwrap();
        fs::write(dicts.join("mine.qj"), "local").unwrap();

        let mut stub = Stub::default();
        let report = run_once(&mut stub, &paths).unwrap();
        assert_eq!(report.manual, 1, "{report:?}");
        assert_eq!(
            fs::read_to_string(paths.data_dir.join("user-terms.tsv")).unwrap(),
            "词\t读音\n对端术语\tdui duan\n",
            "对端较新的术语表要写回活动目录"
        );
        assert!(
            paths.own_dir().join("user-terms.tsv").is_file(),
            "要导出给对端"
        );
        assert!(
            paths.own_dir().join("dicts/mine.qj").is_file(),
            "本机词库要导出给对端"
        );
    }
}
