//! 「术语」页：个人术语表的查看与维护——按标签筛、按语种看译名、逐条编辑、新增与删除。
//! 写回用户目录的 `user-terms.tsv`（与输入法同一份），写前自动备份。

use bamboo_slip::taffy::{
    AlignItems, Dimension, FlexDirection, FlexWrap, LengthPercentage, LengthPercentageAuto, NodeId,
    Rect, Size, Style,
};
use bamboo_slip::{
    Button, Checkbox, RenderError, ScrollArea, SettingRow, Switch, TextField, Ui, Visual, dialog,
    group, paragraph, section_title,
};

use crate::message::{FieldId, Message};
use crate::terms::{Lang, Terms};
use crate::view::Page;
use crate::view::parts::controls::choice;

/// 列表一次最多画多少条：整表上千条时一帧画完会明显卡，标签 + 搜索 + 这个上限够用了。
const PAGE_SIZE: usize = 60;

/// 对话框宽度（点）。
const DIALOG_WIDTH: f32 = 460.0;

/// 列表页签页头大约占多少点（摘要 + 两行 + 标签），给列表留高度时扣掉它。
const HEADER_POINTS: f32 = 210.0;

pub fn terms(ui: &mut Ui<Message>, page: &Page) -> Result<Vec<NodeId>, RenderError> {
    let theme = &page.theme;
    let mut fixed = Vec::new();
    fixed.push(section_title(ui, theme, "个人术语表")?);

    let Some(list) = page.settings.terms.as_ref() else {
        fixed.push(paragraph(
            ui,
            theme,
            "还没读到术语表。进这一页时会读用户目录里的 user-terms.tsv。",
            None,
        )?);
        return Ok(fixed);
    };

    fixed.push(tabs(ui, page, list)?);
    let body = match page.settings.term_tab {
        crate::terms::TermTab::List => list_tab(ui, page, list)?,
        crate::terms::TermTab::Pending => pending_tab(ui, page)?,
    };
    fixed.push(body);

    if page.settings.term.is_some() {
        editor(ui, page)?;
    }
    if page.settings.term_paste.is_some() {
        paste_dialog(ui, page)?;
    }

    // 固定高度的一列：顶部（标题 + 页签）不动，下面的页签内容自己滚
    let column = ui.scene().node(
        Style {
            flex_direction: FlexDirection::Column,
            gap: page.gap(8.0),
            size: Size {
                width: Dimension::percent(1.0),
                height: Dimension::length(page.content_height),
            },
            ..Style::default()
        },
        Visual::Group,
        &fixed,
    )?;
    Ok(vec![column])
}

/// 页签：术语列表 / 待确认译名（带条数）。
fn tabs(ui: &mut Ui<Message>, page: &Page, list: &Terms) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let waiting = page
        .settings
        .pending
        .iter()
        .filter(|row| row.status == crate::terms::pending::Status::Waiting)
        .count();
    let mut buttons = Vec::new();
    for (tab, code, label) in [
        (
            crate::terms::TermTab::List,
            "list",
            format!("术语列表 {}", list.entries.len()),
        ),
        (
            crate::terms::TermTab::Pending,
            "pending",
            format!("待确认 {waiting}"),
        ),
    ] {
        let selected = page.settings.term_tab == tab;
        let mut button = Button::new(&label);
        if selected {
            button = button.primary();
        }
        buttons.push(button.build(ui, theme, format!("term-tab-{code}"), Message::TermTab(tab))?);
    }
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            align_items: Some(AlignItems::CENTER),
            gap: page.gap(8.0),
            ..Style::default()
        },
        Visual::Group,
        &buttons,
    )
}

/// 术语列表页签：搜索 / 生成 / 新增 / 标签固定，只有列表滚。
/// 列表页签：两行页头固定，列表给确定高度、自己滚。
fn list_tab(ui: &mut Ui<Message>, page: &Page, list: &Terms) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let mut fixed = Vec::new();
    let mut summary = format!(
        "共 {} 条 · 参与拼音候选 {} 条",
        list.entries.len(),
        list.usable()
    );
    if list.blocked > 0 {
        summary.push_str(&format!(" · 屏蔽表 {} 条", list.blocked));
    }
    if !list.marked.is_empty() {
        summary.push_str(&format!(" · 已勾 {} 条", list.marked.len()));
    }
    fixed.push(paragraph(ui, theme, &summary, None)?);
    fixed.push(header(ui, page)?);
    fixed.push(chips(ui, page, list)?);
    if let Some(error) = &list.error {
        fixed.push(paragraph(ui, theme, error, None)?);
    }

    // 页头占的高度是估的，超出部分会被裁掉一点；宁可少给 20 点也别让列表被挤出窗外
    let body_height = (page.content_height - page.px(HEADER_POINTS)).max(page.px(120.0));
    let body = list_body(ui, page, list)?;
    let scroll = ScrollArea::new(Style {
        size: Size {
            width: Dimension::percent(1.0),
            height: Dimension::length(body_height),
        },
        flex_shrink: 0.0,
        min_size: Size {
            width: LengthPercentageAuto::length(0.0),
            height: LengthPercentageAuto::length(0.0),
        },
        ..Style::default()
    })
    .build(ui, theme, ("terms-scroll", "list"), body)?;
    fixed.push(scroll);
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Column,
            gap: page.gap(6.0),
            ..Style::default()
        },
        Visual::Group,
        &fixed,
    )
}

