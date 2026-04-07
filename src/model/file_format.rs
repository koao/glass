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
            .filter_map(|e| match e {
                DataEntry::Byte(val, ts) => {
                    let base = *t0.get_or_insert(*ts);
                    let rel = ts.duration_since(base).as_micros() as u64;
                    Some(SavedEntry::Byte(*val, rel))
                }
                DataEntry::Idle(ms) => Some(SavedEntry::Idle(*ms)),
                DataEntry::Error => None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn roundtrip_preserves_byte_values_and_idle() {
        let t0 = Instant::now();
        let entries = vec![
            DataEntry::Byte(0x12, t0),
            DataEntry::Byte(0x34, t0 + Duration::from_micros(50)),
            DataEntry::Idle(7.5),
            DataEntry::Byte(0xFF, t0 + Duration::from_micros(120)),
        ];
        let file = GlassFile::from_entries(&entries);
        let restored = file.to_entries();
        assert_eq!(restored.len(), 4);

        let extract = |e: &DataEntry| -> (Option<u8>, Option<f64>) {
            match e {
                DataEntry::Byte(b, _) => (Some(*b), None),
                DataEntry::Idle(ms) => (None, Some(*ms)),
                DataEntry::Error => (None, None),
            }
        };
        assert_eq!(extract(&restored[0]), (Some(0x12), None));
        assert_eq!(extract(&restored[1]), (Some(0x34), None));
        assert_eq!(extract(&restored[2]), (None, Some(7.5)));
        assert_eq!(extract(&restored[3]), (Some(0xFF), None));
    }

    #[test]
    fn errors_are_excluded_from_save() {
        let t0 = Instant::now();
        let entries = vec![
            DataEntry::Byte(0xAA, t0),
            DataEntry::Error,
            DataEntry::Byte(0xBB, t0 + Duration::from_micros(10)),
        ];
        let file = GlassFile::from_entries(&entries);
        assert_eq!(file.entries.len(), 2);
    }

    #[test]
    fn relative_time_starts_at_zero() {
        let t0 = Instant::now() + Duration::from_secs(100);
        let entries = vec![
            DataEntry::Byte(0x01, t0),
            DataEntry::Byte(0x02, t0 + Duration::from_micros(250)),
        ];
        let file = GlassFile::from_entries(&entries);
        match (&file.entries[0], &file.entries[1]) {
            (SavedEntry::Byte(_, a), SavedEntry::Byte(_, b)) => {
                assert_eq!(*a, 0);
                assert_eq!(*b, 250);
            }
            _ => panic!("expected Byte entries"),
        }
    }

    #[test]
    fn empty_buffer_roundtrip() {
        let file = GlassFile::from_entries(&[]);
        assert!(file.entries.is_empty());
        assert!(file.to_entries().is_empty());
    }

    #[test]
    fn json_serde_roundtrip() {
        let t0 = Instant::now();
        let entries = vec![DataEntry::Byte(0x42, t0), DataEntry::Idle(1.5)];
        let file = GlassFile::from_entries(&entries);
        let json = serde_json::to_string(&file).unwrap();
        let parsed: GlassFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.version, 1);
    }
}
