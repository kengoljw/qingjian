/// 整句转换的打分来源：词级语言模型。实现放兄弟 crate（`qingjian-lm`），Core 只认这个 trait。
pub trait LanguageModel: Send {
    /// `log P(word | previous)`；`previous` 为 `None` 表示句首。模型不认识 `word` 时返回 `None`，
    /// 由 Core 用词库词频兜底。
    fn log_prob(&self, previous: Option<&str>, word: &str) -> Option<f64>;

    /// `log P(word)`：模型自己的一元概率，不看上文。模型不认识 `word` 时返回 `None`。
    ///
    /// 词级排序拿 [`log_prob`](Self::log_prob) 减它，剩下的才是「上文带来的增益」。必须分开算：
    /// 模型是按**词**统计的，不区分读音，`ne` 下 那 的一元分其实几乎全是 `nà` 的统计，
    /// 直接拿它跟 呢 比就会得出「那 比 呢 常用 17 倍」这种与词库读音词频相反的结论。
    fn unigram_log_prob(&self, _word: &str) -> Option<f64> {
        None
    }
}

/// 没接语言模型：一律兜底，整句转换退化为一元词频。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLanguageModel;

impl LanguageModel for NoLanguageModel {
    fn log_prob(&self, _previous: Option<&str>, _word: &str) -> Option<f64> {
        None
    }
}
