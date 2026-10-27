//! 术语译名生成：借 `qingjian-predict` 的释义兜底（`GlossFiller`），后台线程问模型、结果回主线程。
//!
//! 与候选窗释义兜底同一套提示词与解析（只要第一条译名）；生成结果先进待确认表，
//! 用户确认后才写进术语表的语种列。法 / 韩语言还没进 `Language`，这两列先按英语口径生成。

use std::sync::{Arc, Mutex};

use bamboo_slip::Waker;
use qingjian_core::GlossFiller;
use qingjian_predict::{CloudGlossFiller, PredictConfig};

use super::pending::{Lang, Pending, Status};

/// 一批最多几个词（与释义兜底一致）。
const BATCH: usize = 8;

/// 一次生成最多问几批（防止误点把额度烧光）。
pub const MAX_BATCHES_PER_RUN: usize = 4;

/// 后台线程的产出。
pub enum Outcome {
    /// 一条生成好的译名。
    Filled(Pending),
    /// 这一批失败的原因。
    Failed(String),
    /// 全部做完。
    Done,
}

/// 术语译名生成任务。
#[derive(Default)]
pub struct TermGloss {
    outcomes: Arc<Mutex<Vec<Outcome>>>,
    running: bool,
    waker: Option<Waker>,
}

impl TermGloss {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// 开跑：`words` 要生成译名的词，`langs` 要哪几个语种。
    pub fn start(&mut self, config: &PredictConfig, words: Vec<String>, langs: Vec<Lang>) {
        if self.running || words.is_empty() || langs.is_empty() {
            return;
        }
        if !config.enabled {
            self.fail("云服务没启用：在「云服务」页打开再试");
            return;
        }
        let Some(_) = config.resolve_api_key() else {
            self.fail("还没配 API 密钥：在「云服务」页填好再试");
            return;
        };
        let outcomes = Arc::clone(&self.outcomes);
        let waker = self.waker.clone();
        self.running = true;
        let config = config.clone();
        std::thread::spawn(move || {
            let mut filled = 0;
            for (index, chunk) in words.chunks(BATCH).enumerate() {
                if index >= MAX_BATCHES_PER_RUN {
                    break;
                }
                for lang in &langs {
                    match ask(&config, chunk, *lang) {
                        Ok(rows) => {
                            filled += rows.len();
                            for row in rows {
                                push(&outcomes, Outcome::Filled(row));
                            }
                        }
                        Err(reason) => push(&outcomes, Outcome::Failed(reason)),
                    }
                }
                wake(&waker);
            }
            if filled == 0 {
                push(
                    &outcomes,
                    Outcome::Failed("一条也没生成出来：检查云服务设置或换个模型".to_owned()),
                );
            }
            push(&outcomes, Outcome::Done);
            wake(&waker);
        });
    }

    /// 取走这一轮的结果。
    pub fn drain(&mut self) -> Vec<Outcome> {
        let mut guard = match self.outcomes.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let taken: Vec<Outcome> = guard.drain(..).collect();
        if taken.iter().any(|outcome| matches!(outcome, Outcome::Done)) {
            self.running = false;
        }
        taken
    }

    fn fail(&mut self, reason: &str) {
        self.running = false;
        push(&self.outcomes, Outcome::Failed(reason.to_owned()));
        push(&self.outcomes, Outcome::Done);
        if let Some(waker) = &self.waker {
            waker.wake();
        }
    }
}

fn push(outcomes: &Arc<Mutex<Vec<Outcome>>>, outcome: Outcome) {
    match outcomes.lock() {
        Ok(mut guard) => guard.push(outcome),
        Err(poisoned) => poisoned.into_inner().push(outcome),
    }
}

fn wake(waker: &Option<Waker>) {
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// 问一批：开一个一次性的兜底线程，收齐这一批（等太久就带着已有结果返回），然后关掉。
fn ask(config: &PredictConfig, words: &[String], lang: Lang) -> Result<Vec<Pending>, String> {
    let mut filler =
        CloudGlossFiller::new(config).map_err(|error| format!("起不了网络线程：{error}"))?;
    for word in words {
        filler.request(lang.core(), word);
    }
    let started = std::time::Instant::now();
    let mut out: Vec<Pending> = Vec::new();
    while out.len() < words.len() {
        if started.elapsed() > std::time::Duration::from_secs(30) {
            break;
        }
        let arrived = filler.poll();
        if arrived.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        for filled in arrived {
            let Some(text) = filled
                .translation
                .senses()
                .first()
                .map(|sense| sense.text.trim().to_owned())
                .filter(|text| !text.is_empty())
            else {
                continue;
            };
            out.push(Pending {
                word: filled.word,
                lang,
                text,
                status: Status::Waiting,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_maps_somewhere() {
        for lang in Lang::ALL {
            let _ = lang.core();
            assert!(!lang.code().is_empty());
        }
    }
}
