//! 离线整句转换：把一串音节转成最可能的词序列（`woxiangqu` → 我想去）。
//!
//! 词图上每个格子放正好覆盖那几个音节的词（每格只留词频最高的几个），Viterbi + 束搜索找最优路径。
//! 打分来自 [`LanguageModel`]（bigram，`qingjian-lm` 实现），模型不认识的词用词库词频（一元）兜底并扣分；
//! 用户选过的词（Learner 的 weight）加分。没接模型时整体退化为一元词频。
//!
//! 个人 n-gram（[`UserNgram`]，二元 + 三元，随上屏在线更新、由 Learner 持有）与静态模型插值，让整句越用越像自己：
//! 静态模型只看前一个词，个人部分看前两个词（[`Context`]），Viterbi 里前二词取最优前驱的回指，不扩状态。
//!
//! 简拼位置（`wxq` 的 `w x q`）按前缀取词，每个格子多留一些候选，全靠语言模型在路径上分辨；
//! 全拼句子末尾落单的声母也按前缀读（`youdiandongx` → 有点东西），落单的单元音（i / u）当作没打完的韵母丢弃，
//! 简拼句子里末尾单字母就是一个音节。
//!
//! 格子查词是最贵的一步，结果放进 [`SpanCache`]：敲键是增量的，每一键只有以它结尾的几个格子是新的。

mod context;
mod conversion;
mod interpolation;
mod language_model;
mod personal;
mod scorer;
mod sentence_word;
mod span;
mod text_segment;
mod user_ngram;
mod viterbi;

pub use context::Context;
pub use conversion::Conversion;
pub use interpolation::Interpolation;
pub use language_model::{LanguageModel, NoLanguageModel};
pub use personal::Personal;
pub use scorer::SentenceScorer;
pub use sentence_word::SentenceWord;
pub use span::{MAX_SPAN_CACHE_ENTRIES, SpanCache, SpanWord};
pub(crate) use text_segment::is_han;
pub use text_segment::{MAX_TEXT_WORD_CHARS, segment_text};
pub use user_ngram::UserNgram;
pub use viterbi::{convert, convert_paths, convert_whole, convert_with};

/// 句首标记：个人 n-gram 里句首词的前词。与 `qingjian-lm` 语料统计用的是同一个记号。
pub const SENTENCE_START: &str = "<s>";

/// 个人概率里 bigram 部分的权重，其余给个人一元。
pub const USER_LAMBDA: f64 = 0.8;

/// 个人插值权重 μ = c(v)/(c(v)+K)：前词见过 K 次时个人数据与静态模型各占一半（再受封顶限制）。
/// 取 8 是为了一次误选翻不过静态 bigram，选两次才翻。
pub const CONFIDENCE_K: f64 = 8.0;

/// 个人插值权重的封顶：用户数据再多，没见过的接续也最多打这个折，保留说新话的余地。
pub const MAX_CONFIDENCE: f64 = 0.5;

/// 个人三元的绝对折扣 D：每条见过的三元接续让出 D 份概率给二元回退，见得少的上文回退得多。
/// 0.75 是 n-gram 平滑的惯用值，个人数据量小、一次见过的接续占多数时它决定了三元能压过二元多少。
pub const TRIGRAM_DISCOUNT: f64 = 0.75;

/// 个人 n-gram 最多存多少条转移（二元对 + 三元条），超过就整体减半（忘掉久远的偏好）。一年的个人输入远到不了这个量。
pub const MAX_USER_TRANSITIONS: usize = 200_000;

/// 一个词最多几个音节；更长的词库里有但极少，限制它让词图规模可控。
pub const MAX_WORD_SYLLABLES: usize = 8;

/// 全拼句子末尾未打完的音节至少要几个字母才参与整句；再短的前缀只有落单声母例外（见 `viterbi`），
/// 落单的 i / u 等单元音范围太大又像没打完的韵母，不参与。
pub const MIN_PARTIAL_LETTERS: usize = 2;

/// 每个格子最多留几个词（按词库词频 + 用户加分）。同音词很多，全留会让束搜索白费。
pub const SPAN_CANDIDATES: usize = 6;

