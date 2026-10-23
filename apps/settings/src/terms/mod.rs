//! 个人术语表：读用户目录的 `user-terms.tsv`，给「术语」页展示。只读，编辑走文件。
//!
//! 术语表是输入法与设置程序共用的本机数据：输入法把它转成附加词库参与排序，
//! 设置程序这里只是把同一份文件列出来看。

use std::path::PathBuf;

use qingjian_platform::terms::{self, Term};

use crate::system::paths;

/// 术语表的路径：用户数据目录下的 `user-terms.tsv`。
pub fn path() -> Option<PathBuf> {
    paths::user_dir().map(|dir| dir.join("user-terms.tsv"))
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
            let tag = if term.tag.is_empty() {
                "未分类"
            } else {
                &term.tag
            };
            match out.iter_mut().find(|(name, _)| name == tag) {
                Some((_, count)) => *count += 1,
                None => out.push((tag.to_owned(), 1)),
            }
        }
        out
    }

    /// 按标签筛过的条目；`None` 是全部。
    pub fn matching(&self, tag: Option<&str>) -> impl Iterator<Item = &Term> {
        self.entries.iter().filter(move |term| match tag {
            None => true,
            Some(tag) => {
                let own = if term.tag.is_empty() {
                    "未分类"
                } else {
                    &term.tag
                };
                own == tag
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Terms;
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
}
