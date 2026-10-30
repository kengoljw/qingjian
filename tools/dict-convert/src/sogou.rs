//! 搜狗用户词库 `.bin`（文件头 `SGPU`）→ 青简词库 TSV。
//!
//! 搜狗的读音字段是私有编码，这里不解：读音由「单字读音表」逐字拼，
//! 只收常用字（GB2312）里、且每个字都有读音的词。搜狗词库里没有的词是自造词，
//! 拼出来的读音未必准，导入后可在术语页改。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use qingjian_dictionary::{looks_like_sogou, sogou_words};

use crate::error::ConvertError;

/// 读音表（`字\t音节`）与常用字集合。
pub struct Tables {
    pub readings: HashMap<char, String>,
    pub common: HashSet<char>,
}

pub fn load_tables(readings: &Path, common: &Path) -> Result<Tables, ConvertError> {
    let mut map = HashMap::new();
    for line in std::fs::read_to_string(readings)?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((ch, syllable)) = line.split_once('\t') else {
            continue;
        };
        let Some(ch) = ch.chars().next() else {
            continue;
        };
        let syllable = syllable.trim();
        if !syllable.is_empty() {
            map.insert(ch, syllable.to_owned());
        }
    }
    let set = std::fs::read_to_string(common)?.chars().collect();
    Ok(Tables {
        readings: map,
        common: set,
    })
}

/// 转换：写出 `词\t拼音\t词频` 的 TSV。
pub fn convert(
    input: &Path,
    readings: &Path,
    common: &Path,
    out: &Path,
) -> Result<(usize, usize), ConvertError> {
    let tables = load_tables(readings, common)?;
    let bytes = std::fs::read(input)?;
    if !looks_like_sogou(&bytes) {
        return Err(ConvertError::Format {
            path: input.to_owned(),
            line: 1,
            reason: "不是搜狗用户词库（文件头不是 SGPU）".to_owned(),
        });
    }
    let mut text = String::new();
    let mut skipped = 0;
    for word in sogou_words(&bytes) {
        let mut syllables = Vec::with_capacity(word.chars().count());
        let mut ok = true;
        for ch in word.chars() {
            if !tables.common.contains(&ch) {
                ok = false;
                break;
            }
            match tables.readings.get(&ch) {
                Some(syllable) => syllables.push(syllable.as_str()),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            skipped += 1;
            continue;
        }
        text.push_str(&word);
        text.push('\t');
        text.push_str(&syllables.join(" "));
        text.push_str("\t10000\n");
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let entries = text.lines().count();
    std::fs::write(out, text)?;
    tracing::info!(entries, skipped, "搜狗用户词库已转换");
    Ok((entries, skipped))
}
