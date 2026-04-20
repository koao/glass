//! PC からの電文送信ルール
//!
//! 各ルールは「名前・送信データ (混在記法)・モード」の組で、
//! モードは Manual / Interval / OnReceive の 3 種類。
//! ルール一覧は JSON 形式で `glass_send_rules.json` に永続化する。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::model::entry::DataEntry;
use crate::trigger::PatternMatcher;
use crate::ui::search::parse_mixed_pattern;

/// 送信モード。serde では internally-tagged `"mode"` で区別し、未知の mode 値は
/// `#[serde(other)]` で Manual にフォールバックする。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SendMode {
    /// 定期送信 (period_ms 周期)
    Interval { period_ms: u64 },
    /// 受信トリガ送信 (パターン検出で 1 回送信)
    OnReceive { pattern_text: String },
    /// 手動送信のみ (UI のボタンで送信)
    #[serde(other)]
    Manual,
}

impl SendMode {
    pub fn kind(&self) -> SendModeKind {
        match self {
            SendMode::Manual => SendModeKind::Manual,
            SendMode::Interval { .. } => SendModeKind::Interval,
            SendMode::OnReceive { .. } => SendModeKind::OnReceive,
        }
    }
}

/// モード種別だけを表す enum (UI 切替時の比較用)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendModeKind {
    Manual,
    Interval,
    OnReceive,
}

/// 送信ルール (UI で編集・永続化される単位)
#[derive(Clone, Debug)]
pub struct SendRule {
    pub name: String,
    /// 送信データの混在記法テキスト (UI 編集用)
    pub data_text: String,
    /// 有効フラグ (チェックで Interval/OnReceive が作動)
    pub enabled: bool,
    /// モード
    pub mode: SendMode,

    /// data_text から解析済みのバイト列 (永続化しない)
    bytes: Vec<u8>,
    /// Interval モードの次回送信予定 (永続化しない)
    next_send_at: Option<Instant>,
    /// OnReceive モードのパターンマッチ状態 (永続化しない)
    matcher: PatternMatcher,
}

impl SendRule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data_text: String::new(),
            enabled: false,
            mode: SendMode::Manual,
            bytes: Vec::new(),
            next_send_at: None,
            matcher: PatternMatcher::new(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// data_text から bytes を再計算する (UI で編集された直後に呼ぶ)
    pub fn refresh_bytes(&mut self) {
        self.bytes = parse_mixed_pattern(&self.data_text);
    }

    /// OnReceive モードのパターンを設定し、matcher を再構築する
    pub fn refresh_on_receive_pattern(&mut self) {
        if let SendMode::OnReceive { pattern_text } = &self.mode {
            self.matcher.set_pattern_text(pattern_text);
        } else {
            self.matcher = PatternMatcher::new();
        }
    }

    /// Interval モードのタイマー/スキャン状態をリセット (clear_all などで呼ぶ)
    pub fn reset_execution_state(&mut self, entries_len: usize) {
        self.next_send_at = None;
        self.matcher.reset(entries_len);
    }

    /// Interval モードで「今送信すべきか」判定し、送信すべきならバイト列を返す
    pub fn tick_interval(&mut self, now: Instant) -> Option<Vec<u8>> {
        let SendMode::Interval { period_ms } = &self.mode else {
            return None;
        };
        if self.bytes.is_empty() || *period_ms == 0 {
            return None;
        }
        let due = match self.next_send_at {
            Some(t) => t,
            None => {
                // 有効化直後は「次の周期タイミングで初回送信」
                self.next_send_at = Some(now + Duration::from_millis(*period_ms));
                return None;
            }
        };
        if now >= due {
            self.next_send_at = Some(due + Duration::from_millis(*period_ms));
            Some(self.bytes.clone())
        } else {
            None
        }
    }

    /// OnReceive モードで受信バッファをスキャンし、マッチしたらバイト列を返す
    pub fn scan_recv(&mut self, entries: &[DataEntry]) -> Option<Vec<u8>> {
        if !matches!(self.mode, SendMode::OnReceive { .. }) || self.bytes.is_empty() {
            return None;
        }
        if self.matcher.scan(entries) {
            Some(self.bytes.clone())
        } else {
            None
        }
    }

    pub fn to_persisted(&self) -> PersistedSendRule {
        // enabled は意図的に永続化しない (起動時は必ず OFF で始まる)
        PersistedSendRule {
            name: self.name.clone(),
            data_text: self.data_text.clone(),
            mode: self.mode.clone(),
        }
    }

    pub fn from_persisted(p: PersistedSendRule) -> Self {
        // enabled は保存しない仕様なので起動時は常に false
        let mut rule = Self {
            name: p.name,
            data_text: p.data_text,
            enabled: false,
            mode: p.mode,
            bytes: Vec::new(),
            next_send_at: None,
            matcher: PatternMatcher::new(),
        };
        rule.refresh_bytes();
        rule.refresh_on_receive_pattern();
        rule
    }
}

// ---------------------------------------------------------------------------
// 永続化
// ---------------------------------------------------------------------------

