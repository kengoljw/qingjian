//! 术语编辑卡片：打开、关闭、整表存回 `user-terms.tsv`。

use bamboo_slip::TextEdit;
use qingjian_platform::terms::Term;

use super::Settings;
use crate::message::FieldId;
use crate::terms::{Lang, TermEditor, TermPaste, path, save};

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

    /// 给这些词生成译名：结果进待确认表（重复的先清掉同词同语种的旧行）。
    pub(super) fn generate_terms(&mut self, words: Vec<String>) {
        if words.is_empty() {
            self.term_notice = Some("先勾几条要生成译名的词（列表行前的方框）".to_owned());
            return;
        }
        let langs: Vec<crate::terms::pending::Lang> = self
            .gloss_langs
            .iter()
            .filter_map(|code| crate::terms::pending::Lang::from_code(code))
            .collect();
        if langs.is_empty() {
            self.term_notice = Some("先在上面勾一个要生成的语言".to_owned());
            return;
        }
        self.pending.retain(|row| {
            !row.status.eq(&crate::terms::pending::Status::Waiting)
                || !words.iter().any(|word| word == &row.word)
        });
        self.term_notice = Some(format!(
            "正在生成 {} 条译名（{} 个词 × {} 种语言）…",
            words.len() * langs.len(),
            words.len(),
            langs.len()
        ));
        self.gloss.start(&self.config.predict, words, langs);
    }

    /// 后台线程叫醒了：把结果并进待确认表。
    pub(super) fn collect_gloss(&mut self) {
        let outcomes = self.gloss.drain();
        if outcomes.is_empty() {
            return;
        }
        let mut failed = 0;
        for outcome in outcomes {
            match outcome {
                crate::terms::gloss::Outcome::Filled(row) => {
                    self.pending.retain(|existing| {
                        !(existing.word == row.word && existing.lang == row.lang)
                    });
                    self.pending.push(row);
                }
                crate::terms::gloss::Outcome::Failed(reason) => {
                    failed += 1;
                    self.term_notice = Some(reason);
                }
                crate::terms::gloss::Outcome::Done => {}
            }
        }
        if failed == 0 {
            let waiting = self
                .pending
                .iter()
                .filter(|row| row.status == crate::terms::pending::Status::Waiting)
                .count();
            self.term_notice = Some(format!("生成完成，待确认 {waiting} 条"));
        }
        let _ = crate::terms::pending::save(&self.pending);
    }

    /// 确认一条：写进术语表的语种列（英文列除外，英文列是主译名）。
    pub(super) fn accept_pending(&mut self, index: usize) {
        let Some(row) = self.pending.get(index).cloned() else {
            return;
        };
        let Some(list) = self.terms.as_mut() else {
            return;
        };
        let Some(term) = list.entries.iter_mut().find(|term| term.word == row.word) else {
            self.term_notice = Some(format!("术语表里找不到「{}」", row.word));
            return;
        };
        match row.lang {
            crate::terms::pending::Lang::En => term.english = row.text.clone(),
            crate::terms::pending::Lang::Ja => term.japanese = row.text.clone(),
            crate::terms::pending::Lang::Es => term.spanish = row.text.clone(),
            crate::terms::pending::Lang::Fr => term.french = row.text.clone(),
            crate::terms::pending::Lang::Ko => term.korean = row.text.clone(),
        }
        let entries = list.entries.clone();
        if let Some(path) = path()
            && let Err(error) = save(&path, &entries)
        {
            self.term_notice = Some(error);
            return;
        }
        if let Some(row) = self.pending.get_mut(index) {
            row.status = crate::terms::pending::Status::Accepted;
        }
        // 术语是译名的唯一权威：写完术语表就把各语言的个人释义表重算一遍，
        // 否则候选窗注解还是旧的（用户改「围挡」译名不生效就是这个原因）。
        if let Some(path) = path()
            && let Some(list) = self.terms.as_ref()
        {
            let (languages, total) = crate::terms::glossary::sync(&path, &list.entries);
            tracing::info!(languages, total, "已按术语表重写个人释义表");
        }

        let _ = crate::terms::pending::save(&self.pending);
        self.term_notice = Some(format!("「{}」的{}译名已确认", row.word, row.lang.label()));
    }

    pub(super) fn drop_pending(&mut self, index: usize) {
        if index < self.pending.len() {
            self.pending.remove(index);
            let _ = crate::terms::pending::save(&self.pending);
        }
    }

    pub(super) fn toggle_gloss_lang(&mut self, code: &'static str) {
        if let Some(position) = self.gloss_langs.iter().position(|item| *item == code) {
            self.gloss_langs.remove(position);
        } else {
            self.gloss_langs.push(code);
        }
    }

    pub(super) fn open_term_paste(&mut self) {
        self.fields
            .insert(FieldId::TermPaste, TextEdit::multiline(String::new()));
        self.term_paste = Some(TermPaste::default());
    }

    pub(super) fn close_term_paste(&mut self) {
        self.term_paste = None;
        self.fields.remove(&FieldId::TermPaste);
    }

    /// 把粘贴框里的一行一词并进表：同名的跳过，整表写回。
    pub(super) fn import_terms(&mut self) {
        let Some(list) = self.terms.as_ref() else {
            return;
        };
        let text = self
            .fields
            .get(&FieldId::TermPaste)
            .map(|field| field.text().to_owned())
            .unwrap_or_default();
        let incoming = crate::terms::import::parse(&text);
        if incoming.is_empty() {
            if let Some(paste) = self.term_paste.as_mut() {
                paste.error = Some(
                    "一行也没读到：一行一个词，后面可以跟读音 / 标签 / 英文（制表符分隔）"
                        .to_owned(),
                );
                paste.result = None;
            }
            return;
        }
        let (incoming, too_long): (Vec<_>, Vec<_>) = incoming
            .into_iter()
            .partition(|term| !crate::terms::is_too_long(&term.word));
        let imported: Vec<String> = incoming.iter().map(|term| term.word.clone()).collect();
        let mut entries = list.entries.clone();
        let (added, skipped) = crate::terms::import::merge(&mut entries, incoming);
        let saved = path()
            .ok_or_else(|| "找不到用户数据目录".to_owned())
            .and_then(|path| save(&path, &entries));
        match saved {
            Ok(()) => {
                for word in imported {
                    if let Err(error) = crate::terms::blocked::remove(&word) {
                        tracing::warn!(%error, "清屏蔽表失败");
                    }
                }
                let blocked = crate::terms::blocked::words().len();
                if let Some(list) = self.terms.as_mut() {
                    list.entries = entries;
                    list.blocked = blocked;
                    list.error = None;
                }
                // 术语是译名的唯一权威：写完术语表就把各语言的个人释义表重算一遍，
                // 否则候选窗注解还是旧的（用户改「围挡」译名不生效就是这个原因）。
                if let Some(path) = path()
                    && let Some(list) = self.terms.as_ref()
                {
                    let (languages, total) = crate::terms::glossary::sync(&path, &list.entries);
                    tracing::info!(languages, total, "已按术语表重写个人释义表");
                }
                if let Some(paste) = self.term_paste.as_mut() {
                    paste.error = None;
                    paste.result = Some((added, skipped + too_long.len()));
                    if !too_long.is_empty() {
                        paste.error = Some(format!(
                            "有 {} 条超过 {} 个字，按录入错误跳过了",
                            too_long.len(),
                            crate::terms::MAX_TERM_CHARS
                        ));
                    }
                }
            }
            Err(error) => {
                if let Some(paste) = self.term_paste.as_mut() {
                    paste.error = Some(error);
                    paste.result = None;
                }
            }
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
        // 术语不超过 6 个字符：超过的基本都是文档里的句子碎片，录入时就拦下
        if !delete {
            let word = field(FieldId::TermWord);
            if crate::terms::is_too_long(&word) {
                if let Some(editor) = self.term.as_mut() {
                    editor.error = Some(format!(
                        "术语不超过 {} 个字（现在是 {} 个）",
                        crate::terms::MAX_TERM_CHARS,
                        word.chars().count()
                    ));
                }
                return;
            }
        }
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
                // 术语是译名的唯一权威：写完术语表就把各语言的个人释义表重算一遍，
                // 否则候选窗注解还是旧的（用户改「围挡」译名不生效就是这个原因）。
                if let Some(path) = path()
                    && let Some(list) = self.terms.as_ref()
                {
                    let (languages, total) = crate::terms::glossary::sync(&path, &list.entries);
                    tracing::info!(languages, total, "已按术语表重写个人释义表");
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
