//! 术语 → 个人释义表：术语表里改了译名，候选窗要跟着变，所以按术语表现算出一份 `user-glossary-<语言>.tsv`。
//!
//! 候选窗的注解走 `LayeredTranslator`：个人释义表优先于随包表。以前这份个人表只有云端兜底写、
//! 术语的英文靠手工导入，改了术语译名注解还是旧的；现在术语是唯一权威，每次写术语表就重算这一份。

use std::path::Path;

use qingjian_platform::terms::Term;

/// 一门语言的个人释义表取术语的哪一列。
#[derive(Debug, Clone, Copy)]
enum Column {
    English,
    Japanese,
    Spanish,
    French,
    Korean,
}

impl Column {
    /// 每门语言：（文件后缀, 取哪一列）。
    const ALL: [(&'static str, Self); 5] = [
        ("en", Self::English),
        ("ja", Self::Japanese),
        ("es", Self::Spanish),
        ("fr", Self::French),
        ("ko", Self::Korean),
    ];

    fn value(self, term: &Term) -> &str {
        match self {
            Self::English => &term.english,
            Self::Japanese => &term.japanese,
            Self::Spanish => &term.spanish,
            Self::French => &term.french,
            Self::Korean => &term.korean,
        }
    }
}

/// 按术语表重写各语言的个人释义表；`terms_path` 是术语表文件，产物与它同目录。
/// 返回（写了几门语言, 共几条）。
pub fn sync(terms_path: &Path, entries: &[Term]) -> (usize, usize) {
    let mut languages = 0;
    let mut total = 0;
    for (code, column) in Column::ALL {
        let lines: Vec<(String, String)> = entries
            .iter()
            .filter(|term| !term.word.trim().is_empty())
            .filter_map(|term| {
                let text = column.value(term).trim();
                (!text.is_empty()).then(|| (term.word.trim().to_owned(), text.to_owned()))
            })
            .collect();
        // 一门语言一条都没有就不写，免得把别的来源攒的表清空
        if lines.is_empty() {
            continue;
        }
        let path = terms_path.with_file_name(format!("user-glossary-{code}.tsv"));
        if let Err(error) = write(&path, code, &lines) {
            tracing::warn!(%error, path = %path.display(), "个人释义表写入失败");
            continue;
        }
        languages += 1;
        total += lines.len();
    }
    (languages, total)
}

fn write(path: &Path, code: &str, lines: &[(String, String)]) -> Result<(), String> {
    let mut text = format!(
        "# 青简个人释义表（{code}）：由术语表生成，改术语请到设置「术语」页，别直接改这里\n"
    );
    let mut sorted = lines.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.dedup_by(|a, b| a.0 == b.0);
    for (word, translation) in sorted {
        text.push_str(&word);
        text.push('\t');
        text.push_str(&translation);
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

    #[test]
    fn rewrites_every_language_from_the_term_table() {
        let dir = std::env::temp_dir().join(format!("qingjian-gloss-sync-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let terms_path = dir.join("user-terms.tsv");
        let entries = parse(
            "围挡\twei dang\t工程施工\tY\thoarding\t\t\t\t\t囲い\t\t\t\n\
             总包\tzong bao\t工程施工\tY\tmain contractor\n",
        );
        let (languages, total) = sync(&terms_path, &entries);
        assert_eq!(languages, 2, "只有英文与日文有译名");
        assert_eq!(total, 3);

        let english = std::fs::read_to_string(dir.join("user-glossary-en.tsv")).unwrap();
        assert!(english.contains("围挡\thoarding"), "{english}");
        assert!(english.contains("总包\tmain contractor"));
        let japanese = std::fs::read_to_string(dir.join("user-glossary-ja.tsv")).unwrap();
        assert!(japanese.contains("围挡\t囲い"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_language_is_not_written() {
        let dir = std::env::temp_dir().join(format!("qingjian-gloss-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let terms_path = dir.join("user-terms.tsv");
        let entries = parse("围挡\twei dang\t工程施工\tY\thoarding\n");
        let (languages, _) = sync(&terms_path, &entries);
        assert_eq!(languages, 1);
        assert!(!dir.join("user-glossary-fr.tsv").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
