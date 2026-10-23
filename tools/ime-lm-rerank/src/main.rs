//! 同池离线重排：把 chinese-ime-lm 的模型接到**青简已经产出的那批候选**上，量净收益。
//!
//! 输入是 `qingjian-cli --eval-text … --eval-details base.jsonl` 的逐句记录（带
//! `candidate_facts`），输出是同样结构的 `ext.jsonl`，只把候选顺序改成模型认为更好的那个。
//! 两条纪律写在代码里，因为它们决定了数字有没有意义：
//!
//! 1. **不重新生成候选**。只重排 `base.jsonl` 里那一批，否则比的是解码器不是重排器。
//! 2. **字表外字符整句跳过**。上游字表 12,288，青简 19,147，遇到模型不认识的字符不给它打分
//!    ——那个字只能当 `<unk>` 算，会把整条路径压死（青简 `all_known()` 是同一个道理）。
//!
//! 用法：`ime-lm-rerank <model.safetensors> <base.jsonl> <ext.jsonl>`

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::sync::Arc;
use std::time::Instant;

use chinese_ime_lm::{CandidateFacts, Reranker, SentenceModel};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("用法：ime-lm-rerank <model.safetensors> <base.jsonl> <ext.jsonl>");
        std::process::exit(2);
    }
    let (model_path, input, output) = (&args[0], &args[1], &args[2]);

    let bytes = std::fs::read(model_path).unwrap_or_else(|error| {
        eprintln!("读不了模型 {model_path}: {error}");
        std::process::exit(1);
    });
    let model = SentenceModel::load(&bytes).unwrap_or_else(|error| {
        eprintln!("模型加载失败: {error}");
        std::process::exit(1);
    });
    println!(
        "模型：{}  上下文 {}  许可 {}",
        model.attribution(),
        model.context_length(),
        model.license()
    );
    let mut reranker = Reranker::new(Arc::new(model));

    let reader = BufReader::new(std::fs::File::open(input).unwrap());
    let mut writer = BufWriter::new(std::fs::File::create(output).unwrap());

    let mut rows = 0usize;
    let mut decisions = 0usize;
    let mut promoted = 0usize;
    let mut skipped_unknown = 0usize;
    let mut elapsed = std::time::Duration::ZERO;

    for line in reader.lines() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        let mut row: serde_json::Value = match serde_json::from_str(&line) {
            Ok(row) => row,
            Err(_) => continue,
        };
        rows += 1;

        let context = row["context"].as_str().unwrap_or("").to_owned();
        let candidates: Vec<String> = row["candidates"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let facts = facts_of(&row);

        // 字表外字符：**参与比较的那些候选**里有模型不认识的字符就整句不打分，保留静态顺序。
        // 闸门不能套在全部候选上——英文、符号、颜文字本来就不在中文表里，套上去会把九成句子
        // 白白扔掉；真正被 `<unk>` 压坏的是 `best_where` 实际打分的那些（`answers_key` 为真）。
        let compared_known = candidates.iter().zip(&facts).all(|(candidate, fact)| {
            !fact.answers_key
                || candidate
                    .chars()
                    .all(|c| reranker.model().token(c).is_some())
        });
        if !compared_known || candidates.is_empty() {
            skipped_unknown += 1;
            writeln!(writer, "{line}").unwrap();
            continue;
        }

        let texts: Vec<&str> = candidates.iter().map(String::as_str).collect();
        let started = Instant::now();
        let best = reranker.best_where(&context, &texts, |index| {
            facts.get(index).copied().unwrap_or(CandidateFacts {
                answers_key: false,
                trusted_dictionary_hit: false,
            })
        });
        let per_row = started.elapsed();
        elapsed += per_row;
        decisions += 1;
        row["rerank_ms"] = serde_json::Value::from(per_row.as_secs_f64() * 1000.0);

        if let Some(index) = best {
            promoted += 1;
            let mut order: Vec<usize> = Vec::with_capacity(candidates.len());
            order.push(index);
            order.extend((0..candidates.len()).filter(|&other| other != index));
            reorder(&mut row, &order);
            row["promoted"] = serde_json::Value::from(index);
        }

        writeln!(writer, "{row}").unwrap();
        if rows.is_multiple_of(2000) {
            println!("  已处理 {rows} 句…");
        }
    }
    writer.flush().unwrap();

    let per = elapsed.as_secs_f64() * 1000.0 / decisions.max(1) as f64;
    println!(
        "完成：{rows} 句，参与决策 {decisions}，改序 {promoted}，字表外跳过 {skipped_unknown}，平均 {per:.2} ms/句"
    );
}

/// 从一行记录里取出上游要的两个事实；缺字段按「不吃满键、不是可信命中」处理（保守）。
fn facts_of(row: &serde_json::Value) -> Vec<CandidateFacts> {
    row["candidate_facts"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|fact| CandidateFacts {
                    answers_key: fact["answers_key"].as_bool().unwrap_or(false),
                    trusted_dictionary_hit: fact["trusted_dictionary_hit"]
                        .as_bool()
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 按给定顺序重排 `candidates` 与 `candidate_facts`，两份必须同步移动。
fn reorder(row: &mut serde_json::Value, order: &[usize]) {
    for field in ["candidates", "candidate_facts"] {
        let Some(list) = row[field].as_array() else {
            continue;
        };
        let moved: Vec<serde_json::Value> = order
            .iter()
            .filter_map(|&index| list.get(index).cloned())
            .collect();
        if moved.len() == list.len() {
            row[field] = serde_json::Value::Array(moved);
        }
    }
}