/// 待确认页签：生成按钮、语言勾选与提示固定，待确认列表自己滚。
fn pending_tab(ui: &mut Ui<Message>, page: &Page) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let mut fixed = Vec::new();
    fixed.push(generate_row(ui, page)?);
    if let Some(notice) = &page.settings.term_notice {
        fixed.push(paragraph(ui, theme, notice, None)?);
    }
    let body_height = (page.content_height - page.px(HEADER_POINTS)).max(page.px(120.0));
    let body = pending_section(ui, page)?;
    let scroll = ScrollArea::new(Style {
        size: Size {
            width: Dimension::percent(1.0),
            height: Dimension::length(body_height),
        },
        flex_shrink: 0.0,
        min_size: Size {
            width: LengthPercentageAuto::length(0.0),
            height: LengthPercentageAuto::length(0.0),
        },
        ..Style::default()
    })
    .build(ui, theme, ("terms-scroll", "pending"), body)?;
    fixed.push(scroll);
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Column,
            gap: page.gap(6.0),
            ..Style::default()
        },
        Visual::Group,
        &fixed,
    )
}

/// 生成那一行：语言勾选 + 全选 + 生成按钮（放在「待确认」页签）。
fn generate_row(ui: &mut Ui<Message>, page: &Page) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let mut lang_boxes = Vec::new();
    for lang in crate::terms::pending::Lang::ALL {
        let on = page
            .settings
            .gloss_langs
            .iter()
            .any(|code| *code == lang.code());
        lang_boxes.push(Checkbox::new(on, lang.label()).build(
            ui,
            theme,
            ("gloss-lang", lang.code()),
            Message::ToggleGlossLang(lang.code()),
        )?);
    }
    let marked_words = page
        .settings
        .terms
        .as_ref()
        .map(|list| list.marked.clone())
        .unwrap_or_default();
    let label = format!("生成译名（{} 条）", marked_words.len());
    let generating = page.settings.gloss.running();
    let mut generate = Button::new(if generating { "生成中…" } else { &label });
    generate = if generating {
        generate.disabled(true)
    } else {
        generate.primary()
    };
    let generate = generate.build(
        ui,
        theme,
        "generate-gloss",
        Message::GenerateTerms(marked_words),
    )?;
    let all_on = page
        .settings
        .terms
        .as_ref()
        .is_some_and(|list| !list.marked.is_empty());
    let mark_all = Button::new(if all_on { "全不选" } else { "全选" }).build(
        ui,
        theme,
        "mark-all-terms",
        Message::MarkAllTerms(!all_on),
    )?;
    let lang_row = ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: Some(AlignItems::CENTER),
            gap: page.gap(10.0),
            ..Style::default()
        },
        Visual::Group,
        &lang_boxes,
    )?;
    group(ui, theme, &[lang_row, mark_all, generate])
}

/// 页头两行：搜索与新增/导入；显示译名。
fn header(ui: &mut Ui<Message>, page: &Page) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let mut rows = Vec::new();

    let mut line = Vec::new();
    if let Some(field) = page.settings.fields.get(&FieldId::TermSearch) {
        line.push(
            TextField::new(field)
                .placeholder("词面 / 译名 / 读音")
                .width(240.0)
                .build(
                    ui,
                    theme,
                    "term-search",
                    |edit| Message::Edit(FieldId::TermSearch, edit),
                    None,
                    None,
                )?,
        );
    }
    line.push(Button::new("新增…").build(ui, theme, "add-term", Message::OpenTerm(None))?);
    line.push(Button::new("粘贴导入…").build(ui, theme, "paste-terms", Message::OpenTermPaste)?);
    let line = ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: Some(AlignItems::CENTER),
            gap: page.gap(8.0),
            ..Style::default()
        },
        Visual::Group,
        &line,
    )?;
    rows.push(
        SettingRow::new("搜索")
            .info("词面必填；读音留空就只参与英文注解、不进拼音候选")
            .control(line)
            .build(ui, theme)?,
    );

    let lang = choice(
        ui,
        page,
        "term-lang",
        &Lang::ALL,
        page.settings.term_lang,
        Lang::label,
        Message::TermLang,
    )?;
    rows.push(SettingRow::new("显示译名").control(lang).build(ui, theme)?);
    group(ui, theme, &rows)
}