/// 有简拼位置的格子最多留几个词：`h` 下有 和 / 好 / 会 / 还 / 很 …… 几十个常用字，
/// 只留六个会把句子里要的那个挤掉，多留一些让语言模型去挑。
pub const ABBREVIATED_SPAN_CANDIDATES: usize = 20;

/// 每个位置最多保留几条部分路径。
pub const BEAM_WIDTH: usize = 8;

/// 语言模型不认识、只能按词库词频兜底的词扣多少分：模型见过的词更可信。
pub const FALLBACK_PENALTY: f64 = -4.0;

/// 模型不认识的词的兜底 log 概率：词库词频占总词频的比例再扣 [`FALLBACK_PENALTY`]。`log_total` 是总词频的对数。
pub fn fallback_log_prob(frequency: u32, log_total: f64) -> f64 {
    lexical_log_prob(frequency, log_total) + FALLBACK_PENALTY
}

/// 词自己的一元 log 概率，按**词库读音词频**算：同一个字在 `ne` 下和 `na` 下是两条不同的记录，
/// 词频各算各的（呢/ne 53.9 万、那/ne 3.1 万），正是「用户敲这个音想要哪个字」该看的数。
pub fn lexical_log_prob(frequency: u32, log_total: f64) -> f64 {
    (f64::from(frequency) + 1.0).ln() - log_total
}

/// 词级排序里语言模型该贡献多少：上文让它比平时更可能的倍数（log 比），不是它的一元概率本身。
///
/// 模型按**词**统计、不分读音，所以它的一元项对「同音不同字」没有区分力，反而会误导：
/// 句首打 `ne`，模型给 那 −4.8、给 呢 −7.8（那 的统计几乎全是 `nà`），而词库读音词频说 呢 是 那 的 17 倍。
/// 拿模型的**条件概率减自己的一元概率**，剩下的才是「这个上文偏向它」的证据：
/// 「做了」后面出 吧、句首出 把 靠的正是这份增益，而「那 比 呢 常见」这种与读音无关的先验被抵掉了。
/// 模型不认识这个词时（`unigram` 为 `None`）没有增益可加，返回 0。
pub fn context_lift(transition: f64, unigram: Option<f64>) -> f64 {
    unigram.map_or(0.0, |base| (transition - base).clamp(-LIFT_CAP, LIFT_CAP))
}

/// [`context_lift`] 的上下限：1.5 分约 4.5 倍。留出足够的上下文话语权（「我想 → 去蹭饭」这类搭配）
/// 又不让模型凭一个稀疏搭配把词库词频差了两三个数量级的词翻上来。
pub const LIFT_CAP: f64 = 1.5;

/// 词级排序的得分：词库读音词频打底，再加大语言模型的上下文增益。
/// 句首、单字、同音词这些「模型没有上下文可说」的场合，结果就等于词频序——用户要的常用字在前。
pub fn word_log_prob(
    model: &dyn LanguageModel,
    personal: Personal<'_>,
    context: Context<'_>,
    word: &str,
    frequency: u32,
    log_total: f64,
) -> f64 {
    let transition = transition_log_prob(
        model,
        personal,
        context,
        word,
        fallback_log_prob(frequency, log_total),
    );
    lexical_log_prob(frequency, log_total) + context_lift(transition, model.unigram_log_prob(word))
}

