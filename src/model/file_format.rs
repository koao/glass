use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::entry::DataEntry;

/// ファイル保存用のエントリ（シリアライズ可能）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SavedEntry {
    /// 受信バイト (値, 先頭バイトからの相対時間μs)
    Byte(u8, u64),
    /// アイドル検出 (持続時間ms)
    Idle(f64),
}

/// Glass モニタファイルフォーマット (.glm)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlassFile {
    /// フォーマットバージョン
    pub version: u32,
    /// 保存時刻 (ISO 8601 ローカル)
    pub saved_at: String,
    /// エントリ一覧
    pub entries: Vec<SavedEntry>,
}

impl GlassFile {
    /// MonitorBuffer のエントリからファイルデータを生成（1パス）
    pub fn from_entries(entries: &[DataEntry]) -> Self {
        let mut t0: Option<Instant> = None;
        let saved = entries
            .iter()
            .map(|e| match e {
                DataEntry::Byte(val, ts) => {
                    let base = *t0.get_or_insert(*ts);
                    let rel = ts.duration_since(base).as_micros() as u64;
                    SavedEntry::Byte(*val, rel)
                }
                DataEntry::Idle(ms) => SavedEntry::Idle(*ms),
            })
            .collect();

        Self {
            version: 1,
            saved_at: unix_timestamp_string(),
            entries: saved,
        }
    }

    /// ファイルデータから DataEntry を復元（合成Instant使用）
    pub fn to_entries(&self) -> Vec<DataEntry> {
        let base = Instant::now();
        self.entries
            .iter()
            .map(|e| match e {
                SavedEntry::Byte(val, rel_us) => {
                    let ts = base + std::time::Duration::from_micros(*rel_us);
                    DataEntry::Byte(*val, ts)
                }
                SavedEntry::Idle(ms) => DataEntry::Idle(*ms),
            })
            .collect()
    }
}

/// 現在時刻をUnixタイムスタンプ文字列で取得
fn unix_timestamp_string() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", secs)
}
