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
    processed_count: usize,
}

impl DisplayBuffer {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            processed_count: 0,
        }
    }

    /// MonitorBufferの新しいエントリを同期
    pub fn sync_entries(&mut self, entries: &[DataEntry], idle_threshold_ms: f64) {
        if self.processed_count > entries.len() {
            self.cells.clear();
            self.processed_count = 0;
        }
        for entry in entries[self.processed_count..].iter() {
            self.push_entry_internal(entry, idle_threshold_ms);
        }
        self.processed_count = entries.len();
    }

    fn push_entry_internal(&mut self, entry: &DataEntry, idle_threshold_ms: f64) {
        match entry {
            DataEntry::Byte(b, _) => {
                self.cells.push(DisplayCell::Data(*b));
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
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.processed_count = 0;
    }

    pub fn cells(&self) -> &[DisplayCell] {
        &self.cells
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }
}
