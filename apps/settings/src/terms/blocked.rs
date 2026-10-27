//! 屏蔽表：`user-blocked.tsv`，一行一个词。
//!
//! 术语页删掉的词写进来，学习层读到后不再把它加回用户词（否则删了又会被自动学习学回来）。
//! 重新把词加回术语表时从这里去掉。

use std::path::{Path, PathBuf};

use crate::terms;

const HEADER: &str = "# 青简屏蔽表：一行一个词。术语页删掉的词写在这里，自动学习不再加回来";

/// 屏蔽表与术语表同目录。
pub fn path() -> Option<PathBuf> {
    terms::path().map(|path| path.with_file_name("user-blocked.tsv"))
}

/// 现在屏蔽了哪些词。
pub fn words() -> Vec<String> {
    path().map(|path| words_at(&path)).unwrap_or_default()
}

pub fn add(word: &str) -> Result<(), String> {
    match path() {
        Some(path) => add_at(&path, word),
        None => Err("找不到用户数据目录".to_owned()),
    }
}

pub fn remove(word: &str) -> Result<(), String> {
    match path() {
        Some(path) => remove_at(&path, word),
        None => Err("找不到用户数据目录".to_owned()),
    }
}

pub fn words_at(path: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| !line.contains('\t'))
        .map(str::to_owned)
        .collect()
}

pub fn add_at(path: &Path, word: &str) -> Result<(), String> {
    let word = word.trim();
    if word.is_empty() {
        return Ok(());
    }
    let mut list = words_at(path);
    if list.iter().any(|existing| existing == word) {
        return Ok(());
    }
    list.push(word.to_owned());
    list.sort();
    write(path, &list)
}

pub fn remove_at(path: &Path, word: &str) -> Result<(), String> {
    let mut list = words_at(path);
    let before = list.len();
    list.retain(|existing| existing != word.trim());
    if list.len() == before {
        return Ok(());
    }
    write(path, &list)
}

fn write(path: &Path, words: &[String]) -> Result<(), String> {
    let mut text = String::from(HEADER);
    text.push('\n');
    for word in words {
        text.push_str(word);
        text.push('\n');
    }
    let temp = path.with_extension("tsv.tmp");
    std::fs::write(&temp, text).map_err(|error| format!("写 {} 失败：{error}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|error| format!("替换 {} 失败：{error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("qingjian-blocked-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("user-blocked.tsv")
    }

    #[test]
    fn add_remove_and_skip_duplicates() {
        let path = temp("add");
        add_at(&path, "旧词").unwrap();
        add_at(&path, "旧词").unwrap();
        assert_eq!(words_at(&path), vec!["旧词".to_owned()]);
        add_at(&path, "另一个").unwrap();
        assert_eq!(words_at(&path).len(), 2);
        remove_at(&path, "旧词").unwrap();
        assert_eq!(words_at(&path), vec!["另一个".to_owned()]);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn missing_file_reads_as_empty_and_comments_are_ignored() {
        let path = temp("comments");
        assert!(words_at(&path).is_empty());
        std::fs::write(&path, "# 注释\n\n旧词\n").unwrap();
        assert_eq!(words_at(&path), vec!["旧词".to_owned()]);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
