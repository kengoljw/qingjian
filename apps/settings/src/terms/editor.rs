//! 术语的编辑草稿：新增或改一条时的样子（输入框在 `Settings::fields` 里），保存时整表写回 `user-terms.tsv`。

use qingjian_platform::terms::Term;

/// 编辑中的一条。
#[derive(Debug, Clone)]
pub struct TermEditor {
    /// 改的是第几条；`None` 是新增。
    pub index: Option<usize>,

    /// 打开时那一条的样子，保存前比一下有没有被别处改过。
    pub original: Option<Term>,

    /// 是否参与拼音候选。
    pub in_candidates: bool,

    /// 保存不了的原因，显示在卡片里。
    pub error: Option<String>,

    /// 「删除」点过一次，等第二次确认。
    pub confirm_delete: bool,
}

impl TermEditor {
    pub fn new() -> Self {
        Self {
            index: None,
            original: None,
            in_candidates: true,
            error: None,
            confirm_delete: false,
        }
    }

    pub fn edit(index: usize, term: &Term) -> Self {
        Self {
            index: Some(index),
            original: Some(term.clone()),
            in_candidates: term.in_candidates,
            error: None,
            confirm_delete: false,
        }
    }

    /// 在 `entries` 上应用这次编辑（`term` 为 `None` 是删除）；表在打开后被改过就报错。
    pub fn apply(&self, entries: &[Term], term: Option<Term>) -> Result<Vec<Term>, String> {
        let mut list = entries.to_vec();
        match self.index {
            Some(index) => {
                if list.get(index) != self.original.as_ref() {
                    return Err("这条在别处被改过了，没有保存；关掉再打开看看现在的样子".to_owned());
                }
                match term {
                    Some(term) => list[index] = term,
                    None => {
                        list.remove(index);
                    }
                }
            }
            None => {
                if let Some(term) = term {
                    if list.iter().any(|existing| existing.word == term.word) {
                        return Err(format!("「{}」已经在表里了", term.word));
                    }
                    list.push(term);
                }
            }
        }
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_platform::terms::parse;

    fn term(word: &str) -> Term {
        parse(&format!("{word}\tdu yin\t工程施工\tY\n")).remove(0)
    }

    #[test]
    fn new_terms_are_rejected_when_the_word_exists() {
        let entries = vec![term("业主"), term("总包")];
        let editor = TermEditor::new();
        assert!(editor.apply(&entries, Some(term("业主"))).is_err());
        let saved = editor.apply(&entries, Some(term("幕墙"))).unwrap();
        assert_eq!(saved.len(), 3);
    }

    #[test]
    fn edits_refuse_to_overwrite_a_changed_list() {
        let entries = vec![term("业主"), term("总包")];
        let editor = TermEditor::edit(1, &entries[1]);
        assert!(editor.apply(&entries, Some(term("承包商"))).is_ok());
        let changed = vec![term("业主"), term("承包商")];
        assert!(editor.apply(&changed, None).is_err());
    }

    #[test]
    fn delete_removes_the_row() {
        let entries = vec![term("业主"), term("总包")];
        let editor = TermEditor::edit(0, &entries[0]);
        let saved = editor.apply(&entries, None).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].word, "总包");
    }
}
