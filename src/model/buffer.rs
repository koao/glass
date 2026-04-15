use super::entry::DataEntry;

/// バッファ上限
const MAX_ENTRIES: usize = 500_000;
/// 上限超過時に削除する割合
const TRIM_RATIO: f64 = 0.1;

/// モニタバッファ: 全受信データを保持
pub struct MonitorBuffer {
    entries: Vec<DataEntry>,
    byte_count: usize,
    error_count: usize,
    /// 累積削除エントリ数（下流の sync_entries が trim 差分を検出するために使う）
    trimmed_total: usize,
}

impl MonitorBuffer {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            byte_count: 0,
            error_count: 0,
            trimmed_total: 0,
        }
    }

    /// エントリを追加（バッファ上限管理あり）
    pub fn push(&mut self, entry: DataEntry) {
        match &entry {
            DataEntry::Byte(..) => self.byte_count += 1,
            DataEntry::Error => self.error_count += 1,
            _ => {}
        }
        self.entries.push(entry);
        // 上限超過時に古いエントリを削除
        if self.entries.len() > MAX_ENTRIES {
            let trim_count = (MAX_ENTRIES as f64 * TRIM_RATIO) as usize;
            self.entries.drain(..trim_count);
            self.trimmed_total += trim_count;
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.byte_count = 0;
        self.error_count = 0;
        self.trimmed_total = 0;
    }

    pub fn entries(&self) -> &[DataEntry] {
        &self.entries
    }

    pub fn byte_count(&self) -> usize {
        self.byte_count
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }

    pub fn trimmed_total(&self) -> usize {
        self.trimmed_total
    }

    /// 外部エントリ一括読み込み（ファイル読み込み用、上限トリミング済み）
    pub fn load_entries(&mut self, mut entries: Vec<DataEntry>) {
        self.clear();
        if entries.len() > MAX_ENTRIES {
            let trim = entries.len() - MAX_ENTRIES;
            entries.drain(..trim);
        }
        self.byte_count = entries
            .iter()
            .filter(|e| matches!(e, DataEntry::Byte(..)))
            .count();
        self.entries = entries;
    }
}
