//! 术语编辑卡片：打开、关闭、整表存回 `user-terms.tsv`。

use bamboo_slip::TextEdit;
use qingjian_platform::terms::Term;

use super::Settings;
use crate::message::FieldId;
use crate::terms::{Lang, TermEditor, path, save};

/// 某个语种对应卡片里的哪个输入框。
fn field_of(lang: Lang) -> FieldId {
    match lang {
        Lang::En => FieldId::TermEnglish,
        Lang::Ja => FieldId::TermJapanese,
        Lang::Es => FieldId::TermSpanish,
        Lang::Fr => FieldId::TermFrench,
        Lang::Ko => FieldId::TermKorean,
    }
}

/// 卡片里那八个输入框。
const FIELDS: [FieldId; 8] = [
    FieldId::TermWord,
    FieldId::TermReading,
    FieldId::TermTag,
    FieldId::TermEnglish,
    FieldId::TermJapanese,
    FieldId::TermSpanish,
    FieldId::TermFrench,
    FieldId::TermKorean,
];

impl Settings {
    pub(super) fn open_term(&mut self, index: Option<usize>) {
        let entries = self
            .terms
            .as_ref()
            .map(|list| list.entries.as_slice())
            .unwrap_or(&[]);
        let (editor, term) = match index.and_then(|i| entries.get(i).map(|term| (i, term))) {
            Some((i, term)) => (TermEditor::edit(i, term), term.clone()),
            None => (TermEditor::new(), Term::default()),
        };
        for (id, value) in [
            (FieldId::TermWord, term.word),
            (FieldId::TermReading, term.reading),
            (FieldId::TermTag, term.tag),
            (FieldId::TermEnglish, term.english),
            (FieldId::TermJapanese, term.japanese),
            (FieldId::TermSpanish, term.spanish),
            (FieldId::TermFrench, term.french),
            (FieldId::TermKorean, term.korean),
        ] {
            self.fields.insert(id, TextEdit::new(value));
        }
        self.term = Some(editor);
    }

    pub(super) fn close_term(&mut self) {
        self.term = None;
        for id in FIELDS {
            self.fields.remove(&id);
        }
    }

    /// 存卡片（`delete` 时删掉这条）：整表写回文件，失败把原因留在卡片里。
    pub(super) fn save_term(&mut self, delete: bool) {
        let Some(editor) = self.term.clone() else {
            return;
        };
        let field = |id| {
            self.fields
                .get(&id)
                .map(|edit: &TextEdit| edit.text().trim().to_owned())
                .unwrap_or_default()
        };
        let Some(list) = self.terms.as_ref() else {
            return;
        };
        let original = list.entries.clone();
        let term = (!delete).then(|| {
            let mut term = Term {
                word: field(FieldId::TermWord),
                reading: field(FieldId::TermReading),
                tag: field(FieldId::TermTag),
                in_candidates: editor.in_candidates,
                ..Term::default()
            };
            for lang in Lang::ALL {
                lang.set(&mut term, field(field_of(lang)));
            }
            // 原有行的其他列（其他译法 / 备注 / 来源 / 日期）保住
            if let Some(old) = editor.index.and_then(|i| original.get(i)) {
                term.other = old.other.clone();
                term.note = old.note.clone();
                term.source = old.source.clone();
                term.date = old.date.clone();
            }
            term
        });
        let removed = delete
            .then(|| {
                editor
                    .index
                    .and_then(|i| original.get(i))
                    .map(|term| term.word.clone())
            })
            .flatten();
        let added = term.as_ref().map(|term| term.word.clone());
        let saved = editor.apply(&original, term.clone()).and_then(|entries| {
            let path = path().ok_or_else(|| "找不到用户数据目录".to_owned())?;
            save(&path, &entries)?;
            Ok(entries)
        });
        match saved {
            Ok(entries) => {
                // 删掉的词进屏蔽表，重加的词从屏蔽表拿走：学习层下次读盘生效
                if let Some(word) = removed
                    && let Err(error) = crate::terms::blocked::add(&word)
                {
                    tracing::warn!(%error, "写屏蔽表失败");
                }
                if let Some(word) = added
                    && let Err(error) = crate::terms::blocked::remove(&word)
                {
                    tracing::warn!(%error, "清屏蔽表失败");
                }
                let blocked = crate::terms::blocked::words().len();
                if let Some(list) = self.terms.as_mut() {
                    list.entries = entries;
                    list.blocked = blocked;
                    list.error = None;
                }
                self.close_term();
            }
            Err(error) => {
                if let Some(editor) = self.term.as_mut() {
                    editor.error = Some(error);
                    editor.confirm_delete = false;
                }
            }
        }
    }
}