/// JSON ファイルに保存するルール表現 (enabled は保存しない)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedSendRule {
    pub name: String,
    pub data_text: String,
    #[serde(flatten)]
    pub mode: SendMode,
}

/// ルール一覧のファイル表現
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PersistedSendRules {
    #[serde(default)]
    pub rules: Vec<PersistedSendRule>,
}

/// 保存先: exe 隣の glass_send_rules.json
fn send_rules_path() -> PathBuf {
    crate::util::exe_dir().join("glass_send_rules.json")
}

/// ルール一覧を読み込む (失敗時は空)
pub fn load_send_rules() -> Vec<SendRule> {
    let path = send_rules_path();
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<PersistedSendRules>(&json) {
        Ok(p) => p.rules.into_iter().map(SendRule::from_persisted).collect(),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "送信ルールファイルの parse に失敗したため空リストを使用します"
            );
            Vec::new()
        }
    }
}

/// ルール一覧を保存する
pub fn save_send_rules(rules: &[SendRule]) {
    let path = send_rules_path();
    let dto = PersistedSendRules {
        rules: rules.iter().map(|r| r.to_persisted()).collect(),
    };
    let json = match serde_json::to_string_pretty(&dto) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!(error = %e, "送信ルールの serialize に失敗しました");
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, json) {
        tracing::error!(
            path = %path.display(),
            error = %e,
            "送信ルールファイルの書き込みに失敗しました"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte_entries(bs: &[u8]) -> Vec<DataEntry> {
        bs.iter()
            .map(|b| DataEntry::Byte(*b, Instant::now()))
            .collect()
    }

    // --- SendRule::refresh_bytes ---

    #[test]
    fn refresh_bytes_parses_mixed_pattern() {
        let mut rule = SendRule::new("r");
        rule.data_text = "OK$0D$0A".to_string();
        rule.refresh_bytes();
        assert_eq!(rule.bytes(), &[0x4F, 0x4B, 0x0D, 0x0A]);
    }

    // --- tick_interval ---

    #[test]
    fn tick_interval_returns_none_initially_then_fires_after_period() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$AA".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::Interval { period_ms: 100 };

        let t0 = Instant::now();
        // 初回: next_send_at が設定され、None
        assert!(rule.tick_interval(t0).is_none());
        // 99ms 後: まだ発火しない
        assert!(rule.tick_interval(t0 + Duration::from_millis(99)).is_none());
        // 100ms 後: 発火
        let payload = rule.tick_interval(t0 + Duration::from_millis(100));
        assert_eq!(payload.as_deref(), Some([0xAA].as_slice()));
        // 次の周期まで再び None
        assert!(
            rule.tick_interval(t0 + Duration::from_millis(150))
                .is_none()
        );
        // 200ms 後: 2 回目発火
        let payload2 = rule.tick_interval(t0 + Duration::from_millis(200));
        assert_eq!(payload2.as_deref(), Some([0xAA].as_slice()));
    }

    #[test]
    fn tick_interval_requires_interval_mode() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$AA".to_string();
        rule.refresh_bytes();
        assert!(rule.tick_interval(Instant::now()).is_none());
    }

    #[test]
    fn tick_interval_ignores_empty_bytes() {
        let mut rule = SendRule::new("r");
        rule.mode = SendMode::Interval { period_ms: 10 };
        assert!(rule.tick_interval(Instant::now()).is_none());
    }

    #[test]
    fn tick_interval_ignores_zero_period() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$01".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::Interval { period_ms: 0 };
        assert!(rule.tick_interval(Instant::now()).is_none());
    }

    // --- scan_recv ---

    #[test]
    fn scan_recv_fires_on_exact_match() {
        let mut rule = SendRule::new("r");
        rule.data_text = "ACK".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::OnReceive {
            pattern_text: "$06".to_string(),
        };
        rule.refresh_on_receive_pattern();

        let entries = byte_entries(&[0x01, 0x06, 0x99]);
        assert_eq!(rule.scan_recv(&entries).as_deref(), Some(b"ACK".as_slice()));
    }

    #[test]
    fn scan_recv_partial_match_continues_across_calls() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$00".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::OnReceive {
            pattern_text: "$02$AA".to_string(),
        };
        rule.refresh_on_receive_pattern();

        let mut buf = byte_entries(&[0x02]);
        assert!(rule.scan_recv(&buf).is_none());
        buf.extend(byte_entries(&[0xAA]));
        assert_eq!(rule.scan_recv(&buf).as_deref(), Some([0x00].as_slice()));
    }

    #[test]
    fn scan_recv_ignores_sent_and_idle_entries() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$FF".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::OnReceive {
            pattern_text: "$01$02".to_string(),
        };
        rule.refresh_on_receive_pattern();

        let entries = vec![
            DataEntry::Byte(0x01, Instant::now()),
            DataEntry::Sent(0xAA, Instant::now()),
            DataEntry::Idle(50.0),
            DataEntry::Byte(0x02, Instant::now()),
        ];
        assert!(rule.scan_recv(&entries).is_some());
    }

    #[test]
    fn scan_recv_resets_on_buffer_shrink() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$AA".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::OnReceive {
            pattern_text: "$BB".to_string(),
        };
        rule.refresh_on_receive_pattern();

        let entries = byte_entries(&[0x01, 0x02, 0x03]);
        let _ = rule.scan_recv(&entries);
        let empty: Vec<DataEntry> = Vec::new();
        let _ = rule.scan_recv(&empty);
        let next = byte_entries(&[0xBB]);
        assert!(rule.scan_recv(&next).is_some());
    }

    #[test]
    fn scan_recv_after_reset_skips_existing_entries() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$FF".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::OnReceive {
            pattern_text: "$AA".to_string(),
        };
        rule.refresh_on_receive_pattern();

        let mut buf = byte_entries(&[0xAA, 0xAA, 0xAA]);
        rule.reset_execution_state(buf.len());
        assert!(
            rule.scan_recv(&buf).is_none(),
            "有効化前の既存バイトには反応しない"
        );
        buf.extend(byte_entries(&[0xAA]));
        assert!(rule.scan_recv(&buf).is_some());
    }

    #[test]
    fn scan_recv_ignores_empty_pattern() {
        let mut rule = SendRule::new("r");
        rule.data_text = "$FF".to_string();
        rule.refresh_bytes();
        rule.mode = SendMode::OnReceive {
            pattern_text: String::new(),
        };
        rule.refresh_on_receive_pattern();
        let entries = byte_entries(&[0x01, 0x02]);
        assert!(rule.scan_recv(&entries).is_none());
    }

    // --- persistence roundtrip ---

    #[test]
    fn persistence_roundtrip_manual() {
        let mut rule = SendRule::new("Ping");
        rule.data_text = "OK$0D$0A".to_string();
        rule.enabled = true;
        let restored = SendRule::from_persisted(rule.to_persisted());
        assert_eq!(restored.name, "Ping");
        assert_eq!(restored.data_text, "OK$0D$0A");
        // enabled は保存しない仕様なので復元後は常に false
        assert!(!restored.enabled);
        assert_eq!(restored.mode, SendMode::Manual);
        assert_eq!(restored.bytes(), &[0x4F, 0x4B, 0x0D, 0x0A]);
    }

    #[test]
    fn persistence_does_not_retain_enabled_flag() {
        let mut rule = SendRule::new("AutoPoll");
        rule.data_text = "$AA".to_string();
        rule.mode = SendMode::Interval { period_ms: 100 };
        rule.enabled = true;
        let restored = SendRule::from_persisted(rule.to_persisted());
        assert!(!restored.enabled);
    }

    #[test]
    fn persistence_roundtrip_interval() {
        let mut rule = SendRule::new("Poll");
        rule.data_text = "$02$10$03".to_string();
        rule.mode = SendMode::Interval { period_ms: 500 };
        let restored = SendRule::from_persisted(rule.to_persisted());
        assert_eq!(restored.mode, SendMode::Interval { period_ms: 500 });
        assert_eq!(restored.bytes(), &[0x02, 0x10, 0x03]);
    }

    #[test]
    fn persistence_roundtrip_on_receive() {
        let mut rule = SendRule::new("Ack");
        rule.data_text = "ACK".to_string();
        rule.mode = SendMode::OnReceive {
            pattern_text: "$06".to_string(),
        };
        let restored = SendRule::from_persisted(rule.to_persisted());
        assert_eq!(
            restored.mode,
            SendMode::OnReceive {
                pattern_text: "$06".to_string()
            }
        );
        assert_eq!(restored.matcher.pattern, vec![0x06]);
    }

    #[test]
    fn persistence_unknown_mode_falls_back_to_manual() {
        // serde(other) により未知の mode は Manual に落ちる
        let json = r#"{"name":"X","data_text":"A","mode":"unknown"}"#;
        let p: PersistedSendRule = serde_json::from_str(json).unwrap();
        let restored = SendRule::from_persisted(p);
        assert_eq!(restored.mode, SendMode::Manual);
    }

    #[test]
    fn persistence_json_roundtrip() {
        let rules = [
            {
                let mut r = SendRule::new("A");
                r.data_text = "$01".to_string();
                r.mode = SendMode::Interval { period_ms: 100 };
                r
            },
            {
                let mut r = SendRule::new("B");
                r.data_text = "ACK".to_string();
                r.mode = SendMode::OnReceive {
                    pattern_text: "$06".to_string(),
                };
                r
            },
        ];
        let dto = PersistedSendRules {
            rules: rules.iter().map(|r| r.to_persisted()).collect(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        let parsed: PersistedSendRules = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.rules.len(), 2);
        assert_eq!(parsed.rules[0].mode, SendMode::Interval { period_ms: 100 });
        assert_eq!(
            parsed.rules[1].mode,
            SendMode::OnReceive {
                pattern_text: "$06".to_string()
            }
        );
    }
}
