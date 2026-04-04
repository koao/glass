use std::collections::HashSet;

use crate::model::entry::DataEntry;

/// 検索モード
#[derive(Clone, Debug, PartialEq)]
pub enum SearchMode {
    Hex,
    Ascii,
}

/// 検索状態
pub struct SearchState {
    /// 検索クエリ文字列
    pub query: String,
    /// 検索モード
    pub mode: SearchMode,
    /// 検索結果（一致開始位置のバイトインデックスリスト）
    pub results: Vec<usize>,
    /// 現在選択中の結果インデックス
    pub current: usize,
    /// ハイライト対象のエントリインデックス集合
    pub highlighted_indices: HashSet<usize>,
    /// 現在選択中のハイライト対象
    pub current_highlighted: HashSet<usize>,
    /// 検索パターン（パース済みバイト列）
    pattern: Vec<u8>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchMode::Hex,
            results: Vec::new(),
            current: 0,
            highlighted_indices: HashSet::new(),
            current_highlighted: HashSet::new(),
            pattern: Vec::new(),
        }
    }

    /// クエリ文字列からパターンバイト列をパース
    fn update_pattern(&mut self) {
        self.pattern = match self.mode {
            SearchMode::Hex => parse_hex_pattern(&self.query),
            SearchMode::Ascii => self.query.as_bytes().to_vec(),
        };
    }

    /// バッファ内を検索して結果を更新
    pub fn search(&mut self, entries: &[DataEntry]) {
        self.update_pattern();
        self.results.clear();
        self.highlighted_indices.clear();
        self.current_highlighted.clear();
        self.current = 0;

        if self.pattern.is_empty() {
            return;
        }

        // Byteエントリのみ抽出: (エントリ配列インデックス, バイト値)
        let byte_entries: Vec<(usize, u8)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                if let DataEntry::Byte(b, _) = e {
                    Some((i, *b))
                } else {
                    None
                }
            })
            .collect();

        // パターンマッチ（スライディングウィンドウ）
        let pat_len = self.pattern.len();
        if byte_entries.len() < pat_len {
            return;
        }

        for i in 0..=(byte_entries.len() - pat_len) {
            let matches = (0..pat_len).all(|j| byte_entries[i + j].1 == self.pattern[j]);
            if matches {
                self.results.push(i);
                // 一致した各バイトのエントリインデックスをハイライト集合に追加
                for j in 0..pat_len {
                    self.highlighted_indices.insert(byte_entries[i + j].0);
                }
            }
        }

        self.update_current_highlight(&byte_entries);
    }

    /// 現在選択の結果ハイライトを更新
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

    /// 次の検索結果に移動（byte_entries再構築が必要）
    pub fn next(&mut self, entries: &[DataEntry]) {
        if !self.results.is_empty() {
            self.current = (self.current + 1) % self.results.len();
            let byte_entries: Vec<(usize, u8)> = entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    if let DataEntry::Byte(b, _) = e {
                        Some((i, *b))
                    } else {
                        None
                    }
                })
                .collect();
            self.update_current_highlight(&byte_entries);
        }
    }

    /// 前の検索結果に移動
    pub fn prev(&mut self, entries: &[DataEntry]) {
        if !self.results.is_empty() {
            self.current = if self.current == 0 {
                self.results.len() - 1
            } else {
                self.current - 1
            };
            let byte_entries: Vec<(usize, u8)> = entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| {
                    if let DataEntry::Byte(b, _) = e {
                        Some((i, *b))
                    } else {
                        None
                    }
                })
                .collect();
            self.update_current_highlight(&byte_entries);
        }
    }

    pub fn result_count(&self) -> usize {
        self.results.len()
    }
}

/// HEX文字列をバイト列にパース（スペース区切り対応）
/// 例: "02 03 FF" -> [0x02, 0x03, 0xFF]
fn parse_hex_pattern(input: &str) -> Vec<u8> {
    let cleaned: String = input.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut bytes = Vec::new();
    let mut chars = cleaned.chars();
    while let (Some(hi), Some(lo)) = (chars.next(), chars.next()) {
        if let Ok(byte) = u8::from_str_radix(&format!("{}{}", hi, lo), 16) {
            bytes.push(byte);
        }
    }
    bytes
}
