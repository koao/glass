/// 日本語フォント候補（パス, ファミリ名）
/// 上から順に試行し、最初に見つかったものを使用
const FONT_CANDIDATES: &[(&str, &str)] = &[
    ("C:\\Windows\\Fonts\\meiryo.ttc", "Meiryo"),
    ("C:\\Windows\\Fonts\\msgothic.ttc", "MS Gothic"),
    ("C:\\Windows\\Fonts\\YuGothR.ttc", "Yu Gothic"),
];

/// 利用可能な日本語フォントを返す（パス, ファミリ名）
pub fn chosen_font() -> Option<(&'static str, &'static str)> {
    FONT_CANDIDATES
        .iter()
        .find(|(path, _)| std::path::Path::new(path).exists())
        .copied()
}
