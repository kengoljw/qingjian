//! 一行一词粘贴导入：`词 ⇥ 读音 ⇥ 标签 ⇥ 英文`（制表符分隔，后面四列忽略），只有词也行。

use qingjian_platform::terms::Term;

/// 解析粘贴文本：空行与 `#` 注释跳过，词为空的行丢掉。
pub fn parse(text: &str) -> Vec<Term> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t').map(str::trim);
        let Some(word) = fields.next().filter(|word| !word.is_empty()) else {
            continue;
        };
        let reading = fields.next().unwrap_or("").to_owned();
        let tag = fields.next().unwrap_or("").to_owned();
        let english = fields.next().unwrap_or("").to_owned();
        out.push(Term {
            word: word.to_owned(),
            reading,
            tag,
            in_candidates: true,
            english,
            source: "粘贴导入".to_owned(),
            ..Term::default()
        });
    }
    out
}

/// 并进现有表：同名的跳过（保留原来那条），返回（新增条数，跳过条数）。
pub fn merge(existing: &mut Vec<Term>, incoming: Vec<Term>) -> (usize, usize) {
    let mut seen: std::collections::HashSet<String> =
        existing.iter().map(|term| term.word.clone()).collect();
    let mut added = 0;
    let mut skipped = 0;
    for term in incoming {
        if !seen.insert(term.word.clone()) {
            skipped += 1;
            continue;
        }
        existing.push(term);
        added += 1;
    }
    (added, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_words_with_optional_columns() {
        let terms = parse(
            "中间验收\tzhong jian yan shou\t项目管理\tintermediate inspection\n# 注释\n\n只有词\n",
        );
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0].word, "中间验收");
        assert_eq!(terms[0].reading, "zhong jian yan shou");
        assert_eq!(terms[0].tag, "项目管理");
        assert_eq!(terms[0].english, "intermediate inspection");
        assert!(terms[0].in_candidates);
        assert_eq!(terms[1].word, "只有词");
        assert!(terms[1].reading.is_empty());
    }

    #[test]
    fn merge_skips_duplicates_and_keeps_the_old_row() {
        let mut existing = parse("业主\tye zhu\n");
        let (added, skipped) = merge(&mut existing, parse("业主\twrong\n总包\tzong bao\n"));
        assert_eq!((added, skipped), (1, 1));
        assert_eq!(existing.len(), 2);
        assert_eq!(existing[0].reading, "ye zhu");
        assert_eq!(existing[1].word, "总包");
    }
}