fn chips(ui: &mut Ui<Message>, page: &Page, list: &Terms) -> Result<NodeId, RenderError> {
    let mut chips = Vec::new();
    chips.push(chip(
        ui,
        page,
        "全部",
        None,
        page.settings.term_tag.is_none(),
    )?);
    for (tag, count) in list.tags() {
        let selected = page.settings.term_tag.as_deref() == Some(tag.as_str());
        let label = format!("{tag} {count}");
        chips.push(chip(ui, page, &label, Some(tag), selected)?);
    }
    let gap = LengthPercentage::length(page.px(8.0));
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            gap: Size {
                width: gap,
                height: gap,
            },
            ..Style::default()
        },
        Visual::Group,
        &chips,
    )
}

fn chip(
    ui: &mut Ui<Message>,
    page: &Page,
    label: &str,
    tag: Option<String>,
    selected: bool,
) -> Result<NodeId, RenderError> {
    let mut button = Button::new(label);
    if selected {
        button = button.primary();
    }
    button.build(
        ui,
        &page.theme,
        ("term-tag", tag.clone().unwrap_or_else(|| "all".to_owned())),
        Message::TermTag(tag),
    )
}

/// 列表本体：最多 [PAGE_SIZE] 条，剩下的用标签筛选找。
fn list_body(ui: &mut Ui<Message>, page: &Page, list: &Terms) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let filter = page.settings.term_tag.as_deref();
    let query = page.settings.term_query.as_str();
    let matches = |term: &qingjian_platform::terms::Term| {
        crate::terms::matches(term, query)
            && match filter {
                None => true,
                Some(tag) => crate::terms::tag_of(term) == tag,
            }
    };
    let total = list.entries.iter().filter(|term| matches(term)).count();
    let mut rows = Vec::new();
    for (index, term) in list.entries.iter().enumerate() {
        if !matches(term) {
            continue;
        }
        if rows.len() >= PAGE_SIZE {
            break;
        }
        rows.push(row(ui, page, index, term)?);
    }
    let mut out = vec![group(ui, theme, &rows)?];
    if total > PAGE_SIZE {
        out.push(paragraph(
            ui,
            theme,
            &format!(
                "这一页只画前 {PAGE_SIZE} 条（筛出来共 {total} 条）；用上面的标签缩小范围，或下一版加搜索。"
            ),
            None,
        )?);
    }
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Column,
            gap: page.gap(4.0),
            ..Style::default()
        },
        Visual::Group,
        &out,
    )
}

/// 一条术语：词面一行，译名（按所选语种）与读音一行，右边一个「改」。
fn row(
    ui: &mut Ui<Message>,
    page: &Page,
    index: usize,
    term: &qingjian_platform::terms::Term,
) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let word = text(
        ui,
        &term.word,
        theme.text(theme.font.size, 500, theme.colors.text),
    )?;
    let translation = page.settings.term_lang.value(term).to_owned();
    let mut detail = if translation.is_empty() {
        "（这个语种还没译名）".to_owned()
    } else {
        translation
    };
    if !term.reading.is_empty() {
        detail.push_str(" · ");
        detail.push_str(&term.reading);
    }
    let detail = text(
        ui,
        &truncate(&detail, 56),
        theme.text(theme.small, 400, theme.colors.secondary),
    )?;
    let text_column = ui.scene().node(
        Style {
            flex_direction: FlexDirection::Column,
            gap: page.gap(2.0),
            flex_grow: 1.0,
            ..Style::default()
        },
        Visual::Group,
        &[word, detail],
    )?;
    let marked = page
        .settings
        .terms
        .as_ref()
        .is_some_and(|list| list.marked.iter().any(|word| word == &term.word));
    let check = Checkbox::new(marked, "").build(
        ui,
        theme,
        ("mark-term", term.word.as_str()),
        Message::MarkTerm(term.word.clone()),
    )?;
    let edit = Button::new("改").build(
        ui,
        theme,
        ("term-edit", index),
        Message::OpenTerm(Some(index)),
    )?;
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            align_items: Some(AlignItems::CENTER),
            gap: page.gap(12.0),
            padding: Rect {
                top: LengthPercentage::length(page.px(6.0)),
                bottom: LengthPercentage::length(page.px(6.0)),
                left: LengthPercentage::length(0.0),
                right: LengthPercentage::length(0.0),
            },
            size: Size {
                width: Dimension::percent(1.0),
                height: Dimension::auto(),
            },
            ..Style::default()
        },
        Visual::Group,
        &[check, text_column, edit],
    )
}

