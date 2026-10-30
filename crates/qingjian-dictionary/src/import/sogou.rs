//! 搜狗用户词库 `.bin`（文件头 `SGPU`）：词面是 UTF-16LE 明文，每条前面有 u16 字节长度。
//!
//! 搜狗的读音字段是私有编码，这里不解；读数由调用方给的「单字 → 音节」表逐字拼出来，
//! 只收每个字都在常用字表（GB2312）里的词。多音字会取最常用的那个读音，导入后可在术语页改。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use qingjian_format::Metadata;

use crate::dictionary::Dictionary;
use crate::error::DictionaryError;

use super::imported::Imported;

/// 文件头是不是 Sogou 用户词库。
pub fn looks_like_sogou(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SGPU")
}

/// 从 `.bin` 里扫出所有「u16 字节长度 + 汉字」的词面，去重。
pub fn words(bytes: &[u8]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut offset = 0;
    while offset + 2 <= bytes.len() {
        let length = usize::from(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]));
        if (2..=40).contains(&length)
            && length % 2 == 0
            && offset + 2 + length <= bytes.len()
            && let Some(word) = decode_word(&bytes[offset + 2..offset + 2 + length])
            && seen.insert(word.clone())
        {
            words.push(word);
            offset += 2 + length;
            continue;
        }
        offset += 2;
    }
    words
}

/// 一段 UTF-16LE 是不是 1–12 个汉字。
fn decode_word(bytes: &[u8]) -> Option<String> {
    let mut word = String::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let code = u16::from_le_bytes([pair[0], pair[1]]);
        let ch = char::from_u32(u32::from(code))?;
        if !('\u{4e00}'..='\u{9fff}').contains(&ch) {
            return None;
        }
        word.push(ch);
    }
    (!word.is_empty() && word.chars().count() <= 12).then_some(word)
}

/// 搜狗词库导入的结果（比普通导入多一个「跳过」数）。
pub struct SogouImported {
    pub imported: Imported,
    pub skipped: usize,
}

/// 把 `.bin` 转成用户词库：`readings` 是「字 → 音节」，`common` 是允许的常用字。
pub fn import_sogou(
    source: &Path,
    dest_dir: &Path,
    readings: &HashMap<char, String>,
    common: &HashSet<char>,
) -> Result<SogouImported, DictionaryError> {
    let bytes = std::fs::read(source)?;
    if !looks_like_sogou(&bytes) {
        return Err(DictionaryError::Corrupt("not a Sogou SGPU dictionary"));
    }
    let stem = source
        .file_name()
        .and_then(|name| name.to_str())
        .map(super::strip_extensions)
        .filter(|stem| !stem.is_empty())
        .ok_or(DictionaryError::Corrupt("source has no usable file name"))?;
    std::fs::create_dir_all(dest_dir)?;
    let target = dest_dir.join(format!("{stem}.qj"));

    let mut tsv = String::new();
    let mut skipped = 0;
    for word in words(&bytes) {
        if word.chars().any(|ch| !common.contains(&ch)) {
            skipped += 1;
            continue;
        }
        let mut syllables = Vec::with_capacity(word.chars().count());
        let mut ok = true;
        for ch in word.chars() {
            match readings.get(&ch) {
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
        tsv.push_str(&word);
        tsv.push('\t');
        tsv.push_str(&syllables.join(" "));
        tsv.push_str("\t10000\n");
    }
    let dictionary = Dictionary::parse(&tsv)?;
    if dictionary.is_empty() {
        return Err(DictionaryError::Corrupt(
            "Sogou dictionary has no words with known readings",
        ));
    }
    let metadata = Metadata {
        name: stem.clone(),
        source: source.display().to_string(),
        ..Metadata::default()
    };
    dictionary.write_qj(&target, &metadata)?;
    Ok(SogouImported {
        imported: Imported {
            path: target,
            name: stem,
            entries: dictionary.len(),
        },
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(word: &str) -> Vec<u8> {
        let bytes: Vec<u8> = word.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut out = (bytes.len() as u16).to_le_bytes().to_vec();
        out.extend(bytes);
        out
    }

    #[test]
    fn reads_length_prefixed_words() {
        let mut bytes = b"SGPU".to_vec();
        bytes.extend([0; 36]);
        bytes.extend(record("阿宝"));
        bytes.extend(record("包"));
        assert_eq!(words(&bytes), vec!["阿宝", "包"]);
    }

    #[test]
    fn imports_words_with_readings_and_skips_unknown_characters() {
        let mut bytes = b"SGPU".to_vec();
        bytes.extend([0; 36]);
        bytes.extend(record("阿宝"));
        bytes.extend(record("包"));
        let readings = HashMap::from([
            ('阿', "a".to_owned()),
            ('宝', "bao".to_owned()),
            ('包', "bao".to_owned()),
        ]);
        let common: HashSet<char> = ['阿', '宝', '包'].into_iter().collect();
        let dir = std::env::temp_dir().join(format!("qingjian-sogou-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("user.bin");
        std::fs::write(&source, &bytes).unwrap();
        let result = import_sogou(&source, &dir.join("dicts"), &readings, &common).unwrap();
        assert_eq!(result.imported.entries, 2);
        assert_eq!(result.skipped, 0);
        let dictionary = Dictionary::from_path(&result.imported.path).unwrap();
        assert_eq!(dictionary.lookup(&["a", "bao"], false)[0].text, "阿宝");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_words_with_characters_outside_the_common_set() {
        let mut bytes = b"SGPU".to_vec();
        bytes.extend([0; 36]);
        bytes.extend(record("阿宝"));
        bytes.extend(record("包"));
        let readings = HashMap::from([
            ('阿', "a".to_owned()),
            ('宝', "bao".to_owned()),
            ('包', "bao".to_owned()),
        ]);
        let common: HashSet<char> = ['阿'].into_iter().collect();
        let dir = std::env::temp_dir().join(format!("qingjian-sogou-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("user.bin");
        std::fs::write(&source, &bytes).unwrap();
        let result = import_sogou(&source, &dir.join("dicts"), &readings, &common);
        assert!(result.is_err(), "没有可用词时报错，不写空词库");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
