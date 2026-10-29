//! 本机装的字族名，给字体下拉框列出来。按名字表读，不加载字形；设置程序启动时扫一次。

/// 字族名，排好序、去掉重复与系统私用的（`.` 开头）；Windows 上只留能显示汉字的字族
/// （中文候选窗的字体，选一个画不出汉字的字体没有意义）。
pub fn families() -> Vec<String> {
    let mut names = scan();
    names.retain(|name| !name.starts_with('.'));
    #[cfg(windows)]
    names.retain(|name| bamboo_slip::system_fonts::family_has_char(name, '中'));
    names.sort();
    names.dedup();
    names
}

#[cfg(windows)]
fn scan() -> Vec<String> {
    bamboo_slip::system_fonts::families()
}

/// 设置程序自己的界面字体：微软雅黑 UI（中文与拉丁同一套，不再 Segoe UI + 雅黑混搭）。
/// 系统里没有就返回 `None`，设置程序用平台缺省。
#[cfg(windows)]
pub fn ui_font() -> Option<bamboo_slip::UiFont> {
    for family in ["Microsoft YaHei UI", "Microsoft YaHei"] {
        let files = bamboo_slip::system_fonts::family_files(family);
        if !files.is_empty() {
            return Some(bamboo_slip::UiFont {
                family: family.to_owned(),
                files,
            });
        }
    }
    None
}

#[cfg(not(windows))]
pub fn ui_font() -> Option<bamboo_slip::UiFont> {
    None
}

#[cfg(not(windows))]
fn scan() -> Vec<String> {
    let mut db = cosmic_text::fontdb::Database::new();
    for dir in directories() {
        db.load_fonts_dir(dir);
    }
    db.faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect()
}

#[cfg(not(windows))]
fn directories() -> Vec<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut dirs: Vec<std::path::PathBuf> = if cfg!(target_os = "macos") {
        ["/System/Library/Fonts", "/Library/Fonts"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect()
    } else {
        ["/usr/share/fonts", "/usr/local/share/fonts"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect()
    };
    if let Some(home) = home {
        if cfg!(target_os = "macos") {
            dirs.push(home.join("Library/Fonts"));
        } else {
            dirs.push(home.join(".local/share/fonts"));
            dirs.push(home.join(".fonts"));
        }
    }
    dirs
}
