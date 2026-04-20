use crate::model::grid::DisplayCell;
use crate::protocol::definition::ProtocolFile;
use crate::protocol::engine::MatchedMessage;

/// 範囲選択の状態
pub struct Selection {
    /// 選択開始インデックス
    pub anchor: Option<usize>,
    /// 選択終了インデックス
    pub cursor: Option<usize>,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            anchor: None,
            cursor: None,
        }
    }

    pub fn range(&self) -> Option<(usize, usize)> {
        match (self.anchor, self.cursor) {
            (Some(a), Some(c)) => Some((a.min(c), a.max(c))),
            (Some(a), None) => Some((a, a)),
            _ => None,
        }
    }

    pub fn contains(&self, idx: usize) -> bool {
        match self.range() {
            Some((lo, hi)) => idx >= lo && idx <= hi,
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.cursor = None;
    }

    pub fn start(&mut self, idx: usize) {
        self.anchor = Some(idx);
        self.cursor = Some(idx);
    }

    pub fn extend(&mut self, idx: usize) {
        self.cursor = Some(idx);
    }
}

/// プロトコル選択（match ID ベース、trim 耐性あり）
pub struct IdSelection {
    pub anchor: Option<u64>,
    pub cursor: Option<u64>,
}

impl IdSelection {
    pub fn new() -> Self {
        Self {
            anchor: None,
            cursor: None,
        }
    }

    pub fn range(&self) -> Option<(u64, u64)> {
        match (self.anchor, self.cursor) {
            (Some(a), Some(c)) => Some((a.min(c), a.max(c))),
            (Some(a), None) => Some((a, a)),
            _ => None,
        }
    }

