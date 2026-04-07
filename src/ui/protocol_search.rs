use std::collections::HashSet;
use std::fmt::Write;

use crate::protocol::definition::ProtocolFile;
use crate::protocol::engine::MatchedMessage;
use crate::ui::protocol_panel::{extract_ascii, extract_hex};

/// パース済み検索式
enum SearchExpr {
    /// 単一キーワード（小文字化済みテキスト、オプションのHEXバイトパターン）
    Term(String, Option<Vec<u8>>),
    /// AND結合
    And(Vec<SearchExpr>),
    /// OR結合
    Or(Vec<SearchExpr>),
}

impl SearchExpr {
    fn matches(&self, searchable_lower: &str, frame_bytes: &[u8]) -> bool {
        match self {
            SearchExpr::Term(text, hex_pat) => {
                let text_hit = searchable_lower.contains(text.as_str());
                let byte_hit = hex_pat
                    .as_ref()
                    .is_some_and(|pat| contains_bytes(frame_bytes, pat));
                text_hit || byte_hit
            }
            SearchExpr::And(exprs) => exprs
                .iter()
                .all(|e| e.matches(searchable_lower, frame_bytes)),
            SearchExpr::Or(exprs) => exprs
                .iter()
                .any(|e| e.matches(searchable_lower, frame_bytes)),
        }
    }
}

/// クエリ文字列をパース
/// - `A AND B` → And([Term(a), Term(b)])  （大文字のANDのみ）
/// - `A OR B`  → Or([Term(a), Term(b)])   （大文字のORのみ）
/// - `A B`     → And([Term(a), Term(b)])  （スペース区切り = 暗黙AND）
/// - `"A B"`   → Term("a b")             （クォートでスペースをエスケープ）
/// - OR で分割 → 各ブロック内を AND/スペースで分割
///
/// ※ クォート内の AND / OR はキーワードとして扱わない
fn parse_query(input: &str) -> Option<SearchExpr> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // まずクォートを考慮してトークン化
    let tokens = tokenize(input);
    if tokens.is_empty() {
        return None;
    }

    // トークン列を OR で分割 → 各ブロックを AND で結合
    let mut or_groups: Vec<Vec<&Token>> = vec![Vec::new()];
    for tok in &tokens {
        if matches!(tok, Token::Or) {
            or_groups.push(Vec::new());
        } else {
            or_groups.last_mut().unwrap().push(tok);
        }
    }

    let mut or_exprs: Vec<SearchExpr> = Vec::new();
    for group in or_groups {
        // AND キーワードを除去して残りをterm化
        let terms: Vec<SearchExpr> = group
            .into_iter()
            .filter(|t| !matches!(t, Token::And))
            .filter_map(|t| {
                if let Token::Text(s) = t {
                    if s.is_empty() {
                        return None;
                    }
                    let lower = s.to_lowercase();
                    let hex_pat = parse_hex_pattern(s);
                    Some(SearchExpr::Term(lower, hex_pat))
                } else {
                    None
                }
            })
            .collect();

        match terms.len() {
            0 => {}
            1 => or_exprs.push(terms.into_iter().next().unwrap()),
            _ => or_exprs.push(SearchExpr::And(terms)),
        }
    }

    match or_exprs.len() {
        0 => None,
        1 => Some(or_exprs.into_iter().next().unwrap()),
        _ => Some(SearchExpr::Or(or_exprs)),
    }
}

/// トークン種別
enum Token {
    Text(String),
    And,
    Or,
}

