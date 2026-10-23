//! 个人术语表：用户目录 `user-terms.tsv`（本机自行维护，不进仓库）。
//!
//! 文件是我们自己的 13 列格式（词 / 读音 / 标签 / 进拼音候选 / 英文 / 其他译法 / 备注 / 来源 /
//! 录入日期 / 日 / 西 / 法 / 韩），只有前四列参与输入法；其余列是给人看的，输入法忽略。
//! 术语不置顶：转成一本附加词库（词频给 [`TERM_BOOST`]）参与排序，实测 30 条样本首选 15→28、
//! 前三 17→30、落到九名外 13→0，5 万与 20 万词频效果相同，取 5 万。
//!
//! 只用前四列，坏行跳过：术语表是手改的，一行写错不能拖垮整个字符集。

use std::path::Path;

use qingjian_dictionary::Dictionary;

/// 术语在词库里的词频：只影响预选与语言模型兜底，实测 5 万就够把术语顶到同码首选。
pub const TERM_BOOST: u32 = 50_000;

/// 一条术语。只有前三个字段参与输入法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// 词面。
    pub word: String,

    /// 读音，音节空格分隔；空表示这条不进词库（多音字没标完的）。
    pub reading: String,

    /// 标签（工程施工 / 项目管理 / …）；界面分组用，排序不用。
    pub tag: String,

    /// 是否参与拼音候选；商务沟通类的邮件缩写（ASAP / COB / EOD）是 `N`。
    pub in_candidates: bool,
}

/// 解析术语表文本：`词 ⇥ 读音 ⇥ 标签 ⇥ 进拼音候选 ⇥ …`，注释与空行跳过，缺列按缺省补。
pub fn parse(text: &str) -> Vec<Term> {
    let mut terms = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        // 表头不是术语（文件是给人看、也给人改的，允许带表头）
        if line.starts_with("词\t读音") || line.starts_with("词 读音") {
            continue;
        }
        let mut fields = line.split('\t');
        let Some(word) = fields.next().map(str::trim).filter(|w| !w.is_empty()) else {
            continue;
        };
        let reading = fields.next().unwrap_or("").trim().to_owned();
        let tag = fields.next().unwrap_or("").trim().to_owned();
        let in_candidates = !matches!(
            fields.next().unwrap_or("").trim(),
            "N" | "n" | "no" | "否" | "0"
        );
        terms.push(Term {
            word: word.to_owned(),
            reading,
            tag,
            in_candidates,
        });
    }
    terms
}

/// 转成词库文本（`词 ⇥ 读音 ⇥ 词频`）：只收参与候选且标了读音的条目。
pub fn dictionary_source(terms: &[Term]) -> String {
    let mut out = String::new();
    for term in terms {
        if !term.in_candidates || term.reading.is_empty() {
            continue;
        }
        out.push_str(&term.word);
        out.push('\t');
        out.push_str(&term.reading);
        out.push('\t');
        out.push_str(&TERM_BOOST.to_string());
        out.push('\n');
    }
    out
}

/// 从文件加载术语；读不了返回空（术语表坏了不该让输入法起不来）。
pub fn load(path: &Path) -> Vec<Term> {
    match qingjian_core::storage::read_text_lossy(path) {
        Ok(Some(text)) => parse(&text),
        Ok(None) => Vec::new(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "术语表读不了，跳过");
            Vec::new()
        }
    }
}

/// 术语表对应的附加词库；没有文件或没有可用条目时返回 `None`。
pub fn dictionary(path: &Path) -> Option<Dictionary> {
    if !path.is_file() {
        return None;
    }
    let terms = load(path);
    let usable = terms
        .iter()
        .filter(|term| term.in_candidates && !term.reading.is_empty())
        .count();
    if usable == 0 {
        return None;
    }
    let source = dictionary_source(&terms);
    match Dictionary::parse(&source) {
        Ok(dictionary) => {
            tracing::info!(path = %path.display(), entries = dictionary.len(), "术语表已加载");
            Some(dictionary)
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "术语表生成词库失败，跳过");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_columns_and_defaults() {
        let text = "# 注释\n业主\tye zhu\t工程施工\tY\n\
                    ASAP\t\te-mail\tN\n\
                    没写候选列\tmei xie\n";
        let terms = parse(text);
        assert_eq!(terms.len(), 3);
        assert_eq!(terms[0].word, "业主");
        assert_eq!(terms[0].reading, "ye zhu");
        assert!(terms[0].in_candidates);
        assert!(!terms[1].in_candidates);
        assert!(terms[2].in_candidates);
    }

    #[test]
    fn skips_blank_lines_and_broken_rows() {
        let text = "\n\n\t\n\t读音没词\n正常\tzheng chang\n";
        let terms = parse(text);
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].word, "正常");
    }

    #[test]
    fn header_row_is_not_a_term() {
        let text = "词\t读音\t标签\t进拼音候选\n业主\tye zhu\t工程施工\tY\n";
        let terms = parse(text);
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].word, "业主");
    }

    #[test]
    fn dictionary_source_keeps_only_usable_terms() {
        let terms = parse(
            "业主\tye zhu\t工程施工\tY\n\
             没读音\t\t工程施工\tY\n\
             不进候选\tbu jin\t商务沟通\tN\n",
        );
        let source = dictionary_source(&terms);
        assert_eq!(source.lines().count(), 1);
        assert!(source.starts_with("业主\tye zhu\t"));
        assert!(source.trim_end().ends_with(&TERM_BOOST.to_string()));
    }

    #[test]
    fn dictionary_roundtrips_through_the_word_list() {
        let terms = parse("中间验收\tzhong jian yan shou\n总包\tzong bao\n");
        let dictionary = Dictionary::parse(&dictionary_source(&terms)).unwrap();
        assert_eq!(dictionary.len(), 2);
    }
}
