//! 双机同步：把对端快照**原地**合并进当前 learner，并把当前状态导出成一份完整快照。
//!
//! 设计约束（决议 2026-11-06 / 第 1、2 批回传）：
//!
//! - 原地 `&mut self` 合并，**不换 `Box`、不改 `self.path`**。`from_path` 只用来读对端快照，
//!   得到的临时 learner 不装进 Engine、也不调用它的 `flush`，所以不会把本机数据写进快照目录
//!   （第 1 批 v3 Q-A2）。
//! - `(count 取 max, last_used 取 max)`；集合表取并集；**禁止因缺席而删除**。
//! - housekeeping（超上限减半 / `COLD_DAYS` 冷淘汰）不在 merge 路径。导出复用现有
//!   `save_*_to`，与本地 flush 同款，是 `last_used` 的确定性年龄函数（第 2 批 Q-F4）。
//! - 9 张成对表 + `user-blocked.tsv` 整组导出，不按 dirty 过滤；主表与侧车由同一次内存
//!   迭代产生，键集合天然一致。

use std::io::{self, Write};

use super::*;

/// 快照里主表的名字（`from_path` 以它为基准派生其余 9 张表）。
const SNAPSHOT_USER_FILE: &str = "user.tsv";

/// 一份完整快照应有的 10 个文件。`user-blocked.tsv` 允许缺失（按空集处理，兼容老快照），
/// 其余 9 张必须齐备，否则拒绝合并（第 1 批 Q-C5 / 第 2 批 Q-B4）。
pub const SNAPSHOT_FILES: [&str; 10] = [
    SNAPSHOT_USER_FILE,
    USER_DAYS_FILE,
    USER_WORDS_FILE,
    USER_NGRAM_FILE,
    USER_CHOICES_FILE,
    USER_ENGLISH_FILE,
    USER_ENGLISH_DAYS_FILE,
    USER_TYPOS_FILE,
    USER_TYPOS_DAYS_FILE,
    BLOCKED_FILE,
];

/// 一次合并改了多少条目，给日志与单测用。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MergeStats {
    pub counts: usize,
    pub words: usize,
    pub blocked: usize,
    pub ngram: usize,
    pub choices: usize,
    pub english: usize,
    pub typos: usize,
}

