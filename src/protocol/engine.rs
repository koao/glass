use std::fmt::Write;

use regex::Regex;

use super::definition::{ParsedFrameRule, ProtocolFile};
use crate::model::entry::DataEntry;

/// バイトストリームから抽出されたフレーム
#[derive(Clone, Debug)]
pub struct Frame {
    pub bytes: Vec<u8>,
}

/// マッチ結果
#[derive(Clone, Debug)]
pub struct MatchedMessage {
    /// 単調増加ID（trim後もインデックスではなく ID で参照する）
    pub id: u64,
    /// MessageDef配列内のインデックス（Noneは未マッチ＝エラー）
    pub message_def_idx: Option<usize>,
    pub frame: Frame,
    /// 直前のフレームとの間のIDLE時間(ms)
    pub preceding_idle_ms: Option<f64>,
}

/// フレーム構築中の状態
#[derive(Clone, Debug)]
enum FrameState {
    /// トリガーバイト待ち
    Idle,
    /// 固定長取得中（残りバイト数）
    FixedLength(usize),
    /// 終了バイト待ち（終了バイト、上限残り）
    WaitingEnd { end_byte: u8, remaining: usize },
    /// 終了バイト後の追加バイト待ち（残りバイト数）
    WaitingExtra(usize),
}

/// プロトコルマッチングエンジン
pub struct ProtocolEngine {
    /// first_byte 指定パターン: buckets[byte] = list of (msg_idx, regex)
    buckets: Vec<Vec<(usize, Regex)>>,
    /// first_byte 未指定のフォールバックパターン
    fallback: Vec<(usize, Regex)>,
    pub frame_idle_threshold_ms: f64,
    frame_rules: Vec<ParsedFrameRule>,
}

impl ProtocolEngine {
    pub fn new(protocol: &ProtocolFile) -> Self {
        let mut buckets: Vec<Vec<(usize, Regex)>> = (0..256).map(|_| Vec::new()).collect();
        let mut fallback: Vec<(usize, Regex)> = Vec::new();
        for (i, msg) in protocol.messages.iter().enumerate() {
            match Regex::new(&msg.pattern) {
                Ok(re) => {
                    if let Some(b) = msg.parsed_first_byte {
                        buckets[b as usize].push((i, re));
                    } else {
                        fallback.push((i, re));
                    }
                }
                Err(e) => {
                    eprintln!("パターンコンパイルエラー [{}]: {}", msg.id, e);
                }
            }
        }

        let frame_rules: Vec<ParsedFrameRule> = protocol
            .protocol
            .frame_rules
            .iter()
            .filter_map(|r| r.parse())
            .collect();

        Self {
            buckets,
            fallback,
            frame_idle_threshold_ms: protocol.protocol.frame_idle_threshold_ms,
            frame_rules,
        }
    }

    /// フレームのバイト列をHEX文字列に変換してパターンマッチ
    fn match_frame(&self, bytes: &[u8]) -> Option<usize> {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            write!(hex, "{:02X}", b).unwrap();
        }
        if let Some(first) = bytes.first() {
            for (msg_idx, re) in &self.buckets[*first as usize] {
                if re.is_match(&hex) {
                    return Some(*msg_idx);
                }
            }
        }
        for (msg_idx, re) in &self.fallback {
            if re.is_match(&hex) {
                return Some(*msg_idx);
            }
        }
        None
    }

    fn find_rule(&self, byte: u8) -> Option<&ParsedFrameRule> {
        self.frame_rules.iter().find(|r| r.trigger == byte)
    }

    fn has_rules(&self) -> bool {
        !self.frame_rules.is_empty()
    }
}

/// マッチ結果の上限
const MAX_MATCHES: usize = 50_000;
const TRIM_RATIO: f64 = 0.1;

/// プロトコル状態管理（GlassAppが保持）
pub struct ProtocolState {
    pub matches: Vec<MatchedMessage>,
    processed_count: usize,
    pending_bytes: Vec<u8>,
    pending_idle_ms: Option<f64>,
    frame_state: FrameState,
    error_bytes: Vec<u8>,
    /// error_bytes に紐付ける IDLE 時間（蓄積開始時に固定）
    error_idle_ms: Option<f64>,
    /// 次に採番する match ID
    next_match_id: u64,
    /// matches[0] の ID（trim 後の O(1) インデックス解決用）
    first_id: u64,
}

