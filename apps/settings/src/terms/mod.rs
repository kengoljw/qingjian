//! 个人术语表：读写用户目录的 `user-terms.tsv`，给「术语」页展示与维护。
//!
//! 与输入法共用同一份文件：输入法把它转成附加词库参与排序，设置程序这里负责看与改。
//! 写回是整表重写（先备份成 `.bak`），保住十三列格式。

use std::path::{Path, PathBuf};

use qingjian_platform::terms::{self, Term};

use crate::system::paths;

pub mod editor;

pub use editor::TermEditor;

/// 术语表的路径：用户数据目录下的 `user-terms.tsv`。
pub fn path() -> Option<PathBuf> {
    paths::user_dir().map(|dir| dir.join("user-terms.tsv"))
}

/// 术语页上显示的语种：英文是主译名，其余四列是补充。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Ja,
    Es,
    Fr,
    Ko,
}

impl Lang {
    pub const ALL: [Self; 5] = [Self::En, Self::Ja, Self::Es, Self::Fr, Self::Ko];

    pub fn label(self) -> &'static str {
        match self {
            Self::En => "英文",
            Self::Ja => "日语",
            Self::Es => "西班牙语",
            Self::Fr => "法语",
            Self::Ko => "韩语",
        }
    }

    pub fn value(self, term: &Term) -> &str {
        match self {
            Self::En => &term.english,
            Self::Ja => &term.japanese,
            Self::Es => &term.spanish,
            Self::Fr => &term.french,
            Self::Ko => &term.korean,
        }
    }

    pub fn set(self, term: &mut Term, value: String) {
        match self {
            Self::En => term.english = value,
            Self::Ja => term.japanese = value,
            Self::Es => term.spanish = value,
            Self::Fr => term.french = value,
            Self::Ko => term.korean = value,
        }
    }
}

/// 读出来的一份术语表。
pub struct Terms {
    pub entries: Vec<Term>,

    /// 读不出来时的原因（文件不在 / 没有可用条目），页上原样显示。
    pub error: Option<String>,
}

impl Terms {
    pub fn load() -> Self {
        let Some(path) = path() else {
            return Self {
                entries: Vec::new(),
                error: Some("找不到用户数据目录".to_owned()),
            };
        };
        if !path.is_file() {
            return Self {
                entries: Vec::new(),
                error: Some(format!("{} 还不存在", path.display())),
            };
        }
        let entries = terms::load(&path);
        let error = entries
            .is_empty()
            .then(|| format!("{} 里没有可用条目", path.display()));
        Self { entries, error }
    }

    /// 参与拼音候选的条数：要勾了候选、还要有读音。
    pub fn usable(&self) -> usize {
        self.entries
            .iter()
            .filter(|term| term.in_candidates && !term.reading.is_empty())
            .count()
    }

    /// 标签与各自的条数，按标签第一次出现的顺序。
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for term in &self.entries {
            let tag = tag_of(term);
            match out.iter_mut().find(|(name, _)| name == tag) {
                Some((_, count)) => *count += 1,
                None => out.push((tag.to_owned(), 1)),
            }
        }
        out
    }

    /// 按标签筛过的条目；`None` 是全部（页上用带下标的遍历，这里留给测试与后续搜索）。
    #[allow(dead_code)]
    pub fn matching(&self, tag: Option<&str>) -> impl Iterator<Item = &Term> {
        self.entries.iter().filter(move |term| match tag {
            None => true,
            Some(tag) => tag_of(term) == tag,
        })
    }
}

/// 没打标签的归到「未分类」。
pub fn tag_of(term: &Term) -> &str {
    if term.tag.is_empty() {
        "未分类"
    } else {
        &term.tag
    }
}

/// 整表写回：先备份成 `.bak`，写临时文件再替换，写坏了不会把原文件弄丢。
pub fn save(path: &Path, entries: &[Term]) -> Result<(), String> {
    if path.exists() {
        let backup = path.with_extension("tsv.bak");
        std::fs::copy(path, &backup)
            .map_err(|error| format!("备份 {} 失败：{error}", backup.display()))?;
    }
    let mut text = String::from(
        "词\t读音\t标签\t进拼音候选\t英文\t其他译法\t备注\t来源\t录入日期\t日文\t西文\t法文\t韩文\n",
    );
    for term in entries {
        text.push_str(&term.line());
        text.push('\n');
    }
    let temp = path.with_extension("tsv.tmp");
    std::fs::write(&temp, text).map_err(|error| format!("写 {} 失败：{error}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|error| format!("替换 {} 失败：{error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_platform::terms::parse;

    fn list(text: &str) -> Terms {
        Terms {
            entries: parse(text),
            error: None,
        }
    }

    #[test]
    fn tags_count_in_first_seen_order() {
        let terms =
            list("业主\tye zhu\t工程施工\tY\n总包\tzong bao\t工程施工\tY\nBIM\tbi em\t机电\tN\n");
        assert_eq!(
            terms.tags(),
            vec![("工程施工".to_owned(), 2), ("机电".to_owned(), 1)]
        );
    }

    #[test]
    fn empty_tag_reads_as_uncategorised() {
        let terms = list("没标签\tmei biao qian\n");
        assert_eq!(terms.tags(), vec![("未分类".to_owned(), 1)]);
        assert_eq!(terms.matching(Some("未分类")).count(), 1);
    }

    #[test]
    fn usable_needs_candidate_flag_and_reading() {
        let terms =
            list("业主\tye zhu\t工程施工\tY\n没读音\t\t工程施工\tY\n不进\tbu jin\t商务沟通\tN\n");
        assert_eq!(terms.usable(), 1);
    }

    #[test]
    fn languages_read_and_write_their_own_column() {
        let mut entries = parse("业主\tye zhu\t工程施工\tY\tlandlord\t\t\t\t\t大家さん\n");
        assert_eq!(Lang::En.value(&entries[0]), "landlord");
        assert_eq!(Lang::Ja.value(&entries[0]), "大家さん");
        assert_eq!(Lang::Ko.value(&entries[0]), "");
        Lang::Ko.set(&mut entries[0], "임대인".to_owned());
        assert_eq!(entries[0].korean, "임대인");
        assert!(entries[0].line().ends_with("\t임대인"));
    }

    #[test]
    fn save_roundtrips_through_the_file() {
        let dir = std::env::temp_dir().join(format!("qingjian-terms-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user-terms.tsv");
        std::fs::write(&path, "业主\tye zhu\t工程施工\tY\tlandlord\n").unwrap();
        let mut entries = terms::load(&path);
        entries[0].japanese = "大家さん".to_owned();
        save(&path, &entries).unwrap();
        let reloaded = terms::load(&path);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].japanese, "大家さん");
        assert!(path.with_extension("tsv.bak").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