/// 新增 / 编辑的对话框。
fn editor(ui: &mut Ui<Message>, page: &Page) -> Result<(), RenderError> {
    let theme = &page.theme;
    let Some(editor) = &page.settings.term else {
        return Ok(());
    };
    let fields = &page.settings.fields;
    let title = ui.scene().node(
        Style::default(),
        Visual::Text {
            text: if editor.index.is_some() {
                "编辑术语"
            } else {
                "新增术语"
            }
            .to_owned(),
            style: theme.text(theme.font.size + 2.0, 600, theme.colors.text),
        },
        &[],
    )?;
    let mut children = vec![title];
    for (label, id, placeholder) in [
        ("词面", FieldId::TermWord, "如 中间验收"),
        (
            "读音",
            FieldId::TermReading,
            "如 zhong jian yan shou；留空不进候选",
        ),
        ("标签", FieldId::TermTag, "如 工程施工"),
        ("英文", FieldId::TermEnglish, "如 intermediate inspection"),
        ("日文", FieldId::TermJapanese, ""),
        ("西班牙文", FieldId::TermSpanish, ""),
        ("法文", FieldId::TermFrench, ""),
        ("韩文", FieldId::TermKorean, ""),
    ] {
        let Some(value) = fields.get(&id) else {
            continue;
        };
        let width = DIALOG_WIDTH - 120.0;
        let mut field = TextField::new(value).width(width);
        if !placeholder.is_empty() {
            field = field.placeholder(placeholder);
        }
        let field = field.build(
            ui,
            theme,
            ("term-field", id.key()),
            move |edit| Message::Edit(id, edit),
            None,
            Some(Message::CloseTerm),
        )?;
        children.push(SettingRow::new(label).control(field).build(ui, theme)?);
    }
    let candidates = Switch::new(editor.in_candidates).build(
        ui,
        theme,
        "term-candidates",
        Message::TermCandidates(!editor.in_candidates),
    )?;
    children.push(
        SettingRow::new("进拼音候选")
            .info("勾上且填了读音，打拼音时这个词会出现在候选里（只参与排序，不置顶）")
            .control(candidates)
            .build(ui, theme)?,
    );
    if let Some(error) = &editor.error {
        children.push(paragraph(ui, theme, error, None)?);
    }
    let mut buttons = Vec::new();
    if editor.index.is_some() {
        buttons.push(
            Button::new(if editor.confirm_delete {
                "再点一次删除"
            } else {
                "删除"
            })
            .build(ui, theme, "term-delete", Message::DeleteTerm)?,
        );
    }
    let spacer = ui.scene().node(
        Style {
            flex_grow: 1.0,
            size: Size {
                width: Dimension::auto(),
                height: Dimension::auto(),
            },
            ..Style::default()
        },
        Visual::Group,
        &[],
    )?;
    buttons.push(spacer);
    buttons.push(Button::new("取消").build(ui, theme, "term-cancel", Message::CloseTerm)?);
    buttons.push(
        Button::new("保存")
            .primary()
            .build(ui, theme, "term-save", Message::SaveTerm)?,
    );
    children.push(ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            align_items: Some(AlignItems::CENTER),
            gap: page.gap(8.0),
            margin: Rect {
                top: LengthPercentageAuto::length(page.px(4.0)),
                ..Rect::zero()
            },
            ..Style::default()
        },
        Visual::Group,
        &buttons,
    )?);
    dialog(ui, theme, "term-dialog", DIALOG_WIDTH, &children)
}

fn text(
    ui: &mut Ui<Message>,
    value: &str,
    style: bamboo_slip::TextStyle,
) -> Result<NodeId, RenderError> {
    let text = if value.is_empty() { "—" } else { value };
    ui.scene().node(
        Style::default(),
        Visual::Text {
            text: text.to_owned(),
            style,
        },
        &[],
    )
}

