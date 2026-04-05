use std::fmt::Write;

use regex::Regex;

use crate::model::entry::DataEntry;
use super::definition::{ParsedFrameRule, ProtocolFile};

/// バイトストリームから抽出されたフレーム
#[derive(Clone, Debug)]
pub struct Frame {
    pub bytes: Vec<u8>,
}

/// マッチ結果
#[derive(Clone, Debug)]
pub struct MatchedMessage {
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
    compiled_patterns: Vec<(usize, Regex)>,
    pub frame_idle_threshold_ms: f64,
    frame_rules: Vec<ParsedFrameRule>,
}

impl ProtocolEngine {
    pub fn new(protocol: &ProtocolFile) -> Self {
        let mut compiled = Vec::new();
        for (i, msg) in protocol.messages.iter().enumerate() {
            match Regex::new(&msg.pattern) {
                Ok(re) => compiled.push((i, re)),
                Err(e) => {
                    eprintln!("パターンコンパイルエラー [{}]: {}", msg.id, e);
                }
            }
        }

        let frame_rules: Vec<ParsedFrameRule> = protocol.protocol.frame_rules
            .iter()
            .filter_map(|r| r.parse())
            .collect();

        Self {
            compiled_patterns: compiled,
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
        for (msg_idx, re) in &self.compiled_patterns {
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
                    if *ms >= engine.frame_idle_threshold_ms {
                        self.finalize_pending(engine);
                        self.flush_error();
                        self.pending_idle_ms = Some(*ms);
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
            FrameState::WaitingEnd { end_byte, remaining } => {
                let end_byte = *end_byte;
                let remaining = *remaining;
                self.pending_bytes.push(b);

                if b == end_byte {
                    let end_extra = engine.find_rule(self.pending_bytes[0])
                        .map(|r| r.end_extra).unwrap_or(0);
                    if end_extra > 0 {
                        self.frame_state = FrameState::WaitingExtra(end_extra);
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

    fn finalize_pending(&mut self, engine: &ProtocolEngine) {
        if self.pending_bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.pending_bytes);
        let msg_idx = engine.match_frame(&bytes);
        self.matches.push(MatchedMessage {
            message_def_idx: msg_idx,
            frame: Frame { bytes },
            preceding_idle_ms: self.pending_idle_ms.take(),
        });
        self.trim_matches();
    }

    fn flush_error(&mut self) {
        if self.error_bytes.is_empty() {
            return;
        }
        self.matches.push(MatchedMessage {
            message_def_idx: None,
            frame: Frame { bytes: std::mem::take(&mut self.error_bytes) },
            preceding_idle_ms: self.pending_idle_ms.take(),
        });
        self.trim_matches();
    }

    fn trim_matches(&mut self) {
        if self.matches.len() > MAX_MATCHES {
            let trim_count = (MAX_MATCHES as f64 * TRIM_RATIO) as usize;
            self.matches.drain(..trim_count);
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
    }
}
