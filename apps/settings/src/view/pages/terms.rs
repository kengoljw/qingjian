//! 「术语」页：本机个人术语表的只读列表，按标签筛。编辑请改用户目录里的 `user-terms.tsv`。
//! 术语只参与排序、不置顶；这一页只负责让人看得到表里有什么。

use bamboo_slip::taffy::{FlexDirection, FlexWrap, LengthPercentage, NodeId, Rect, Size, Style};
use bamboo_slip::{Button, RenderError, Ui, Visual, group, paragraph, section_title};

use crate::message::Message;
use crate::view::Page;

pub fn terms(ui: &mut Ui<Message>, page: &Page) -> Result<Vec<NodeId>, RenderError> {
    let theme = &page.theme;
    let mut out = Vec::new();
    out.push(section_title(ui, theme, "个人术语表")?);

    let Some(list) = page.settings.terms.as_ref() else {
        out.push(paragraph(
            ui,
            theme,
            "还没读到术语表。进这一页时会读用户目录里的 user-terms.tsv。",
            None,
        )?);
        return Ok(out);
    };

    out.push(paragraph(
        ui,
        theme,
        &format!(
            "共 {} 条，其中 {} 条参与拼音候选。术语只参与排序、不置顶；这一页只读，编辑请改 {}。",
            list.entries.len(),
            list.usable(),
            crate::terms::path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "user-terms.tsv".to_owned())
        ),
        None,
    )?);
    if let Some(error) = &list.error {
        out.push(paragraph(ui, theme, error, None)?);
    }

    // 标签筛选：全部 + 各标签（带条数），当前选中的高亮。
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
    out.push(chip_row(ui, page, &chips)?);

    let filter = page.settings.term_tag.as_deref();
    let rows: Vec<NodeId> = list
        .matching(filter)
        .map(|term| row(ui, page, term))
        .collect::<Result<_, _>>()?;
    out.push(group(ui, theme, &rows)?);
    Ok(out)
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

fn chip_row(ui: &mut Ui<Message>, page: &Page, chips: &[NodeId]) -> Result<NodeId, RenderError> {
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
        chips,
    )
}

/// 一条术语：词面一行（主），英文与读音一行（次，长了会折行）。
fn row(
    ui: &mut Ui<Message>,
    page: &Page,
    term: &qingjian_platform::terms::Term,
) -> Result<NodeId, RenderError> {
    let theme = &page.theme;
    let word = text(
        ui,
        &term.word,
        theme.text(theme.font.size, 500, theme.colors.text),
    )?;
    let mut detail = term.english.clone();
    if !term.reading.is_empty() {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(&term.reading);
    }
    let detail = if detail.is_empty() {
        text(
            ui,
            "—",
            theme.text(theme.small, 400, theme.colors.secondary),
        )?
    } else {
        paragraph(ui, theme, &detail, None)?
    };
    ui.scene().node(
        Style {
            flex_direction: FlexDirection::Column,
            gap: page.gap(2.0),
            padding: Rect {
                top: LengthPercentage::length(page.px(6.0)),
                bottom: LengthPercentage::length(page.px(6.0)),
                left: LengthPercentage::length(0.0),
                right: LengthPercentage::length(0.0),
            },
            ..Style::default()
        },
        Visual::Group,
        &[word, detail],
    )
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
