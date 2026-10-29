//! 端到端回归：真实 Runner 里拖术语页列表的滚动条，滚动位置要变。
//!
//! 这一条补的是「组件级测试过了、真窗口里拖不动」的空档：事件要经过 Runner → 场景 → Interaction，
//! 命中的是不是滑块、按下去有没有捕获、拖动有没有换算成滚动量，都得一起验。

use std::time::Instant;

use bamboo_slip::{Event, FontLibrary, Key, Runner};

use crate::page_id::PageId;
use crate::settings::Settings;
use crate::terms::Terms;

const SCALE: f32 = 2.0;
const SIZE: (u32, u32) = (1640, 1240);

#[test]
fn dragging_the_terms_list_scrollbar_moves_the_list() {
    let dir = std::env::temp_dir().join(format!("qingjian-drag-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");

    let mut settings = Settings::load(config).unwrap();
    // 直接塞 800 条术语：列表一定溢出，滚动条一定出现
    let mut text = String::new();
    for i in 0..800 {
        text.push_str(&format!(
            "测试词{i}\tce shi ci{i}\t工程施工\tY\ttest word {i}\n"
        ));
    }
    settings.terms = Some(Terms {
        entries: qingjian_platform::terms::parse(&text),
        blocked: 0,
        marked: Vec::new(),
        error: None,
    });

    let library = match crate::system::fonts::ui_font() {
        Some(font) => FontLibrary::with_ui_font("zh-CN", &font).unwrap(),
        None => FontLibrary::system("zh-CN").unwrap(),
    };
    let mut runner = Runner::new(settings, library);
    runner.stage().set_animations(false);
    runner.rebuild(SCALE, SIZE, Instant::now()).unwrap();

    // 点侧栏进「术语」页
    let nav = Key::ROOT.child("nav").child(PageId::Terms);
    let (x, y, width, height) = runner.interaction().rect(nav).expect("侧栏要有术语项");
    runner.handle(&Event::PointerMoved {
        x: x + width / 2.0,
        y: y + height / 2.0,
    });
    runner.handle(&Event::PointerDown);
    runner.handle(&Event::PointerUp);
    // 多建两帧：滚动区第一帧才量出大小，第二帧才有滑块
    for _ in 0..3 {
        runner.rebuild(SCALE, SIZE, Instant::now()).unwrap();
    }

    assert_eq!(runner.app().page, PageId::Terms, "应该已经切到术语页");
    let scroll = Key::ROOT.child(("terms-scroll", "list"));
    assert!(
        runner.interaction().rect(scroll).is_some(),
        "术语页的列表滚动区应该存在"
    );
    let extent = runner
        .interaction()
        .scroll_extent(scroll)
        .expect("滚动区应该已经量出大小");
    assert!(
        extent.max().1 > 0.0,
        "内容没溢出：viewport={:?} content={:?}",
        extent.viewport,
        extent.content
    );
    let bar = scroll.child("bar");
    let (x, y, width, height) = runner
        .interaction()
        .rect(bar)
        .expect("术语页列表应该出现滚动条");

    runner.handle(&Event::PointerMoved {
        x: x + width / 2.0,
        y: y + height / 2.0,
    });
    runner.handle(&Event::PointerDown);
    let before = runner.interaction().scroll_offset(scroll).1;
    runner.handle(&Event::PointerMoved {
        x: x + width / 2.0,
        y: y + height / 2.0 + 60.0,
    });
    let after = runner.interaction().scroll_offset(scroll).1;
    assert!(
        after > before,
        "按住滑块往下拖，列表要跟着滚：before={before} after={after}"
    );
    runner.handle(&Event::PointerUp);

    std::fs::remove_dir_all(&dir).ok();
}
