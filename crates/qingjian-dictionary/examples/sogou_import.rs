//! 用真实的搜狗用户词库 `.bin` 跑一遍导入，核对条数与内容。
//!
//! 跑法：`cargo run -p qingjian-dictionary --example sogou_import -- <文件.bin>`

use std::collections::{HashMap, HashSet};

use qingjian_dictionary::{Dictionary, import_sogou};

fn main() {
    let Some(source) = std::env::args().nth(1) else {
        eprintln!("用法：sogou_import <文件.bin>");
        std::process::exit(1);
    };
    let source = std::path::PathBuf::from(source);

    let mut readings: HashMap<char, String> = HashMap::new();
    for line in std::fs::read_to_string("assets/lexicon/char-pinyin.tsv")
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut fields = line.split('\t');
        if let (Some(ch), Some(syllable)) = (fields.next(), fields.next())
            && let Some(ch) = ch.chars().next()
        {
            readings.insert(ch, syllable.trim().to_owned());
        }
    }
    let common: HashSet<char> = std::fs::read_to_string("assets/lexicon/gb2312.txt")
        .unwrap_or_default()
        .chars()
        .collect();

    let dir = std::env::temp_dir().join("qingjian-sogou-example");
    let result = import_sogou(&source, &dir, &readings, &common).expect("导入失败");
    println!(
        "导入 {} 条，跳过 {} 条 → {}",
        result.imported.entries,
        result.skipped,
        result.imported.path.display()
    );

    let dictionary = Dictionary::from_path(&result.imported.path).expect("读不了写出的词库");
    for (word, syllables) in [("阿宝", vec!["a", "bao"]), ("包", vec!["bao"])] {
        if let Some(matches) = dictionary.lookup(&syllables, false).first() {
            println!("  {word} → {}", matches.text);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
