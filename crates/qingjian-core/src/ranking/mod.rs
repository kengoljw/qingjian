//! 候选排序。
//!
//! 词级排序规则：
//! 1. 音节数与输入完全一致的词优先（`kaifa` → 开发 排在 开发者 前）
//! 2. 覆盖输入字母多者优先（`kaif` → 开发者 排在 开 前）
//! 3. 切分里非末尾的简拼音节少者优先（`kaifa` 按 `kai fa` 读的 开放 排在按 `kai f a` 读的 开放啊 前）
//! 4. 最后一个音节完整匹配优先（`kaifa` → 开发 排在 开放 前）
//! 5. 同一输入串下选过的词优先（Learner 的 `choice_weight`：`mgs` 选过 美国式，下次 `mgs` 它在选过的那几个里排前面）；
//!    只比「选过没有」——次数多少走第 6 条的加分曲线，见 [`Scored::key`]
//! 6. 上下文得分：词库读音词频打底（[`sentence::lexical_log_prob`]），加大语言模型的上下文增益
//!    （[`sentence::context_lift`]：模型的条件概率减它自己的一元概率，模型不区分读音，直接拿它的先验会把
//!    `ne` 下的 那 抬到 呢 之上——见 `sentence::word_log_prob`）；再加用户选择次数与同输入串选择次数的加分
//!    （[`weight_bonus`]，对数且封顶），模糊音命中扣 ln 2、敲错变体命中扣那类敲错的代价
//!    （`correction::TypoKind::cost`，个人敲错表打折）。
//!    这样 `ba` 在「做了」后面出 吧、句首出 把；纯词频排序两处都只能出同一个
//! 7. 敲的原音节优先，词长短者优先，最后按字符串稳定排序保证结果可复现
//!
//! 词库静态词频只用于预选（命中太多时先按词频砍到够排的量）与兜底。

mod scored;

use std::collections::HashSet;

pub use scored::{PreselectKey, Scored, SortKey};

/// 用户选择次数的加分系数：加分 = 系数 × ln(1 + min(次数, [`WEIGHT_CAP`]))。
/// 这个加分只负责「同音词里偏向用户常选的那个」，所以既取对数又封顶（最多约 1.5 分，e^1.5 ≈ 4.5 倍），
/// 让上下文（bigram）仍能压过它；不封顶时选过 173 次的 的 会带着 5 分加分把任何含 的 的拆分路径抬到整词之上
/// （`haode` 出 号的 而不是 好的）。用户用得多的词另有个人 bigram 里的一元项撑腰，不靠这里。
pub const WEIGHT_BONUS: f64 = 0.5;

/// 加分里计入的选择次数上限。
pub const WEIGHT_CAP: u32 = 20;

/// 模糊音命中扣的分（词频减半）。敲错变体的代价见 `correction::TypoKind`。
pub const FUZZY_PENALTY: f64 = std::f64::consts::LN_2;

/// 用户选择次数换算成得分加成，词级排序与整句路径共用，见 [`WEIGHT_BONUS`]。
pub fn weight_bonus(count: u32) -> f64 {
    WEIGHT_BONUS * (1.0 + f64::from(count.min(WEIGHT_CAP))).ln()
}