impl ProtocolState {
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
            processed_count: 0,
            pending_bytes: Vec::new(),
            pending_idle_ms: None,
            frame_state: FrameState::Idle,
            error_bytes: Vec::new(),
            error_idle_ms: None,
            next_match_id: 0,
            first_id: 0,
        }
    }

    /// ID から matches 内の現在インデックスを O(1) で解決
    pub fn position_by_id(&self, id: u64) -> Option<usize> {
        let pos = id.checked_sub(self.first_id)? as usize;
        if pos < self.matches.len() {
            Some(pos)
        } else {
            None
        }
    }

    pub fn sync_entries(&mut self, entries: &[DataEntry], engine: &ProtocolEngine) {
        if self.processed_count > entries.len() {
            self.clear();
        }

        for i in self.processed_count..entries.len() {
            match &entries[i] {
                DataEntry::Byte(b, _) => {
                    if engine.has_rules() {
                        self.process_byte_ruled(*b, engine);
                    } else {
                        self.pending_bytes.push(*b);
                    }
                }
                DataEntry::Idle(ms) => {
                    // しきい値未満の IDLE も常に累積し、次フレームの preceding_idle_ms に反映
                    self.pending_idle_ms = Some(self.pending_idle_ms.unwrap_or(0.0) + *ms);
                    if *ms >= engine.frame_idle_threshold_ms {
                        self.finalize_pending(engine);
                        self.flush_error();
                        self.frame_state = FrameState::Idle;
                    }
                }
                DataEntry::Error => {}
            }
        }
        self.processed_count = entries.len();
    }

    fn process_byte_ruled(&mut self, b: u8, engine: &ProtocolEngine) {
        match &self.frame_state {
            FrameState::Idle => {
                if let Some(rule) = engine.find_rule(b) {
                    self.flush_error();
                    self.pending_bytes.clear();
                    self.pending_bytes.push(b);

                    if let Some(len) = rule.length {
                        if len <= 1 {
                            self.finalize_pending(engine);
                        } else {
                            self.frame_state = FrameState::FixedLength(len - 1);
                        }
                    } else if let Some(end_byte) = rule.end_byte {
                        self.frame_state = FrameState::WaitingEnd {
                            end_byte,
                            remaining: rule.max_length - 1,
                        };
                    } else {
                        self.finalize_pending(engine);
                    }
                } else {
                    // エラーバイト蓄積開始時に IDLE 時間を確定（次フレームに漏らさない）
                    if self.error_bytes.is_empty() {
                        self.error_idle_ms = self.pending_idle_ms.take();
                    }
                    self.error_bytes.push(b);
                }
            }
            FrameState::FixedLength(remaining) => {
                self.pending_bytes.push(b);
                let remaining = *remaining;
                if remaining <= 1 {
                    self.finalize_pending(engine);
                    self.frame_state = FrameState::Idle;
                } else {
                    self.frame_state = FrameState::FixedLength(remaining - 1);
                }
            }
            FrameState::WaitingEnd {
                end_byte,
                remaining,
            } => {
                let end_byte = *end_byte;
                let remaining = *remaining;
                self.pending_bytes.push(b);

                if b == end_byte {
                    let end_extra = engine
                        .find_rule(self.pending_bytes[0])
                        .map(|r| r.end_extra)
                        .unwrap_or(0);
                    // max_length の残量で end_extra を切り詰める
                    let extra = end_extra.min(remaining.saturating_sub(1));
                    if extra > 0 {
                        self.frame_state = FrameState::WaitingExtra(extra);
                    } else {
                        self.finalize_pending(engine);
                        self.frame_state = FrameState::Idle;
                    }
                } else if remaining <= 1 {
                    self.finalize_pending(engine);
                    self.frame_state = FrameState::Idle;
                } else {
                    self.frame_state = FrameState::WaitingEnd {
                        end_byte,
                        remaining: remaining - 1,
                    };
                }
            }
            FrameState::WaitingExtra(remaining) => {
                let remaining = *remaining;
                self.pending_bytes.push(b);
                if remaining <= 1 {
                    self.finalize_pending(engine);
                    self.frame_state = FrameState::Idle;
                } else {
                    self.frame_state = FrameState::WaitingExtra(remaining - 1);
                }
            }
        }
    }

    fn push_match(&mut self, m: MatchedMessage) {
        if self.matches.is_empty() {
            self.first_id = m.id;
        }
        self.matches.push(m);
        self.trim_matches();
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_match_id;
        self.next_match_id += 1;
        id
    }

    fn finalize_pending(&mut self, engine: &ProtocolEngine) {
        if self.pending_bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.pending_bytes);
        let msg_idx = engine.match_frame(&bytes);
        let id = self.next_id();
        let preceding_idle_ms = self.pending_idle_ms.take();
        self.push_match(MatchedMessage {
            id,
            message_def_idx: msg_idx,
            frame: Frame { bytes },
            preceding_idle_ms,
        });
    }

    fn flush_error(&mut self) {
        if self.error_bytes.is_empty() {
            return;
        }
        let id = self.next_id();
        let bytes = std::mem::take(&mut self.error_bytes);
        let preceding_idle_ms = self.error_idle_ms.take();
        self.push_match(MatchedMessage {
            id,
            message_def_idx: None,
            frame: Frame { bytes },
            preceding_idle_ms,
        });
    }

    fn trim_matches(&mut self) {
        if self.matches.len() > MAX_MATCHES {
            let trim_count = (MAX_MATCHES as f64 * TRIM_RATIO) as usize;
            self.matches.drain(..trim_count);
            self.first_id = self
                .matches
                .first()
                .map(|m| m.id)
                .unwrap_or(self.next_match_id);
        }
    }

    pub fn flush(&mut self, engine: &ProtocolEngine) {
        self.finalize_pending(engine);
        self.flush_error();
        self.frame_state = FrameState::Idle;
    }

    pub fn clear(&mut self) {
        self.matches.clear();
        self.processed_count = 0;
        self.pending_bytes.clear();
        self.pending_idle_ms = None;
        self.frame_state = FrameState::Idle;
        self.error_bytes.clear();
        self.error_idle_ms = None;
        self.next_match_id = 0;
        self.first_id = 0;
    }
}
