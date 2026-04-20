//! バイト列パターン検出によるトリガ機能
//!
//! 受信バイトストリームから特定のバイト列パターンを検出し、
//! 任意の追加バイト数を受信した後にワンショット発火する。
//! パターン記法は検索バーと同じ混在形式（`$XX` で HEX、それ以外は ASCII）。

use std::time::Instant;

use crate::model::entry::DataEntry;
use crate::ui::search::parse_mixed_pattern;

/// バイト列パターンの部分一致追跡器 (汎用)。
///
/// `DataEntry::Byte` のみを対象にし、`Idle` / `Error` / `Sent` はスキップする。
/// `ByteTrigger` (pause 用) と送信ルールの受信トリガで共有される共通基盤。
#[derive(Clone, Debug, Default)]
pub struct PatternMatcher {
    /// 検出するバイト列
    pub pattern: Vec<u8>,
    /// 部分一致進行度
    matched_len: usize,
    /// 処理済み entries 件数
    last_scanned: usize,
}

impl PatternMatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_pattern(&mut self, pattern: Vec<u8>) {
        self.pattern = pattern;
        self.matched_len = 0;
    }

    pub fn set_pattern_text(&mut self, text: &str) {
        self.set_pattern(parse_mixed_pattern(text));
    }

    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }

    /// スキャンカーソルと部分一致状態をリセットする。
    /// `entries_len` を指定すると「以降の新規受信」のみがマッチ対象になる。
    pub fn reset(&mut self, entries_len: usize) {
        self.last_scanned = entries_len;
        self.matched_len = 0;
    }

    /// 部分一致進行度だけリセット (スキャンカーソルは保持)。
    pub fn reset_progress(&mut self) {
        self.matched_len = 0;
    }

    /// 1 バイト供給。完全一致したら true。
    pub fn feed_byte(&mut self, b: u8) -> bool {
        loop {
            if self.pattern[self.matched_len] == b {
                self.matched_len += 1;
                if self.matched_len == self.pattern.len() {
                    self.matched_len = 0;
                    return true;
                }
                return false;
            }
            if self.matched_len == 0 {
                return false;
            }
            // 厳密な KMP ではなく単純巻き戻し (短パターン前提で実用上十分)
            self.matched_len -= 1;
        }
    }

    /// 新規 entries (`Byte` のみ) をスキャンし、一致したら true。
    /// 一致は 1 回ごとに返り、`last_scanned` は進む。
    pub fn scan(&mut self, entries: &[DataEntry]) -> bool {
        if self.pattern.is_empty() {
            self.last_scanned = entries.len();
            return false;
        }
        if self.last_scanned > entries.len() {
            self.reset(entries.len());
        }
        let start = self.last_scanned;
        for entry in entries.iter().skip(start) {
            self.last_scanned += 1;
            let DataEntry::Byte(b, _) = entry else {
                continue;
            };
            if self.feed_byte(*b) {
                return true;
            }
        }
        false
    }
}

/// バイト列パターン検出トリガ (pause 用、ワンショット + 遅延付き)
pub struct ByteTrigger {
    /// 共通のパターンマッチ器
    pub matcher: PatternMatcher,
    /// UI 編集用テキスト（"OK$0D$0A" など）
    pub pattern_text: String,
    /// マッチ後の停止遅延 (ms)。0 ならマッチ即発火
    pub post_match_delay_ms: u64,
    /// true なら監視中
    pub armed: bool,
    /// マッチが成立した時刻。Some なら遅延カウント中
    match_time: Option<Instant>,
}

impl Default for ByteTrigger {
    fn default() -> Self {
        Self::new()
    }
}

impl ByteTrigger {
    pub fn new() -> Self {
        Self {
            matcher: PatternMatcher::new(),
            pattern_text: String::new(),
            post_match_delay_ms: 0,
            armed: false,
            match_time: None,
        }
    }

    /// テキストからパターンを更新（検索バーと同じ混在記法）
    pub fn set_pattern_text(&mut self, text: String) {
        self.pattern_text = text;
        self.matcher.set_pattern_text(&self.pattern_text);
        if self.matcher.is_empty() {
            self.armed = false;
        }
    }

    pub fn is_pattern_empty(&self) -> bool {
        self.matcher.is_empty()
    }

    /// 現在のバッファ末尾位置からアームする
    pub fn arm_from(&mut self, current_entries_len: usize) {
        if self.matcher.is_empty() {
            return;
        }
        self.armed = true;
        self.matcher.reset(current_entries_len);
        self.match_time = None;
    }

    pub fn disarm(&mut self) {
        self.armed = false;
        self.matcher.reset_progress();
        self.match_time = None;
    }

    /// バッファクリア時などにスキャンカーソルをリセット
    pub fn reset_scan_cursor(&mut self, len: usize) {
        self.matcher.reset(len);
        self.match_time = None;
    }

    /// 新規 entries をスキャン。発火したら true（ワンショット）
    pub fn scan(&mut self, entries: &[DataEntry]) -> bool {
        self.scan_at(entries, Instant::now())
    }

