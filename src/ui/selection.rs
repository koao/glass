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
            DisplayCell::Data(b) => {
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
            DisplayCell::Data(b) => {
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
            DisplayCell::Data(b) => Some(*b),
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
        if let Some(idle_ms) = matched.preceding_idle_ms {
            if !out.is_empty() {
                out.push_str(&format!("--- IDLE {}ms ---\n", idle_ms as u64));
            }
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