/// `log P(word | context)`：先问静态模型（只看前一个词，不认识就用 `fallback`），再与个人 n-gram（看前两个词）插值。
/// 整句路径上的每一步都用它；词级排序用 [`word_log_prob`]。
pub fn transition_log_prob(
    model: &dyn LanguageModel,
    personal: Personal<'_>,
    context: Context<'_>,
    word: &str,
    fallback: f64,
) -> f64 {
    let base = model.log_prob(context.previous, word).unwrap_or(fallback);
    personal.blend(context, word, base)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 模型按词统计：给 那 高、给 呢 低（真实模型就是这样，那 的统计几乎全是 `nà`）。
    /// `lift` 决定这个模型在给定上文下「比它平时更看好多少」，用来验证上下文仍有用。
    struct WordLevelModel {
        previous: Option<&'static str>,
        lift: f64,
    }

    impl WordLevelModel {
        fn new(previous: Option<&'static str>, lift: f64) -> Self {
            Self { previous, lift }
        }
    }

    impl LanguageModel for WordLevelModel {
        fn log_prob(&self, previous: Option<&str>, word: &str) -> Option<f64> {
            let unigram = self.unigram_log_prob(word)?;
            let base = unigram;
            Some(match (previous, self.previous) {
                // 给了「上文」的场合才把增益算进去，让测试能单独验证上下文的作用
                (Some(given), Some(expected)) if given == expected => base + self.lift,
                _ => base,
            })
        }

        fn unigram_log_prob(&self, word: &str) -> Option<f64> {
            Some(match word {
                "那" => -5.6,
                "呢" => -7.0,
                "吧" => -6.6,
                "把" => -7.9,
                _ => return None,
            })
        }
    }

    #[test]
    fn lexical_log_prob_follows_the_reading_frequency() {
        // 呢/ne 53.9 万、那/ne 3.1 万：同一份总词频下差的就是两者词频之比
        let log_total = 19.43;
        let ne = lexical_log_prob(539_480, log_total);
        let na = lexical_log_prob(31_387, log_total);
        let ratio = ((539_480.0_f64 + 1.0) / (31_387.0 + 1.0)).ln();
        assert!((ne - na - ratio).abs() < 1e-9, "{} vs {}", ne, na);
        assert!(ne > na);
    }

    #[test]
    fn context_lift_is_capped_and_needs_a_unigram() {
        // 模型不认识这个词：没有增益可加
        assert_eq!(context_lift(-4.0, None), 0.0);
        // 比平时更可能 / 更不可能，都封在 ±LIFT_CAP
        assert!((context_lift(-3.0, Some(-5.0)) - LIFT_CAP).abs() < 1e-9);
        assert!((context_lift(-7.0, Some(-5.0)) + LIFT_CAP).abs() < 1e-9);
        assert!((context_lift(-4.5, Some(-5.0)) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn word_score_prefers_the_reading_frequency_over_the_model_prior() {
        // 句首打 ne：模型给 那 的一元分高得多，但词库读音词频说 呢 是 那 的 17 倍。
        // 用户要的是 呢 —— 模型的先验不能压过词频，这正是「常用词不靠前」那个报障。
        let log_total = 19.43;
        let model = WordLevelModel::new(None, 1.0);
        let personal = Personal::NONE;
        let ni = word_log_prob(&model, personal, Context::START, "呢", 539_480, log_total);
        let na = word_log_prob(&model, personal, Context::START, "那", 31_387, log_total);
        assert!(ni > na, "呢 {ni} 应排在 那 {na} 前");
    }

    #[test]
    fn word_score_still_lets_the_context_overturn_a_frequency_gap() {
        // 「做了」后面：吧 的搭配增益 1.5 分，把 没有。把 词频高 1 分左右，但 吧 的增益更大时该赢。
        // 这里 把 词频只有 吧 的 1/3，增益全给 吧，验证上下文确实还能翻盘。
        let log_total = 19.43;
        let model = WordLevelModel::new(Some("做了"), LIFT_CAP);
        let personal = Personal::NONE;
        let context = Context::after("做了");
        let ba = word_log_prob(&model, personal, context, "吧", 881_092, log_total);
        // 把 没有增益（模型只给 吧 增益），且词频低
        let wrong = WordLevelModel::new(None, 0.0);
        let ba_plain = word_log_prob(&wrong, personal, context, "吧", 881_092, log_total);
        assert!(ba > ba_plain, "有上文时 吧 应当更高");
    }

    #[test]
    fn word_score_without_a_model_is_the_reading_frequency() {
        // 没接模型（NoLanguageModel）：词级排序退化为一元词频，常用字照样在前
        let log_total = 19.43;
        let personal = Personal::NONE;
        let model = NoLanguageModel;
        let ni = word_log_prob(&model, personal, Context::START, "呢", 539_480, log_total);
        assert!((ni - lexical_log_prob(539_480, log_total)).abs() < 1e-12);
    }
}