/// クォート対応トークナイザ
/// スペースで分割し、ダブルクォート内はスペースを保持。
/// "AND" / "OR"（クォート外かつ大文字完全一致）はキーワードトークンに。
fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for c in input.chars() {
        match c {
            '"' => {
                if in_quote {
                    // クォート閉じ: 中身をそのままTextトークンに（AND/OR判定しない）
                    tokens.push(Token::Text(std::mem::take(&mut current)));
                } else if !current.is_empty() {
                    // クォート開始前にテキストがあればflush
                    push_text_or_keyword(&mut tokens, std::mem::take(&mut current));
                }
                in_quote = !in_quote;
            }
            ' ' if !in_quote => {
                if !current.is_empty() {
                    push_text_or_keyword(&mut tokens, std::mem::take(&mut current));
                }
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        if in_quote {
            // 閉じクォートなし: そのままTextとして扱う
            tokens.push(Token::Text(current));
        } else {
            push_text_or_keyword(&mut tokens, current);
        }
    }
    tokens
}

fn push_text_or_keyword(tokens: &mut Vec<Token>, text: String) {
    match text.as_str() {
        "AND" => tokens.push(Token::And),
        "OR" => tokens.push(Token::Or),
        _ => tokens.push(Token::Text(text)),
    }
}

pub struct ProtocolSearchState {
    pub query: String,
    pub has_searched: bool,
    /// 検索結果（match ID、昇順ソート済み）
    results: Vec<u64>,
    current: usize,
    /// 次回 auto_refresh で走査を開始する match ID
    next_scan_id: u64,
    scroll_to_match: Option<u64>,
}

impl ProtocolSearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            has_searched: false,
            results: Vec::new(),
            current: 0,
            next_scan_id: 0,
            scroll_to_match: None,
        }
    }

    /// 検索実行
    pub fn search(
        &mut self,
        matches: &[MatchedMessage],
        proto: Option<&ProtocolFile>,
        hidden_ids: &HashSet<String>,
    ) {
        self.has_searched = true;
        self.current = 0;
        self.results.clear();
        self.scroll_to_match = None;
        self.next_scan_id = 0;
        self.run_search(matches, proto, hidden_ids);
        if !self.results.is_empty() {
            self.scroll_to_match = Some(self.results[self.current]);
        }
    }

    /// バッファ増加時の差分検索
    pub fn auto_refresh(
        &mut self,
        matches: &[MatchedMessage],
        proto: Option<&ProtocolFile>,
        hidden_ids: &HashSet<String>,
    ) {
        if self.has_searched && !self.query.is_empty() {
            let last_id = matches.last().map(|m| m.id);
            if last_id.is_some_and(|id| id >= self.next_scan_id) {
                self.run_search(matches, proto, hidden_ids);
            }
        }
    }

    /// next_scan_id 以降の matches を検索して results に追加
    fn run_search(
        &mut self,
        matches: &[MatchedMessage],
        proto: Option<&ProtocolFile>,
        hidden_ids: &HashSet<String>,
    ) {
        let expr = match parse_query(&self.query) {
            Some(e) => e,
            None => return,
        };

        let proto = match proto {
            Some(p) => p,
            None => return,
        };

        let mut buf = String::new();

        for matched in matches.iter().filter(|m| m.id >= self.next_scan_id) {
            if let Some(def_idx) = matched.message_def_idx
                && hidden_ids.contains(&proto.messages[def_idx].id)
            {
                continue;
            }

            buf.clear();
            build_searchable_text(matched, proto, &mut buf);
            buf.make_ascii_lowercase();

            if expr.matches(&buf, &matched.frame.bytes) {
                self.results.push(matched.id);
            }
        }
        if let Some(last) = matches.last() {
            self.next_scan_id = last.id + 1;
        }
    }

    pub fn next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.current = (self.current + 1) % self.results.len();
        self.scroll_to_match = Some(self.results[self.current]);
    }

    pub fn prev(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.current = if self.current == 0 {
            self.results.len() - 1
        } else {
            self.current - 1
        };
        self.scroll_to_match = Some(self.results[self.current]);
    }

    pub fn reset(&mut self) {
        self.query.clear();
        self.clear();
    }

    pub fn clear(&mut self) {
        self.results.clear();
        self.current = 0;
        self.has_searched = false;
        self.next_scan_id = 0;
        self.scroll_to_match = None;
    }

    pub fn result_count(&self) -> usize {
        self.results.len()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn is_hit(&self, id: u64) -> bool {
        self.results.binary_search(&id).is_ok()
    }

    pub fn is_current_hit(&self, id: u64) -> bool {
        if self.results.is_empty() {
            return false;
        }
        self.results[self.current] == id
    }

    pub fn take_scroll_target(&mut self) -> Option<u64> {
        self.scroll_to_match.take()
    }
}

/// メッセージから検索対象テキストをバッファに書き込む
fn build_searchable_text(matched: &MatchedMessage, proto: &ProtocolFile, buf: &mut String) {
    match matched.message_def_idx {
        Some(def_idx) => {
            let msg_def = &proto.messages[def_idx];
            buf.push_str(&msg_def.title);
            buf.push(' ');
            for field in &msg_def.fields {
                let ascii = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                let _ = write!(buf, "{}:{} ", field.name, ascii);
            }
        }
        None => {
            buf.push_str("Unknown");
        }
    }
    let hex = extract_hex(&matched.frame.bytes, 0, matched.frame.bytes.len());
    buf.push_str(&hex);
}

/// $XX形式のHEXバイトパターンを抽出（$が含まれない場合はNone）
fn parse_hex_pattern(input: &str) -> Option<Vec<u8>> {
    if !input.contains('$') {
        return None;
    }
    let chars: Vec<char> = input.chars().collect();
    let mut bytes = Vec::new();
    let mut i = 0;
    let mut has_hex = false;
    while i < chars.len() {
        if chars[i] == '$'
            && i + 2 < chars.len()
            && chars[i + 1].is_ascii_hexdigit()
            && chars[i + 2].is_ascii_hexdigit()
        {
            let hex_str: String = [chars[i + 1], chars[i + 2]].iter().collect();
            if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                bytes.push(byte);
                has_hex = true;
            }
            i += 3;
        } else {
            bytes.push(chars[i] as u8);
            i += 1;
        }
    }
    if has_hex { Some(bytes) } else { None }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}