impl MergeStats {
    pub fn total(&self) -> usize {
        self.counts
            + self.words
            + self.blocked
            + self.ngram
            + self.choices
            + self.english
            + self.typos
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// 快照不完整 / 格式不对。复用 `LearningError::Io`，不必给公开错误类型加变体。
fn invalid(message: impl Into<String>) -> LearningError {
    LearningError::Io(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

/// 取一张表的数据行键集合。`cols` 是键占的列数（其余列是数值）。
/// 坏行（列数不足）跳过，与青简自己的加载规则一致。
fn key_set(path: &Path, cols: usize, lower: bool) -> Result<BTreeSet<String>, LearningError> {
    let Some(text) = read_text_lossy(path)? else {
        return Err(invalid(format!("快照缺文件：{}", path.display())));
    };
    let mut set = BTreeSet::new();
    for line in data_lines(&text) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() <= cols {
            continue;
        }
        let key = fields[..cols].join("\t");
        set.insert(if lower { key.to_ascii_lowercase() } else { key });
    }
    Ok(set)
}

/// 把 `前词\t后词\t次数` / `前二词\t前词\t后词\t次数` 拆成（键，次数）。
fn split_counted_row(line: &str) -> Option<(String, u32)> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 3 {
        return None;
    }
    let count = fields.last()?.trim().parse::<u32>().ok()?;
    Some((fields[..fields.len() - 1].join("\t"), count))
}

impl FrequencyLearner {
    /// 校验一份快照是否可安全合并：9 张必备文件齐备，且三组成对表的主表/侧车键集合一致。
    pub fn validate_snapshot(dir: &Path) -> Result<(), LearningError> {
        for name in SNAPSHOT_FILES.iter().take(9) {
            let path = dir.join(name);
            if !path.is_file() {
                return Err(invalid(format!("快照缺文件：{}", path.display())));
            }
        }
        let pairs: [(&str, &str, usize, bool); 3] = [
            (SNAPSHOT_USER_FILE, USER_DAYS_FILE, 1, false),
            (USER_ENGLISH_FILE, USER_ENGLISH_DAYS_FILE, 1, true),
            (USER_TYPOS_FILE, USER_TYPOS_DAYS_FILE, 2, false),
        ];
        for (main, days, cols, lower) in pairs {
            let a = key_set(&dir.join(main), cols, lower)?;
            let b = key_set(&dir.join(days), cols, lower)?;
            if a != b {
                let only_main: Vec<_> = a.difference(&b).take(3).collect();
                let only_days: Vec<_> = b.difference(&a).take(3).collect();
                return Err(invalid(format!(
                    "侧车键集合与主表不一致：{main} / {days}，仅主表={only_main:?}，仅侧车={only_days:?}"
                )));
            }
        }
        Ok(())
    }

    /// 把 `dir` 里的对端快照原地合并进当前 learner。
    ///
    /// 只改内存数据与 dirty 标志；`self.path` 保持指向活动目录，合并结果由后续
    /// `Learner::flush()` 写回活动目录（调用方一天触发一次，不是实时节拍）。
    pub fn merge_snapshot_impl(&mut self, dir: &Path) -> Result<MergeStats, LearningError> {
        Self::validate_snapshot(dir)?;
        // 只读借用：临时 learner 不进入 Engine，也不会被 flush。
        let peer = Self::from_path(dir.join(SNAPSHOT_USER_FILE))?;
        let mut stats = MergeStats::default();

        // 词频：count 与 last_used 双取 max。
        for (text, entry) in &peer.counts {
            let slot = self
                .counts
                .entry(text.clone())
                .or_insert_with(|| Decay::from_parts(0, 0));
            let merged = Decay::from_parts(
                slot.count.max(entry.count),
                slot.last_used.max(entry.last_used),
            );
            if merged != *slot {
                *slot = merged;
                stats.counts += 1;
            }
        }

        // 用户词：并集，拼音冲突时保留本机（确定性，避免两机来回抖动）。
        for (text, pinyin) in &peer.words {
            if !self.words.contains_key(text) {
                self.words.insert(text.clone(), pinyin.clone());
                stats.words += 1;
            }
        }

        // 屏蔽表：并集，单调。
        for word in &peer.blocked {
            if self.blocked.insert(word.clone()) {
                stats.blocked += 1;
            }
        }

        // 按输入串记的选择：嵌套键，次数取 max。<raw> 是普通词文本，不特殊处理。
        for (input, texts) in &peer.choices {
            let slot = self.choices.entry(input.clone()).or_default();
            for (text, count) in texts {
                let value = slot.entry(text.clone()).or_insert(0);
                if *count > *value {
                    *value = *count;
                    stats.choices += 1;
                }
            }
        }

        // 个人英文词：键已是小写编码；次数/日期取 max，首次写法保留本机。
        for (code, (word, entry)) in &peer.english {
            let slot = self
                .english
                .entry(code.clone())
                .or_insert_with(|| (word.clone(), Decay::from_parts(0, 0)));
            let merged = Decay::from_parts(
                slot.1.count.max(entry.count),
                slot.1.last_used.max(entry.last_used),
            );
            if merged != slot.1 {
                slot.1 = merged;
                stats.english += 1;
            }
        }

        // 个人敲错表：嵌套键，次数/日期取 max。
        for (typed, intended) in &peer.typos {
            let slot = self.typos.entry(typed.clone()).or_default();
            for (syllable, entry) in intended {
                let value = slot
                    .entry(syllable.clone())
                    .or_insert_with(|| Decay::from_parts(0, 0));
                let merged = Decay::from_parts(
                    value.count.max(entry.count),
                    value.last_used.max(entry.last_used),
                );
                if merged != *value {
                    *value = merged;
                    stats.typos += 1;
                }
            }
        }

        // 个人 n-gram：字段在 core crate 里是私有的，用现成的 to_tsv / parse_lenient
        // 在文本层按键取 max 后重建。二元三列、三元四列，键 = 除次数外的所有列。
        let mut rows: BTreeMap<String, u32> = BTreeMap::new();
        for line in self.ngram.to_tsv().lines() {
            if let Some((key, count)) = split_counted_row(line) {
                rows.insert(key, count);
            }
        }
        for line in peer.ngram.to_tsv().lines() {
            let Some((key, count)) = split_counted_row(line) else {
                continue;
            };
            let slot = rows.entry(key).or_insert(0);
            if count > *slot {
                *slot = count;
                stats.ngram += 1;
            }
        }
        if stats.ngram > 0 {
            let mut text = String::new();
            for (key, count) in &rows {
                text.push_str(key);
                text.push('\t');
                text.push_str(&count.to_string());
                text.push('\n');
            }
            let (merged, _) = UserNgram::parse_lenient(&text);
            self.ngram = merged;
        }

        if !stats.is_empty() {
            // 重建派生索引：blocked 变了会过滤掉用户词，english 变了要重排词表。
            self.rebuild_words();
            self.rebuild_english();
            // 合并结果需要写回活动目录。宁可多写一次全量，也不漏写；这些标志会被
            // 下一次 flush 清掉，不影响正确性（幂等）。
            self.dirty = true;
            self.words_dirty = true;
            self.ngram_dirty = true;
            self.choices_dirty = true;
            self.english_dirty = true;
            self.typos_dirty = true;
        }
        Ok(stats)
    }

    /// 把当前内存状态整组导出到 `dir`，做成一份可被对端合并的快照。
    ///
    /// 复用现有 `save_*_to` 保证表头 / 列序 / 编码与青简自身写出完全一致；
    /// **不看 dirty**，无条件写全套。导出前后保存并恢复 dirty 标志与 `self.path`，
    /// 这样导出不会「吃掉」本该写回活动目录的那次 flush。
    pub fn export_snapshot_impl(&mut self, dir: &Path) -> Result<(), LearningError> {
        std::fs::create_dir_all(dir)?;
        let saved = (
            self.dirty,
            self.words_dirty,
            self.english_dirty,
            self.ngram_dirty,
            self.choices_dirty,
            self.typos_dirty,
        );
        let base = dir.join(SNAPSHOT_USER_FILE);
        let result = (|| -> Result<(), LearningError> {
            self.save_to(&base)?;
            self.save_words_to(&Self::words_path(&base))?;
            self.save_ngram_to(&Self::ngram_path(&base))?;
            self.save_choices_to(&Self::choices_path(&base))?;
            self.save_english_to(&Self::english_path(&base))?;
            self.save_typos_to(&Self::typos_path(&base))?;
            self.save_blocked_to(&Self::blocked_path(&base))?;
            Ok(())
        })();
        (
            self.dirty,
            self.words_dirty,
            self.english_dirty,
            self.ngram_dirty,
            self.choices_dirty,
            self.typos_dirty,
        ) = saved;
        result
    }

    /// 屏蔽表落盘。learning crate 里原本只有 `load_blocked`，没有写口（第 1 批 Q-C1）。
    /// 表头逐字照 `apps/settings/src/terms/blocked.rs:10`，`BTreeSet` 迭代即字典序。
    pub(super) fn save_blocked_to(&self, path: &Path) -> Result<(), LearningError> {
        write_atomic(path, |file| {
            writeln!(
                file,
                "# 青简屏蔽表：一行一个词。术语页删掉的词写在这里，自动学习不再加回来"
            )?;
            for word in &self.blocked {
                writeln!(file, "{word}")?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(text: &str) -> Candidate {
        Candidate {
            text: text.to_owned(),
            kind: qingjian_core::CandidateKind::Chinese,
            syllables: Vec::new(),
            reading: None,
            translation: None,
            aux_code: None,
            settled: true,
        }
    }

    /// 用当天的 unix 天造数据，避免断言依赖硬编码日期（衰减会按天折算）。
    fn today() -> u64 {
        decay::today()
    }

    /// 每次测试用独立目录，避免并行跑时互相撞。
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("qingjian-merge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, lines: &[String]) {
        std::fs::create_dir_all(dir).unwrap();
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(dir.join(name), text).unwrap();
    }

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 造一份 9 张表齐备、侧车键集合一致的快照。
    fn snapshot(dir: &Path, user: &[String], days: &[String]) {
        write(dir, SNAPSHOT_USER_FILE, user);
        write(dir, USER_DAYS_FILE, days);
        write(dir, USER_WORDS_FILE, &lines(&["# H"]));
        write(dir, USER_NGRAM_FILE, &lines(&["# H"]));
        write(dir, USER_CHOICES_FILE, &lines(&["# H"]));
        write(dir, USER_ENGLISH_FILE, &lines(&["# H"]));
        write(dir, USER_ENGLISH_DAYS_FILE, &lines(&["# H"]));
        write(dir, USER_TYPOS_FILE, &lines(&["# H"]));
        write(dir, USER_TYPOS_DAYS_FILE, &lines(&["# H"]));
    }

    #[test]
    fn empty_peer_is_noop_and_does_not_delete() {
        let dir = scratch("empty-peer");
        snapshot(&dir, &lines(&["# H"]), &lines(&["# D"]));
        let mut learner = FrequencyLearner::parse("的\t10\n了\t4\n");
        let before = learner.len();
        let stats = learner.merge_snapshot_impl(&dir).unwrap();
        assert!(stats.is_empty(), "{stats:?}");
        assert_eq!(learner.len(), before, "空快照不能把本机词条判成删除");
        assert_eq!(learner.weight("的"), 10);
    }

    #[test]
    fn empty_local_takes_peer() {
        let dir = scratch("empty-local");
        let day = today().to_string();
        snapshot(
            &dir,
            &lines(&["# H", "对端词\t7"]),
            &lines(&["# D", &format!("对端词\t{day}")]),
        );
        let mut learner = FrequencyLearner::default();
        let stats = learner.merge_snapshot_impl(&dir).unwrap();
        assert_eq!(stats.counts, 1, "{stats:?}");
        assert_eq!(learner.weight("对端词"), 7);
    }

    #[test]
    fn count_takes_max_and_day_takes_newer() {
        let dir = scratch("max");
        let day = today().to_string();
        snapshot(
            &dir,
            &lines(&["# H", "的\t9999", "新词\t7"]),
            &lines(&["# D", &format!("的\t{day}"), &format!("新词\t{day}")]),
        );
        let mut learner = FrequencyLearner::parse("的\t1141\n了\t607\n");
        let stats = learner.merge_snapshot_impl(&dir).unwrap();
        assert_eq!(stats.counts, 2, "{stats:?}");
        assert_eq!(learner.weight("的"), 9999);
        assert_eq!(learner.weight("新词"), 7);
        assert_eq!(learner.weight("了"), 607);
    }

    #[test]
    fn merge_is_idempotent() {
        let dir = scratch("idempotent");
        let day = today().to_string();
        snapshot(
            &dir,
            &lines(&["# H", "的\t9999", "新词\t7"]),
            &lines(&["# D", &format!("的\t{day}"), &format!("新词\t{day}")]),
        );
        let mut learner = FrequencyLearner::parse("的\t1141\n");
        learner.merge_snapshot_impl(&dir).unwrap();
        let before: Vec<u32> = ["的", "新词"].iter().map(|w| learner.weight(w)).collect();
        let second = learner.merge_snapshot_impl(&dir).unwrap();
        assert!(second.is_empty(), "重复合并不得叠加：{second:?}");
        let after: Vec<u32> = ["的", "新词"].iter().map(|w| learner.weight(w)).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn missing_sidecar_is_rejected() {
        let dir = scratch("missing-sidecar");
        let day = today().to_string();
        snapshot(
            &dir,
            &lines(&["# H", "的\t3"]),
            &lines(&["# D", &format!("的\t{day}")]),
        );
        std::fs::remove_file(dir.join(USER_DAYS_FILE)).unwrap();
        let mut learner = FrequencyLearner::default();
        let err = learner.merge_snapshot_impl(&dir).unwrap_err();
        assert!(err.to_string().contains("缺文件"), "{err}");
        assert_eq!(learner.len(), 0, "拒绝后不得留下半合并状态");
    }

    #[test]
    fn sidecar_key_mismatch_is_rejected() {
        let dir = scratch("key-mismatch");
        let day = today().to_string();
        snapshot(
            &dir,
            &lines(&["# H", "甲\t3", "乙\t4"]),
            &lines(&["# D", &format!("甲\t{day}")]),
        );
        let mut learner = FrequencyLearner::default();
        let err = learner.merge_snapshot_impl(&dir).unwrap_err();
        assert!(err.to_string().contains("键集合"), "{err}");
    }

    #[test]
    fn english_case_normalized_and_local_spelling_kept() {
        let dir = scratch("english");
        snapshot(&dir, &lines(&["# H"]), &lines(&["# D"]));
        let day = today().to_string();
        write(&dir, USER_ENGLISH_FILE, &lines(&["# E", "hello\t2"]));
        write(
            &dir,
            USER_ENGLISH_DAYS_FILE,
            &lines(&["# ED", &format!("hello\t{day}")]),
        );
        let mut learner = FrequencyLearner::parse("");
        learner.load_english("Hello\t5\n", today().saturating_sub(1));
        learner.load_english_days(&format!("hello\t{}\n", today().saturating_sub(1)));
        let stats = learner.merge_snapshot_impl(&dir).unwrap();
        assert_eq!(
            stats.english, 1,
            "对端日期更新，应更新 last_used：{stats:?}"
        );
        assert!(learner.english.contains_key("hello"));
        assert!(!learner.english.contains_key("Hello"), "键必须是小写编码");
        assert_eq!(learner.english["hello"].0, "Hello", "保留本机首次写法");
        assert_eq!(learner.english["hello"].1.count, 5, "次数取 max");
    }

    #[test]
    fn raw_mark_is_plain_text_and_blocked_is_union() {
        let dir = scratch("raw-blocked");
        snapshot(&dir, &lines(&["# H"]), &lines(&["# D"]));
        write(
            &dir,
            USER_CHOICES_FILE,
            &lines(&["# C", "a\t<raw>\t3", "b\t词\t9"]),
        );
        write(&dir, BLOCKED_FILE, &lines(&["# B", "对端屏蔽"]));
        let mut learner = FrequencyLearner::default();
        learner.blocked.insert("本机屏蔽".to_owned());
        let stats = learner.merge_snapshot_impl(&dir).unwrap();
        assert_eq!(stats.choices, 2, "{stats:?}");
        assert_eq!(stats.blocked, 1, "{stats:?}");
        assert_eq!(learner.choice_weight("a", "<raw>"), 3);
        assert!(learner.is_blocked("对端屏蔽"));
        assert!(learner.is_blocked("本机屏蔽"));
    }

    #[test]
    fn export_writes_all_ten_files_and_sidecars_match() {
        let dir = scratch("export");
        let mut learner = FrequencyLearner::parse("的\t10\n了\t4\n");
        learner
            .words
            .insert("用户词".to_owned(), "yong hu ci".to_owned());
        learner.blocked.insert("屏蔽词".to_owned());
        learner.export_snapshot_impl(&dir).unwrap();
        for name in SNAPSHOT_FILES {
            assert!(dir.join(name).is_file(), "缺导出文件 {name}");
        }
        let mut check = FrequencyLearner::default();
        check.merge_snapshot_impl(&dir).unwrap();
        assert_eq!(check.len(), learner.len());
        assert_eq!(check.word_count(), learner.word_count());
        assert_eq!(check.blocked_len(), learner.blocked_len());
    }

    #[test]
    fn export_does_not_change_path_or_dirty() {
        let dir = scratch("path-dirty");
        let active = dir.join("active");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::write(active.join("user.tsv"), "# H\n的\t10\n").unwrap();
        let mut learner = FrequencyLearner::from_path(active.join("user.tsv")).unwrap();
        learner.record(&candidate("新词"));
        assert!(learner.has_unsaved());
        let snap = dir.join("snap");
        learner.export_snapshot_impl(&snap).unwrap();
        assert_eq!(
            learner.path.as_deref(),
            Some(active.join("user.tsv").as_path())
        );
        assert!(
            learner.has_unsaved(),
            "导出不得吃掉待写回活动目录的 dirty 标志"
        );
    }

    #[test]
    fn merge_preserves_active_directory_path() {
        let dir = scratch("merge-path");
        let active = dir.join("active");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::write(active.join("user.tsv"), "# H\n的\t10\n").unwrap();
        let snap = dir.join("snap");
        let day = today().to_string();
        snapshot(
            &snap,
            &lines(&["# H", "新词\t7"]),
            &lines(&["# D", &format!("新词\t{day}")]),
        );
        let mut learner = FrequencyLearner::from_path(active.join("user.tsv")).unwrap();
        learner.merge_snapshot_impl(&snap).unwrap();
        assert_eq!(
            learner.path.as_deref(),
            Some(active.join("user.tsv").as_path())
        );
        assert_eq!(learner.weight("新词"), 7);
    }
}
