use std::collections::HashSet;

use crate::model::entry::DataEntry;

// ---------------------------------------------------------------------------
// IDLE 検索条件（モニタ検索・プロトコル検索共通）
// ---------------------------------------------------------------------------

/// 比較演算子
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum IdleCmp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
}

/// IDLE 検索条件
#[derive(Debug, Clone)]
pub(crate) enum IdleCondition {
    /// すべての IDLE
    Any,
    /// 比較演算子付き (@IDLE>100 など)
    Compare(IdleCmp, f64),
    /// 範囲 (@IDLE100-500)
    Range(f64, f64),
}

impl IdleCondition {
    /// 1ms 未満を切り捨てた整数値で比較する
    pub(crate) fn matches(&self, ms: f64) -> bool {
        let ms = ms.floor();
        match self {
            IdleCondition::Any => true,
            IdleCondition::Compare(cmp, val) => match cmp {
                IdleCmp::Gt => ms > *val,
                IdleCmp::Ge => ms >= *val,
                IdleCmp::Lt => ms < *val,
                IdleCmp::Le => ms <= *val,
                IdleCmp::Eq => (ms - *val).abs() < f64::EPSILON,
            },
            IdleCondition::Range(lo, hi) => ms >= *lo && ms <= *hi,
        }
    }
}

