use super::entry::DataEntry;

/// モニタグリッド用の表示セル
#[derive(Clone, Debug)]
pub enum DisplayCell {
    /// データバイト
    Data(u8),
    /// IDLEカウンタの1文字（0埋め4桁の各桁）
    IdleChar(char),
}

/// 表示バッファ（DataEntryを表示セル列に変換して保持）
pub struct DisplayBuffer {
    cells: Vec<DisplayCell>,
    /// 各セルの元エントリインデックス
    entry_indices: Vec<usize>,
    processed_count: usize,
    /// MonitorBuffer から同期済みの累積 trim 数
    seen_trimmed: usize,
    /// 前方から drain されたセルの累積数（リングバッファ位置の単調増加を保つため）
    cells_trimmed: usize,
}

impl DisplayBuffer {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            entry_indices: Vec::new(),
            processed_count: 0,
            seen_trimmed: 0,
            cells_trimmed: 0,
        }
    }

    /// MonitorBufferの新しいエントリを同期
    pub fn sync_entries(
        &mut self,
        entries: &[DataEntry],
        idle_threshold_ms: f64,
        trimmed_total: usize,
    ) {
        // MonitorBuffer の trim に追随：古いエントリに対応するセルを drain してシフト
        let trim_delta = trimmed_total.saturating_sub(self.seen_trimmed);
        if trim_delta > 0 {
            let split = self
                .entry_indices
                .iter()
                .position(|&idx| idx >= trim_delta)
                .unwrap_or(self.entry_indices.len());
            self.cells.drain(..split);
            self.entry_indices.drain(..split);
            for idx in &mut self.entry_indices {
                *idx -= trim_delta;
            }
            self.processed_count = self.processed_count.saturating_sub(trim_delta);
            self.cells_trimmed += split;
            self.seen_trimmed = trimmed_total;
        }
        // 完全リセット検出
        if self.processed_count > entries.len() || trimmed_total < self.seen_trimmed {
            self.cells.clear();
            self.entry_indices.clear();
            self.processed_count = 0;
            self.cells_trimmed = 0;
            self.seen_trimmed = trimmed_total;
        }
        for (offset, entry) in entries[self.processed_count..].iter().enumerate() {
            let entry_idx = self.processed_count + offset;
            self.push_entry_internal(entry, idle_threshold_ms, entry_idx);
        }
        self.processed_count = entries.len();
    }

    fn push_entry_internal(&mut self, entry: &DataEntry, idle_threshold_ms: f64, entry_idx: usize) {
        match entry {
            DataEntry::Byte(b, _) => {
                self.cells.push(DisplayCell::Data(*b));
                self.entry_indices.push(entry_idx);
            }
            DataEntry::Idle(ms) => {
                // IDLE設定ごとに1加算、最大9999、0埋め4桁
                let count = if idle_threshold_ms > 0.0 {
                    ((*ms / idle_threshold_ms).floor() as u64).min(9999)
                } else {
                    9999
                };
                let text = format!("{:04}", count);
                for ch in text.chars() {
                    self.cells.push(DisplayCell::IdleChar(ch));
                    self.entry_indices.push(entry_idx);
                }
            }
            DataEntry::Error => {}
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.entry_indices.clear();
        self.processed_count = 0;
        self.seen_trimmed = 0;
        self.cells_trimmed = 0;
    }

    pub fn cells(&self) -> &[DisplayCell] {
        &self.cells
    }

    pub fn entry_indices(&self) -> &[usize] {
        &self.entry_indices
    }

    /// これまでに累計で書き込まれたセル数（単調増加・リングバッファ位置計算用）
    pub fn total_written(&self) -> usize {
        self.cells.len() + self.cells_trimmed
    }

    /// cells[0] の論理インデックス（= 先頭から drain された累計セル数）
    pub fn cells_offset(&self) -> usize {
        self.cells_trimmed
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn idle_chars(buf: &DisplayBuffer) -> String {
        buf.cells()
            .iter()
            .filter_map(|c| match c {
                DisplayCell::IdleChar(ch) => Some(*ch),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn byte_becomes_single_data_cell() {
        let mut buf = DisplayBuffer::new();
        buf.sync_entries(&[DataEntry::Byte(0xAB, Instant::now())], 100.0, 0);
        assert_eq!(buf.len(), 1);
        assert!(matches!(buf.cells()[0], DisplayCell::Data(0xAB)));
    }

    #[test]
    fn idle_count_zero_padded_4digits() {
        let mut buf = DisplayBuffer::new();
        // 350ms / 100ms = 3
        buf.sync_entries(&[DataEntry::Idle(350.0)], 100.0, 0);
        assert_eq!(buf.len(), 4);
        assert_eq!(idle_chars(&buf), "0003");
    }

    #[test]
    fn idle_count_clamped_to_9999() {
        let mut buf = DisplayBuffer::new();
        buf.sync_entries(&[DataEntry::Idle(1.0e9)], 1.0, 0);
        assert_eq!(idle_chars(&buf), "9999");
    }

    #[test]
    fn incremental_sync_only_processes_new() {
        let mut buf = DisplayBuffer::new();
        let t = Instant::now();
        let entries = vec![DataEntry::Byte(0x01, t), DataEntry::Byte(0x02, t)];
        buf.sync_entries(&entries, 100.0, 0);
        let mut entries2 = entries.clone();
        entries2.push(DataEntry::Byte(0x03, t));
        buf.sync_entries(&entries2, 100.0, 0);
        assert_eq!(buf.len(), 3);
        assert!(matches!(buf.cells()[2], DisplayCell::Data(0x03)));
    }

    #[test]
    fn shrunk_input_resets_buffer() {
        let mut buf = DisplayBuffer::new();
        let t = Instant::now();
        buf.sync_entries(
            &[DataEntry::Byte(0x01, t), DataEntry::Byte(0x02, t)],
            100.0,
            0,
        );
        buf.sync_entries(&[DataEntry::Byte(0xFF, t)], 100.0, 0);
        assert_eq!(buf.len(), 1);
        assert!(matches!(buf.cells()[0], DisplayCell::Data(0xFF)));
    }

    #[test]
    fn error_entry_produces_no_cell() {
        let mut buf = DisplayBuffer::new();
        buf.sync_entries(&[DataEntry::Error], 100.0, 0);
        assert_eq!(buf.len(), 0);
    }
}