/// 粘贴导入卡片：一行一个词。
fn paste_dialog(ui: &mut Ui<Message>, page: &Page) -> Result<(), RenderError> {
    let theme = &page.theme;
    let Some(paste) = &page.settings.term_paste else {
        return Ok(());
    };
    let Some(field) = page.settings.fields.get(&FieldId::TermPaste) else {
        return Ok(());
    };
    let title = ui.scene().node(
        Style::default(),
        Visual::Text {
            text: "粘贴导入".to_owned(),
            style: theme.text(theme.font.size + 2.0, 600, theme.colors.text),
        },
        &[],
    )?;
    let hint = paragraph(
        ui,
        theme,
        "一行一个词，后面可以跟读音、标签、英文，用制表符分隔；同名的跳过（保留原来那条）。例：\n中间验收⇥zhong jian yan shou⇥项目管理⇥intermediate inspection",
        None,
    )?;
    let text = TextField::new(field)
        .placeholder("一行一个词…")
        .width(DIALOG_WIDTH - 60.0)
        .rows(8)
        .build(
            ui,
            theme,
            "term-paste",
            |edit| Message::Edit(FieldId::TermPaste, edit),
            None,
            None,
        )?;
    let mut children = vec![title, hint, text];
    if let Some(error) = &paste.error {
        children.push(paragraph(ui, theme, error, None)?);
    }
    if let Some((added, skipped)) = paste.result {
        children.push(paragraph(
            ui,
            theme,
            &format!("已导入 {added} 条，跳过 {skipped} 条同名的；关掉卡片就能看到。"),
            None,
        )?);
    }
    let spacer = ui.scene().node(
        Style {
            flex_grow: 1.0,
            size: Size {
                width: Dimension::auto(),
                height: Dimension::auto(),
            },
            ..Style::default()
        },
        Visual::Group,
        &[],
    )?;
    let cancel =
        Button::new("关闭").build(ui, theme, "term-paste-close", Message::CloseTermPaste)?;
    let import = Button::new("导入").primary().build(
        ui,
        theme,
        "term-paste-import",
        Message::ImportTerms,
    )?;
    let buttons = ui.scene().node(
        Style {
            flex_direction: FlexDirection::Row,
            align_items: Some(AlignItems::CENTER),
            gap: page.gap(8.0),
            margin: Rect {
                top: LengthPercentageAuto::length(page.px(4.0)),
                ..Rect::zero()
            },
            ..Style::default()
        },
        Visual::Group,
        &[spacer, cancel, import],
    )?;
    children.push(buttons);
    dialog(ui, theme, "term-paste-dialog", DIALOG_WIDTH, &children)
}

/// 待确认区：每条一行「词 · 语种 · 译名」，右边「确认」「丢弃」。
fn pending_section(ui: &mut Ui<Message>, page: &Page) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let waiting: Vec<(usize, &crate::terms::pending::Pending)> = page
        .settings
        .pending
        .iter()
        .enumerate()
        .filter(|(_, row)| row.status == crate::terms::pending::Status::Waiting)
        .collect();
    if waiting.is_empty() {
        return ui.scene().node(Style::default(), Visual::Group, &[]);
    }
    let mut rows = vec![section_title(ui, theme, "待确认的译名")?];
    for (index, row) in &waiting {
        let label = format!(
            "{} · {} · {}（{}）",
            row.word,
            row.lang.label(),
            row.text,
            row.status.label()
        );
        let text = paragraph(ui, theme, &label, None)?;
        let accept = Button::new("确认").build(
            ui,
            theme,
            ("accept-pending", *index),
            Message::AcceptPending(*index),
        )?;
        let drop = Button::new("丢弃").build(
            ui,
            theme,
            ("drop-pending", *index),
            Message::DropPending(*index),
        )?;
        let buttons = ui.scene().node(
            Style {
                flex_direction: FlexDirection::Row,
                align_items: Some(AlignItems::CENTER),
                gap: page.gap(6.0),
                ..Style::default()
            },
            Visual::Group,
            &[accept, drop],
        )?;
        let row = ui.scene().node(
            Style {
                flex_direction: FlexDirection::Row,
                align_items: Some(AlignItems::CENTER),
                gap: page.gap(10.0),
                size: Size {
                    width: Dimension::percent(1.0),
                    height: Dimension::auto(),
                },
                ..Style::default()
            },
            Visual::Group,
            &[text, buttons],
        )?;
        rows.push(row);
    }
    group(ui, theme, &rows)
}

/// 副标题单行截断：折行排版每帧都要量文字，长了又占地方。
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}