/// 排序并按词文本去重（同一个词可能被多种切分命中，保留得分最高的一条），最多留 `limit` 条。
/// `context` 给每条命中算（同输入串下的选择次数, 上下文 log 概率），只对预选后剩下的那些调用。
///
/// 排序键先算好再排：单字母简拼能命中两万条，比较器里每次数字符数会让排序占掉几十毫秒；去重也只做到够数为止。
pub fn rank(
    items: &mut Vec<Scored<'_>>,
    limit: usize,
    context: impl Fn(&Scored<'_>) -> (u32, f64),
) {
    // 远超上限时先按结构键 + 词频线性选出前面一段：同一个词会被多种切分命中，多选一倍留给去重（结果仍可能略少于上限，无妨）
    let preselect = limit.saturating_mul(2);
    if items.len() > preselect.saturating_mul(2) {
        let mut keyed: Vec<(PreselectKey, Scored<'_>)> = items
            .drain(..)
            .map(|item| (item.preselect_key(), item))
            .collect();
        keyed.select_nth_unstable_by(preselect, |a, b| b.0.cmp(&a.0));
        keyed.truncate(preselect);
        items.extend(keyed.into_iter().map(|(_, item)| item));
    }
    let mut keyed: Vec<(SortKey<'_>, Scored<'_>)> = items
        .drain(..)
        .map(|item| {
            let (choice, log_prob) = context(&item);
            // 这个输入串下选过的次数与全词次数用同一条曲线：选过就是偏好，选得多更有分量，但都封顶，
            // 不至于像以前那样「选过一次就永远钉在第一」。
            let score = log_prob + weight_bonus(item.weight) + weight_bonus(choice) - item.penalty;
            (item.key(choice, score), item)
        })
        .collect();
    keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut seen: HashSet<&str> = HashSet::with_capacity(limit.min(keyed.len()));
    items.extend(
        keyed
            .into_iter()
            .map(|(_, item)| item)
            .filter(|item| seen.insert(item.hit.text))
            .take(limit),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use qingjian_dictionary::Match;

    fn hit<'a>(text: &'a str, pinyin: &'a str, frequency: u32, exact: bool) -> Match<'a> {
        Match {
            text,
            pinyin,
            frequency,
            exact,
        }
    }

    #[test]
    fn exact_beats_frequency_and_weight_beats_static_frequency() {
        let mut items = vec![
            Scored {
                hit: hit("开发者", "kai fa zhe", 99999, false),
                full_last: true,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开放", "kai fang", 20000, true),
                full_last: false,
                coverage: 5,
                abbreviated: 0,
                weight: 5,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开发", "kai fa", 9000, true),
                full_last: true,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, |_| (0, 0.0));
        let texts: Vec<&str> = items.iter().map(|s| s.hit.text).collect();
        assert_eq!(texts, ["开发", "开放", "开发者"]);
    }

    #[test]
    fn context_score_orders_within_the_same_structure() {
        let mut items = vec![
            Scored {
                hit: hit("把", "ba", 3_000_000, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("吧", "ba", 2_000_000, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        // 上下文说 吧 更像：词频高的 把 让位
        let by_context = |s: &Scored<'_>| (0, if s.hit.text == "吧" { -1.0 } else { -6.0 });
        rank(&mut items, usize::MAX, by_context);
        let texts: Vec<&str> = items.iter().map(|s| s.hit.text).collect();
        assert_eq!(texts, ["吧", "把"]);
        // 同一输入串下选过的压过上下文
        rank(&mut items, usize::MAX, |s| {
            (
                u32::from(s.hit.text == "把"),
                if s.hit.text == "吧" { -1.0 } else { -6.0 },
            )
        });
        assert_eq!(items[0].hit.text, "把");
        // 同分时用户选过的、非模糊音的靠前
        for item in &mut items {
            item.weight = u32::from(item.hit.text == "把") * 3;
        }
        rank(&mut items, usize::MAX, |_| (0, -2.0));
        assert_eq!(items[0].hit.text, "把");
        for item in &mut items {
            item.weight = 3;
            item.penalty = if item.hit.text == "把" {
                FUZZY_PENALTY
            } else {
                0.0
            };
        }
        rank(&mut items, usize::MAX, |_| (0, -2.0));
        assert_eq!(items[0].hit.text, "吧");
    }

    #[test]
    fn fewer_abbreviated_syllables_win_at_equal_coverage() {
        let mut items = vec![
            Scored {
                hit: hit("开放啊", "kai fang a", 132, true),
                full_last: true,
                coverage: 5,
                abbreviated: 1,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("开放", "kai fang", 500_000, true),
                full_last: false,
                coverage: 5,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, |_| (0, 0.0));
        assert_eq!(items[0].hit.text, "开放");
    }

    #[test]
    fn weight_bonus_is_logarithmic_and_capped() {
        assert_eq!(weight_bonus(0), 0.0);
        assert!(weight_bonus(1) < weight_bonus(10));
        assert_eq!(weight_bonus(WEIGHT_CAP), weight_bonus(WEIGHT_CAP * 50));
        assert!(weight_bonus(u32::MAX) < 1.6);
    }

    #[test]
    fn deduplicates_by_text_keeping_best() {
        let mut items = vec![
            Scored {
                hit: hit("西安", "xi an", 4000, false),
                full_last: true,
                coverage: 4,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("西安", "xi an", 4000, true),
                full_last: true,
                coverage: 4,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, |_| (0, 0.0));
        assert_eq!(items.len(), 1);
        assert!(items[0].hit.exact);
    }

    /// 用户报障：「输入 ne 想出现 呢，却总排在 那 后面」。`ne` 下 那 与 呢 的结构项逐项相同（都是单字、
    /// 都精确覆盖），只有分数能定序；而语言模型按词统计，它给 那 的先验（其实来自 `nà`）更高。
    /// 用 `sentence::word_log_prob` 打出来的分（词库读音词频打底 + 上下文增益）必须让 呢 在前。
    #[test]
    fn common_word_beats_the_model_prior_at_equal_structure() {
        use crate::sentence::{self, Context, LanguageModel, Personal};

        /// 模型按词统计：那 常见、呢 少见；句首没有额外增益。
        struct WordLevel;

        impl LanguageModel for WordLevel {
            fn log_prob(&self, _previous: Option<&str>, word: &str) -> Option<f64> {
                Some(match word {
                    "那" => -4.81,
                    "呢" => -7.76,
                    _ => return None,
                })
            }

            fn unigram_log_prob(&self, word: &str) -> Option<f64> {
                self.log_prob(None, word)
            }
        }

        let log_total = 19.43;
        let score = |text: &str, frequency: u32| {
            sentence::word_log_prob(
                &WordLevel,
                Personal::NONE,
                Context::START,
                text,
                frequency,
                log_total,
            )
        };
        // 词库读音词频：呢/ne 53.9 万，那/ne 3.1 万
        let ni = score("呢", 539_480);
        let na = score("那", 31_387);
        assert!(ni > na, "呢 {ni} 应排在 那 {na} 前");

        let mut items = vec![
            Scored {
                hit: hit("那", "ne", 31_387, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("呢", "ne", 539_480, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, |item| {
            let score = if item.hit.text == "呢" { ni } else { na };
            (0, score)
        });
        assert_eq!(items[0].hit.text, "呢");
    }

    /// 同一个输入串下选过的词仍要排前，但累计次数不能把常用字永远压在下面：
    /// 用户误上屏 那 20 次、正经选 呢 15 次，呢 的词频高十七倍，该 呢 在前。
    /// （以前选择次数是排序键里的一项，20 > 15 就把 那 钉死在第一。）
    #[test]
    fn choice_count_cannot_pin_a_mistake_above_a_much_commoner_word() {
        let mut items = vec![
            Scored {
                hit: hit("那", "ne", 31_387, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("呢", "ne", 539_480, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        // 那 选过 20 次、呢 15 次；分差按词频比给出（呢 高 2.84 分）
        rank(&mut items, usize::MAX, |item| {
            if item.hit.text == "那" {
                (20, 0.0)
            } else {
                (15, 2.84)
            }
        });
        assert_eq!(items[0].hit.text, "呢");

        // 但「只在一个输入串下选过」仍然算数：都没被选过时，选过的那个排前
        let mut items = vec![
            Scored {
                hit: hit("那", "ne", 31_387, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
            Scored {
                hit: hit("呢", "ne", 539_480, true),
                full_last: true,
                coverage: 2,
                abbreviated: 0,
                weight: 0,
                penalty: 0.0,
            },
        ];
        rank(&mut items, usize::MAX, |item| {
            if item.hit.text == "那" {
                (1, 0.0)
            } else {
                (0, 0.0)
            }
        });
        assert_eq!(items[0].hit.text, "那");
    }
}