    /// テスト用に「現在時刻」を注入できる scan
    fn scan_at(&mut self, entries: &[DataEntry], now: Instant) -> bool {
        if !self.armed || self.matcher.is_empty() {
            return false;
        }
        // 遅延カウント中: 経過時間のみ確認（バイトはこれ以上見ない）
        if let Some(t) = self.match_time {
            if now.saturating_duration_since(t).as_millis() as u64 >= self.post_match_delay_ms {
                self.fire();
                return true;
            }
            // 遅延中は新着バイトを処理対象から外しておく
            self.matcher.reset(entries.len());
            return false;
        }
        if self.matcher.scan(entries) {
            if self.post_match_delay_ms == 0 {
                self.fire();
                return true;
            }
            self.match_time = Some(now);
            return false;
        }
        false
    }

    fn fire(&mut self) {
        self.armed = false;
        self.matcher.reset_progress();
        self.match_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn t() -> Instant {
        Instant::now()
    }

    fn bytes(bs: &[u8]) -> Vec<DataEntry> {
        bs.iter().map(|b| DataEntry::Byte(*b, t())).collect()
    }

    #[test]
    fn parses_mixed_pattern() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("OK$0D$0A".to_string());
        assert_eq!(trig.matcher.pattern, vec![0x4F, 0x4B, 0x0D, 0x0A]);
    }

    #[test]
    fn fires_on_exact_match() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$02$AA".to_string());
        trig.arm_from(0);
        let entries = bytes(&[0x01, 0x02, 0xAA, 0x99]);
        assert!(trig.scan(&entries));
        assert!(!trig.armed, "ワンショットで disarm されるはず");
    }

    #[test]
    fn no_match_keeps_armed() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$02$AA".to_string());
        trig.arm_from(0);
        let entries = bytes(&[0x01, 0x02, 0x03]);
        assert!(!trig.scan(&entries));
        assert!(trig.armed);
    }

    #[test]
    fn partial_match_resumes_across_calls() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$02$AA$BB".to_string());
        trig.arm_from(0);
        let mut buf = bytes(&[0x02, 0xAA]);
        assert!(!trig.scan(&buf));
        buf.extend(bytes(&[0xBB]));
        assert!(trig.scan(&buf));
    }

    #[test]
    fn rewinds_on_mismatch() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$02$02$AA".to_string());
        trig.arm_from(0);
        let entries = bytes(&[0x02, 0x02, 0x02, 0xAA]);
        assert!(trig.scan(&entries));
    }

    #[test]
    fn idle_does_not_reset_partial_match() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$02$AA".to_string());
        trig.arm_from(0);
        let entries = vec![
            DataEntry::Byte(0x02, t()),
            DataEntry::Idle(50.0),
            DataEntry::Byte(0xAA, t()),
        ];
        assert!(trig.scan(&entries));
    }

    #[test]
    fn fires_only_once() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$AA".to_string());
        trig.arm_from(0);
        let entries = bytes(&[0xAA, 0xAA]);
        assert!(trig.scan(&entries));
        let more = bytes(&[0xAA, 0xAA, 0xAA, 0xAA]);
        assert!(!trig.scan(&more));
    }

    #[test]
    fn empty_pattern_cannot_arm() {
        let mut trig = ByteTrigger::new();
        trig.arm_from(0);
        assert!(!trig.armed);
    }

    #[test]
    fn shrunk_buffer_resets_cursor() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$AA".to_string());
        trig.arm_from(0);
        let entries = bytes(&[0x01, 0x02]);
        let _ = trig.scan(&entries);
        let small: Vec<DataEntry> = Vec::new();
        let _ = trig.scan(&small);
        let next = bytes(&[0xAA]);
        assert!(trig.scan(&next));
    }

    #[test]
    fn post_match_delay_holds_until_elapsed() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$02$AA".to_string());
        trig.post_match_delay_ms = 100;
        trig.arm_from(0);
        let t0 = Instant::now();
        // マッチ直後はまだ発火しない
        let buf = bytes(&[0x02, 0xAA]);
        assert!(!trig.scan_at(&buf, t0));
        assert!(trig.armed);
        // 50ms 後もまだ
        assert!(!trig.scan_at(&buf, t0 + std::time::Duration::from_millis(50)));
        assert!(trig.armed);
        // 100ms 経過で発火
        assert!(trig.scan_at(&buf, t0 + std::time::Duration::from_millis(100)));
        assert!(!trig.armed);
    }

    #[test]
    fn post_match_zero_fires_immediately() {
        let mut trig = ByteTrigger::new();
        trig.set_pattern_text("$AA".to_string());
        trig.post_match_delay_ms = 0;
        trig.arm_from(0);
        assert!(trig.scan(&bytes(&[0xAA])));
    }

    // --- PatternMatcher 単体 ---

    #[test]
    fn pattern_matcher_skips_non_byte_entries() {
        let mut m = PatternMatcher::new();
        m.set_pattern_text("$02$AA");
        let entries = vec![
            DataEntry::Byte(0x02, t()),
            DataEntry::Sent(0xFF, t()),
            DataEntry::Idle(10.0),
            DataEntry::Error,
            DataEntry::Byte(0xAA, t()),
        ];
        assert!(m.scan(&entries));
    }

    #[test]
    fn pattern_matcher_reset_skips_existing() {
        let mut m = PatternMatcher::new();
        m.set_pattern_text("$AA");
        let mut buf = bytes(&[0xAA, 0xAA, 0xAA]);
        m.reset(buf.len());
        assert!(!m.scan(&buf));
        buf.extend(bytes(&[0xAA]));
        assert!(m.scan(&buf));
    }
}