/// `@IDLE...` 形式のクエリをパースして IDLE 条件を返す。
/// `@IDLE` で始まらなければ `None`（リテラル扱い）。
pub(crate) fn parse_idle_condition(input: &str) -> Option<IdleCondition> {
    let trimmed = input.trim();
    if trimmed.len() < 5 || !trimmed[..5].eq_ignore_ascii_case("@idle") {
        return None;
    }
    let rest = &trimmed[5..];
    if rest.is_empty() {
        return Some(IdleCondition::Any);
    }

    // >=N, >N, <=N, <N, =N
    if let Some(num_str) = rest.strip_prefix(">=") {
        return num_str
            .parse::<f64>()
            .ok()
            .map(|v| IdleCondition::Compare(IdleCmp::Ge, v));
    }
    if let Some(num_str) = rest.strip_prefix('>') {
        return num_str
            .parse::<f64>()
            .ok()
            .map(|v| IdleCondition::Compare(IdleCmp::Gt, v));
    }
    if let Some(num_str) = rest.strip_prefix("<=") {
        return num_str
            .parse::<f64>()
            .ok()
            .map(|v| IdleCondition::Compare(IdleCmp::Le, v));
    }
    if let Some(num_str) = rest.strip_prefix('<') {
        return num_str
            .parse::<f64>()
            .ok()
            .map(|v| IdleCondition::Compare(IdleCmp::Lt, v));
    }
    if let Some(num_str) = rest.strip_prefix('=') {
        return num_str
            .parse::<f64>()
            .ok()
            .map(|v| IdleCondition::Compare(IdleCmp::Eq, v));
    }

    // N-M (範囲)
    if let Some(dash_pos) = rest.find('-')
        && dash_pos > 0
    {
        let lo_str = &rest[..dash_pos];
        let hi_str = &rest[dash_pos + 1..];
        if let (Ok(lo), Ok(hi)) = (lo_str.parse::<f64>(), hi_str.parse::<f64>()) {
            return Some(IdleCondition::Range(lo, hi));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// モニタ検索
// ---------------------------------------------------------------------------

pub struct SearchState {
    pub query: String,
    results: Vec<usize>,
    current: usize,
    highlighted_indices: HashSet<usize>,
    current_highlighted: HashSet<usize>,
    pattern: Vec<u8>,
    scroll_to_entry: Option<usize>,
    pub has_searched: bool,
    last_search_len: usize,
    /// IDLE 検索モード時の条件
    idle_cond: Option<IdleCondition>,
}

/// Byteエントリのみ抽出: (エントリ配列インデックス, バイト値)
fn collect_byte_entries(entries: &[DataEntry]) -> Vec<(usize, u8)> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if let DataEntry::Byte(b, _) = e {
                Some((i, *b))
            } else {
                None
            }
        })
        .collect()
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            current: 0,
            highlighted_indices: HashSet::new(),
            current_highlighted: HashSet::new(),
            pattern: Vec::new(),
            scroll_to_entry: None,
            has_searched: false,
            last_search_len: 0,
            idle_cond: None,
        }
    }

    /// バッファ内を検索して結果を更新
    pub fn search(&mut self, entries: &[DataEntry]) {
        self.has_searched = true;
        self.last_search_len = entries.len();
        self.current = 0;

        if let Some(cond) = parse_idle_condition(&self.query) {
            self.idle_cond = Some(cond);
            self.pattern.clear();
            self.results.clear();
            self.highlighted_indices.clear();
            self.current_highlighted.clear();
            self.run_idle_search(entries, 0);
        } else {
            self.idle_cond = None;
            self.pattern = parse_mixed_pattern(&self.query);
            self.run_search(entries);
        }
    }

    /// バッファが増えていれば自動再検索（受信中用）
    pub fn auto_refresh(&mut self, entries: &[DataEntry]) {
        if !self.has_searched || entries.len() == self.last_search_len {
            return;
        }
        let has_query = self.idle_cond.is_some() || !self.pattern.is_empty();
        if !has_query {
            return;
        }
        let prev_len = self.last_search_len;
        self.last_search_len = entries.len();
        let prev_current = self.current;
        if self.idle_cond.is_some() {
            self.run_idle_search(entries, prev_len);
        } else {
            self.run_search(entries);
        }
        if !self.results.is_empty() {
            self.current = prev_current.min(self.results.len() - 1);
            if self.idle_cond.is_some() {
                self.update_current_highlight_idle();
            } else {
                let byte_entries = collect_byte_entries(entries);
                self.update_current_highlight(&byte_entries);
            }
        }
    }

    /// IDLE 検索（条件にマッチする Idle エントリを収集）
    /// `from` から走査を開始する（初回は 0、差分更新時は前回の末尾）
    fn run_idle_search(&mut self, entries: &[DataEntry], from: usize) {
        let cond = match &self.idle_cond {
            Some(c) => c,
            None => return,
        };

        for (i, entry) in entries.iter().enumerate().skip(from) {
            if let DataEntry::Idle(ms) = entry
                && cond.matches(*ms)
            {
                self.results.push(i);
                self.highlighted_indices.insert(i);
            }
        }
        if from == 0 && !self.results.is_empty() {
            self.current_highlighted.insert(self.results[0]);
            self.scroll_to_entry = Some(self.results[0]);
        }
    }

    /// 検索のコアロジック（results/highlighted_indicesを再構築）
    fn run_search(&mut self, entries: &[DataEntry]) {
        self.results.clear();
        self.highlighted_indices.clear();
        self.current_highlighted.clear();

        if self.pattern.is_empty() {
            return;
        }

        let byte_entries = collect_byte_entries(entries);
        let pat_len = self.pattern.len();
        if byte_entries.len() < pat_len {
            return;
        }

        for i in 0..=(byte_entries.len() - pat_len) {
            let matches = (0..pat_len).all(|j| byte_entries[i + j].1 == self.pattern[j]);
            if matches {
                self.results.push(i);
                for j in 0..pat_len {
                    self.highlighted_indices.insert(byte_entries[i + j].0);
                }
            }
        }

        self.update_current_highlight(&byte_entries);
    }

    /// 次の検索結果に移動
    pub fn next(&mut self, entries: &[DataEntry]) {
        self.navigate(entries, true);
    }

    /// 前の検索結果に移動
    pub fn prev(&mut self, entries: &[DataEntry]) {
        self.navigate(entries, false);
    }

    fn navigate(&mut self, entries: &[DataEntry], forward: bool) {
        if self.results.is_empty() {
            return;
        }
        if forward {
            self.current = (self.current + 1) % self.results.len();
        } else {
            self.current = if self.current == 0 {
                self.results.len() - 1
            } else {
                self.current - 1
            };
        }
        if self.idle_cond.is_some() {
            self.update_current_highlight_idle();
            self.scroll_to_entry = Some(self.results[self.current]);
        } else {
            let byte_entries = collect_byte_entries(entries);
            self.update_current_highlight(&byte_entries);
            let start = self.results[self.current];
            if start < byte_entries.len() {
                self.scroll_to_entry = Some(byte_entries[start].0);
            }
        }
    }

    /// IDLE モード用: current_highlighted を更新
    fn update_current_highlight_idle(&mut self) {
        self.current_highlighted.clear();
        if !self.results.is_empty() {
            self.current_highlighted.insert(self.results[self.current]);
        }
    }

    fn update_current_highlight(&mut self, byte_entries: &[(usize, u8)]) {
        self.current_highlighted.clear();
        if self.results.is_empty() || byte_entries.is_empty() {
            return;
        }
        let start = self.results[self.current];
        let pat_len = self.pattern.len();
        for j in 0..pat_len {
            if start + j < byte_entries.len() {
                self.current_highlighted.insert(byte_entries[start + j].0);
            }
        }
    }

    /// クエリと検索状態を全リセット
    pub fn reset(&mut self) {
        self.query.clear();
        self.clear();
    }

    /// 検索結果のみクリア（クエリは保持）
    pub fn clear(&mut self) {
        self.results.clear();
        self.current = 0;
        self.highlighted_indices.clear();
        self.current_highlighted.clear();
        self.pattern.clear();
        self.scroll_to_entry = None;
        self.has_searched = false;
        self.last_search_len = 0;
        self.idle_cond = None;
    }

    // --- アクセサ ---

    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn has_highlights(&self) -> bool {
        !self.highlighted_indices.is_empty()
    }

    pub fn is_highlighted(&self, entry_idx: usize) -> bool {
        self.current_highlighted.contains(&entry_idx)
            || self.highlighted_indices.contains(&entry_idx)
    }

    pub fn is_current_highlight(&self, entry_idx: usize) -> bool {
        self.current_highlighted.contains(&entry_idx)
    }

    pub fn take_scroll_target(&mut self) -> Option<usize> {
        self.scroll_to_entry.take()
    }
}

