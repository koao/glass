use std::collections::HashSet;

use crate::model::entry::DataEntry;

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
        }
    }

    /// バッファ内を検索して結果を更新
    pub fn search(&mut self, entries: &[DataEntry]) {
        self.pattern = parse_mixed_pattern(&self.query);
        self.has_searched = true;
        self.last_search_len = entries.len();
        self.current = 0;
        self.run_search(entries);
    }

    /// バッファが増えていれば自動再検索（受信中用）
    pub fn auto_refresh(&mut self, entries: &[DataEntry]) {
        if self.has_searched && !self.pattern.is_empty() && entries.len() != self.last_search_len {
            self.last_search_len = entries.len();
            let prev_current = self.current;
            self.run_search(entries);
            // 現在位置を維持（範囲内なら）
            if !self.results.is_empty() {
                self.current = prev_current.min(self.results.len() - 1);
                let byte_entries = collect_byte_entries(entries);
                self.update_current_highlight(&byte_entries);
            }
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
        let byte_entries = collect_byte_entries(entries);
        self.update_current_highlight(&byte_entries);
        let start = self.results[self.current];
        if start < byte_entries.len() {
            self.scroll_to_entry = Some(byte_entries[start].0);
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
fn parse_mixed_pattern(input: &str) -> Vec<u8> {
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
    use super::parse_mixed_pattern;

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
}
