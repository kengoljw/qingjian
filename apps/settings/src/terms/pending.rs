//! 术语译名的待确认表：`user-terms-pending.tsv`，与术语表同目录。
//!
//! 一行一条：`词 ⇥ 语种 ⇥ 译名 ⇥ 状态`，状态是 `待确认` / `已确认` / `失败`。
//! 设置程序在这里生成、确认或丢弃；确认后写进术语表对应的语种列（英文列除外，英文列是主译名）。

use std::path::{Path, PathBuf};

/// 待确认表的语种标记，与术语表的列对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
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

    /// 文件里用的标记。
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ja => "ja",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::Ko => "ko",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|lang| lang.code().eq_ignore_ascii_case(code.trim()))
    }

    /// 生成时用的「学习语言」；法 / 韩还没进 `Language`，生成时先退回英语（等语言落地再换）。
    pub fn core(self) -> qingjian_core::Language {
        match self {
            Self::En | Self::Fr | Self::Ko => qingjian_core::Language::English,
            Self::Ja => qingjian_core::Language::Japanese,
            Self::Es => qingjian_core::Language::Spanish,
        }
    }
}

/// 一条待确认的译名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub word: String,
    pub lang: Lang,
    pub text: String,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 生成好了，等人确认。
    Waiting,
    /// 确认过，已经写进术语表。
    Accepted,
    /// 生成失败（网络、模型回复不合格式）。
    Failed,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "待确认",
            Self::Accepted => "已确认",
            Self::Failed => "失败",
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Waiting => "待确认",
            Self::Accepted => "已确认",
            Self::Failed => "失败",
        }
    }

    fn from_code(code: &str) -> Self {
        match code.trim() {
            "已确认" => Self::Accepted,
            "失败" => Self::Failed,
            _ => Self::Waiting,
        }
    }
}

/// 待确认表与术语表同目录。
pub fn path() -> Option<PathBuf> {
    crate::terms::path().map(|path| path.with_file_name("user-terms-pending.tsv"))
}

pub fn load() -> Vec<Pending> {
    path().map(|path| load_at(&path)).unwrap_or_default()
}

pub fn save(rows: &[Pending]) -> Result<(), String> {
    match path() {
        Some(path) => save_at(&path, rows),
        None => Err("找不到用户数据目录".to_owned()),
    }
}

pub fn load_at(path: &Path) -> Vec<Pending> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    source
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let word = fields.next()?.trim();
            let lang = Lang::from_code(fields.next()?)?;
            let text = fields.next().unwrap_or("").trim().to_owned();
            let status = Status::from_code(fields.next().unwrap_or(""));
            (!word.is_empty()).then(|| Pending {
                word: word.to_owned(),
                lang,
                text,
                status,
            })
        })
        .collect()
}

pub fn save_at(path: &Path, rows: &[Pending]) -> Result<(), String> {
    let mut text = String::from("# 青简术语译名待确认表：词\t语种\t译名\t状态\n");
    for row in rows {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            row.word,
            row.lang.code(),
            row.text,
            row.status.code()
        ));
    }
    let temp = path.with_extension("tsv.tmp");
    std::fs::write(&temp, text).map_err(|error| format!("写 {} 失败：{error}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|error| format!("替换 {} 失败：{error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("qingjian-pending-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("user-terms-pending.tsv")
    }

    #[test]
    fn round_trips_through_the_file() {
        let path = temp("roundtrip");
        let rows = vec![
            Pending {
                word: "业主".to_owned(),
                lang: Lang::Ja,
                text: "大家さん".to_owned(),
                status: Status::Waiting,
            },
            Pending {
                word: "总包".to_owned(),
                lang: Lang::Ko,
                text: "".to_owned(),
                status: Status::Failed,
            },
        ];
        save_at(&path, &rows).unwrap();
        assert_eq!(load_at(&path), rows);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn skips_broken_lines_and_unknown_languages() {
        let path = temp("broken");
        std::fs::write(
            &path,
            "# 注释\n缺列\n业主\txx\tlandlord\t待确认\n业主\tja\t大家さん\t待确认\n",
        )
        .unwrap();
        let rows = load_at(&path);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].lang, Lang::Ja);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