/// 混在パターンをパース
/// `$XX` → 16進数バイト、それ以外 → ASCIIバイト
/// 例: "OK$0D$0A" → [0x4F, 0x4B, 0x0D, 0x0A]
pub fn parse_mixed_pattern(input: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$'
            && i + 2 < chars.len()
            && chars[i + 1].is_ascii_hexdigit()
            && chars[i + 2].is_ascii_hexdigit()
        {
            let hex_str: String = [chars[i + 1], chars[i + 2]].iter().collect();
            if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                bytes.push(byte);
            }
            i += 3;
        } else {
            bytes.push(chars[i] as u8);
            i += 1;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_ascii() {
        assert_eq!(
            parse_mixed_pattern("Hello"),
            vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]
        );
    }

    #[test]
    fn pure_hex() {
        assert_eq!(parse_mixed_pattern("$0D$0A"), vec![0x0D, 0x0A]);
    }

    #[test]
    fn mixed_ascii_hex() {
        assert_eq!(
            parse_mixed_pattern("OK$0D$0A"),
            vec![0x4F, 0x4B, 0x0D, 0x0A]
        );
    }

    #[test]
    fn hex_case_insensitive() {
        assert_eq!(parse_mixed_pattern("$0d$Ff"), vec![0x0D, 0xFF]);
    }

    #[test]
    fn invalid_hex_falls_back_to_literal() {
        // $XX は非16進なのでリテラル '$', 'X', 'X' として扱われる
        let r = parse_mixed_pattern("$XX");
        assert_eq!(r, vec![b'$', b'X', b'X']);
    }

    #[test]
    fn short_hex_at_end_is_literal() {
        // 末尾の $0 は桁不足なのでリテラル
        let r = parse_mixed_pattern("A$0");
        assert_eq!(r, vec![b'A', b'$', b'0']);
    }

    #[test]
    fn empty_input() {
        assert!(parse_mixed_pattern("").is_empty());
    }

    // --- parse_idle_condition ---

    #[test]
    fn idle_any() {
        let cond = parse_idle_condition("@IDLE").unwrap();
        assert!(matches!(cond, IdleCondition::Any));
        assert!(cond.matches(0.0));
        assert!(cond.matches(999.0));
    }

    #[test]
    fn idle_case_insensitive() {
        assert!(parse_idle_condition("@idle").is_some());
        assert!(parse_idle_condition("@Idle").is_some());
        assert!(parse_idle_condition("@IDLE").is_some());
    }

    #[test]
    fn idle_gt() {
        let cond = parse_idle_condition("@IDLE>100").unwrap();
        assert!(!cond.matches(100.0));
        assert!(!cond.matches(100.9)); // floor(100.9) = 100, not > 100
        assert!(cond.matches(101.0));
    }

    #[test]
    fn idle_ge() {
        let cond = parse_idle_condition("@IDLE>=100").unwrap();
        assert!(cond.matches(100.0));
        assert!(cond.matches(100.9));
        assert!(!cond.matches(99.9)); // floor(99.9) = 99
    }

    #[test]
    fn idle_lt() {
        let cond = parse_idle_condition("@IDLE<50").unwrap();
        assert!(cond.matches(49.9)); // floor(49.9) = 49
        assert!(!cond.matches(50.0));
        assert!(!cond.matches(50.9)); // floor(50.9) = 50
    }

    #[test]
    fn idle_le() {
        let cond = parse_idle_condition("@IDLE<=50").unwrap();
        assert!(cond.matches(50.0));
        assert!(cond.matches(50.9)); // floor(50.9) = 50
        assert!(!cond.matches(51.0));
    }

    #[test]
    fn idle_eq() {
        let cond = parse_idle_condition("@IDLE=200").unwrap();
        assert!(cond.matches(200.0));
        assert!(cond.matches(200.9)); // floor(200.9) = 200
        assert!(!cond.matches(201.0));
    }

    #[test]
    fn idle_range() {
        let cond = parse_idle_condition("@IDLE100-500").unwrap();
        assert!(cond.matches(100.0));
        assert!(cond.matches(300.0));
        assert!(cond.matches(500.0));
        assert!(cond.matches(500.9)); // floor(500.9) = 500
        assert!(!cond.matches(99.9)); // floor(99.9) = 99
        assert!(!cond.matches(501.0));
    }

    #[test]
    fn idle_non_idle_returns_none() {
        assert!(parse_idle_condition("hello").is_none());
        assert!(parse_idle_condition("@abc").is_none());
        assert!(parse_idle_condition("$0D").is_none());
    }

    #[test]
    fn idle_invalid_suffix_returns_none() {
        assert!(parse_idle_condition("@IDLEabc").is_none());
    }

    // --- SearchState IDLE モード ---

    #[test]
    fn search_idle_finds_idle_entries() {
        let t = std::time::Instant::now();
        let entries = vec![
            DataEntry::Byte(0x01, t),
            DataEntry::Idle(50.0),
            DataEntry::Byte(0x02, t),
            DataEntry::Idle(200.0),
            DataEntry::Byte(0x03, t),
        ];
        let mut state = SearchState::new();
        state.query = "@IDLE".to_string();
        state.search(&entries);
        assert_eq!(state.result_count(), 2);
        assert!(state.is_highlighted(1));
        assert!(state.is_highlighted(3));
    }

    #[test]
    fn search_idle_with_condition() {
        let entries = vec![
            DataEntry::Idle(50.0),
            DataEntry::Idle(200.0),
            DataEntry::Idle(500.0),
        ];
        let mut state = SearchState::new();
        state.query = "@IDLE>100".to_string();
        state.search(&entries);
        assert_eq!(state.result_count(), 2);
        assert!(!state.is_highlighted(0));
        assert!(state.is_highlighted(1));
        assert!(state.is_highlighted(2));
    }

    #[test]
    fn search_idle_navigate() {
        let t = std::time::Instant::now();
        let entries = vec![
            DataEntry::Byte(0x01, t),
            DataEntry::Idle(50.0),
            DataEntry::Byte(0x02, t),
            DataEntry::Idle(200.0),
            DataEntry::Byte(0x03, t),
            DataEntry::Idle(300.0),
        ];
        let mut state = SearchState::new();
        state.query = "@IDLE".to_string();
        state.search(&entries);
        assert_eq!(state.result_count(), 3);
        assert_eq!(state.current_index(), 0);
        // current_highlight は最初の IDLE (index=1)
        assert!(state.is_current_highlight(1));

        state.next(&entries);
        assert_eq!(state.current_index(), 1);
        assert!(state.is_current_highlight(3));
        assert!(!state.is_current_highlight(1));

        state.prev(&entries);
        assert_eq!(state.current_index(), 0);
        assert!(state.is_current_highlight(1));

        // ラップアラウンド
        state.prev(&entries);
        assert_eq!(state.current_index(), 2);
        assert!(state.is_current_highlight(5));
    }

    #[test]
    fn search_idle_auto_refresh() {
        let t = std::time::Instant::now();
        let mut entries = vec![DataEntry::Byte(0x01, t), DataEntry::Idle(100.0)];
        let mut state = SearchState::new();
        state.query = "@IDLE".to_string();
        state.search(&entries);
        assert_eq!(state.result_count(), 1);

        // バッファが増えた
        entries.push(DataEntry::Byte(0x02, t));
        entries.push(DataEntry::Idle(200.0));
        state.auto_refresh(&entries);
        assert_eq!(state.result_count(), 2);
    }

    #[test]
    fn search_idle_no_match() {
        let t = std::time::Instant::now();
        let entries = vec![DataEntry::Byte(0x01, t), DataEntry::Byte(0x02, t)];
        let mut state = SearchState::new();
        state.query = "@IDLE".to_string();
        state.search(&entries);
        assert_eq!(state.result_count(), 0);
    }

    #[test]
    fn idle_decimal_value() {
        let cond = parse_idle_condition("@IDLE>10.5").unwrap();
        assert!(cond.matches(11.0)); // floor(11) = 11 > 10.5
        assert!(!cond.matches(10.9)); // floor(10.9) = 10, not > 10.5
    }

    #[test]
    fn idle_whitespace_trimmed() {
        let cond = parse_idle_condition("  @IDLE  ").unwrap();
        assert!(matches!(cond, IdleCondition::Any));
    }

    #[test]
    fn idle_short_input_returns_none() {
        assert!(parse_idle_condition("@IDL").is_none());
        assert!(parse_idle_condition("@").is_none());
        assert!(parse_idle_condition("").is_none());
    }
}