    pub fn contains(&self, id: u64) -> bool {
        match self.range() {
            Some((lo, hi)) => id >= lo && id <= hi,
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.cursor = None;
    }

    pub fn start(&mut self, id: u64) {
        self.anchor = Some(id);
        self.cursor = Some(id);
    }

    pub fn extend(&mut self, id: u64) {
        self.cursor = Some(id);
    }
}

// ===== モニタビュー用コピーフォーマッタ =====

/// 混合形式: 印字可能ASCIIはそのまま、それ以外は $HH
pub fn format_monitor_mixed(cells: &[DisplayCell], range: (usize, usize)) -> String {
    let mut out = String::new();
    let mut in_idle = false;
    for cell in &cells[range.0..=range.1.min(cells.len() - 1)] {
        match cell {
            DisplayCell::Data(b) | DisplayCell::Sent(b) => {
                if in_idle {
                    out.push_str("[IDLE]");
                    in_idle = false;
                }
                if *b >= 0x21 && *b <= 0x7E {
                    out.push(*b as char);
                } else {
                    out.push_str(&format!("${:02X}", b));
                }
            }
            DisplayCell::IdleChar(_) => {
                in_idle = true;
            }
        }
    }
    if in_idle {
        out.push_str("[IDLE]");
    }
    out
}

/// 全HEX形式: 全バイトを $HH
pub fn format_monitor_hex(cells: &[DisplayCell], range: (usize, usize)) -> String {
    let mut out = String::new();
    let mut in_idle = false;
    for cell in &cells[range.0..=range.1.min(cells.len() - 1)] {
        match cell {
            DisplayCell::Data(b) | DisplayCell::Sent(b) => {
                if in_idle {
                    out.push_str("[IDLE]");
                    in_idle = false;
                }
                out.push_str(&format!("${:02X}", b));
            }
            DisplayCell::IdleChar(_) => {
                in_idle = true;
            }
        }
    }
    if in_idle {
        out.push_str("[IDLE]");
    }
    out
}

/// バイナリ形式: 生バイト列を文字列としてコピー（IDLEは除外）
pub fn format_monitor_binary(cells: &[DisplayCell], range: (usize, usize)) -> String {
    let bytes: Vec<u8> = cells[range.0..=range.1.min(cells.len() - 1)]
        .iter()
        .filter_map(|cell| match cell {
            DisplayCell::Data(b) | DisplayCell::Sent(b) => Some(*b),
            DisplayCell::IdleChar(_) => None,
        })
        .collect();
    // 生バイト列をそのまま文字列に（Latin-1的変換）
    bytes.iter().map(|&b| b as char).collect()
}

// ===== プロトコルパネル用コピーフォーマッタ =====

/// プロトコルメッセージの構造化テキスト
pub fn format_protocol_copy(
    matches: &[MatchedMessage],
    proto: &ProtocolFile,
    selected_indices: &[usize],
) -> String {
    let mut out = String::new();
    for &idx in selected_indices {
        if idx >= matches.len() {
            continue;
        }
        let matched = &matches[idx];

        // IDLE行
        if let Some(idle_ms) = matched.preceding_idle_ms
            && !out.is_empty()
        {
            out.push_str(&format!("--- IDLE {}ms ---\n", idle_ms as u64));
        }

        // メッセージ行
        let title = match matched.message_def_idx {
            Some(def_idx) => proto.messages[def_idx].title.clone(),
            None => "Unmatched".to_string(),
        };

        // フィールド値を収集
        let mut fields = String::new();
        if let Some(def_idx) = matched.message_def_idx {
            let msg_def = &proto.messages[def_idx];
            for field in &msg_def.fields {
                let hex_val = super::protocol_panel::extract_hex(
                    &matched.frame.bytes,
                    field.offset,
                    field.size,
                );
                if !fields.is_empty() {
                    fields.push(' ');
                }
                fields.push_str(&format!("{}:{}", field.name, hex_val));
            }
        }

        // HEXダンプ
        let hex_dump =
            super::protocol_panel::extract_hex(&matched.frame.bytes, 0, matched.frame.bytes.len());

        if fields.is_empty() {
            out.push_str(&format!("[{:03}] {} | {}\n", idx, title, hex_dump));
        } else {
            out.push_str(&format!(
                "[{:03}] {}  {} | {}\n",
                idx, title, fields, hex_dump
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(bytes: &[u8]) -> Vec<DisplayCell> {
        bytes.iter().map(|b| DisplayCell::Data(*b)).collect()
    }

    #[test]
    fn selection_range_normalizes_backward() {
        let mut s = Selection::new();
        s.start(10);
        s.extend(3);
        assert_eq!(s.range(), Some((3, 10)));
        assert!(s.contains(3));
        assert!(s.contains(7));
        assert!(s.contains(10));
        assert!(!s.contains(11));
    }

    #[test]
    fn selection_anchor_only() {
        let mut s = Selection::new();
        s.start(5);
        assert_eq!(s.range(), Some((5, 5)));
        s.clear();
        assert_eq!(s.range(), None);
    }

    #[test]
    fn id_selection_normalizes() {
        let mut s = IdSelection::new();
        s.start(100);
        s.extend(50);
        assert_eq!(s.range(), Some((50, 100)));
        assert!(s.contains(75));
        assert!(!s.contains(49));
    }

    #[test]
    fn format_monitor_mixed_uses_dollar_for_non_printable() {
        // 'A' (0x41) はそのまま、0x0D は $0D、0x0A は $0A
        let c = cells(&[0x41, 0x0D, 0x0A, b'B']);
        let s = format_monitor_mixed(&c, (0, 3));
        assert_eq!(s, "A$0D$0AB");
    }

    #[test]
    fn format_monitor_hex_emits_all_bytes_as_dollar() {
        let c = cells(&[0xFF, 0x00, 0x42]);
        let s = format_monitor_hex(&c, (0, 2));
        assert_eq!(s, "$FF$00$42");
    }

    #[test]
    fn format_monitor_binary_strips_idle_chars() {
        let mut c = cells(b"AB");
        c.insert(1, DisplayCell::IdleChar('0'));
        c.insert(2, DisplayCell::IdleChar('1'));
        let s = format_monitor_binary(&c, (0, c.len() - 1));
        assert_eq!(s, "AB");
    }

    #[test]
    fn format_monitor_mixed_treats_sent_as_data() {
        // 送信バイトもコピー時は受信と同じ扱い (色は描画側でのみ区別)
        let c = vec![
            DisplayCell::Data(b'A'),
            DisplayCell::Sent(b'B'),
            DisplayCell::Data(0x0D),
        ];
        let s = format_monitor_mixed(&c, (0, 2));
        assert_eq!(s, "AB$0D");
    }

    #[test]
    fn format_monitor_mixed_inserts_idle_marker() {
        let c = vec![
            DisplayCell::Data(b'A'),
            DisplayCell::IdleChar('0'),
            DisplayCell::IdleChar('0'),
            DisplayCell::IdleChar('0'),
            DisplayCell::IdleChar('1'),
            DisplayCell::Data(b'B'),
        ];
        let s = format_monitor_mixed(&c, (0, 5));
        assert_eq!(s, "A[IDLE]B");
    }
}
