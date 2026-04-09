use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::checksum::ChecksumSpec;

/// プロトコル定義ファイル全体
#[derive(Clone, Debug, Deserialize)]
pub struct ProtocolFile {
    pub protocol: ProtocolMeta,
    #[serde(default)]
    pub messages: Vec<MessageDef>,
}

/// シーケンス図設定
#[derive(Clone, Debug, Deserialize, Default)]
pub struct SequenceConfig {
    /// デフォルトの送信元式（省略可）
    #[serde(default)]
    pub source: Option<String>,
    /// デフォルトの宛先式（省略可）
    #[serde(default)]
    pub destination: Option<String>,
    /// ブロードキャスト値（この値が宛先の場合、全参加者への送信）
    #[serde(default)]
    pub broadcast: Option<String>,
    /// マスタ参加者名 (常にシーケンス図の一番左に固定される)
    #[serde(default)]
    pub master: Option<String>,
}

/// プロトコルメタデータ
#[derive(Clone, Debug, Deserialize)]
pub struct ProtocolMeta {
    pub title: String,
    /// フレーム内の小IDLE無視閾値(ms)
    #[serde(default = "default_frame_idle_threshold")]
    pub frame_idle_threshold_ms: f64,
    /// フレーム取得ルール（先頭バイトに基づく取得方法の定義）
    #[serde(default)]
    pub frame_rules: Vec<FrameRule>,
    /// シーケンス図設定（省略可）
    #[serde(default)]
    pub sequence: Option<SequenceConfig>,
}

fn default_frame_idle_threshold() -> f64 {
    5.0
}

/// フレーム取得ルール
/// 先頭バイト（trigger）に基づいてどのようにフレームを取得するかを定義する
#[derive(Clone, Debug, Deserialize)]
pub struct FrameRule {
    /// 取得開始トリガーバイト (HEX、例: "02" = STX)
    pub trigger: String,
    /// 固定長の場合のバイト数（triggerバイト含む）
    #[serde(default)]
    pub length: Option<usize>,
    /// 終了バイト (HEX、例: "03" = ETX) — 可変長フレーム用
    #[serde(default)]
    pub end: Option<String>,
    /// 終了バイト後の追加バイト数（BCC等）
    #[serde(default)]
    pub end_extra: usize,
    /// 取得バイト数上限（無限ループ防止）
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    /// チェックサム / CRC 検証仕様（省略可）
    #[serde(default)]
    pub checksum: Option<ChecksumSpec>,
}

fn default_max_length() -> usize {
    512
}

/// パース済みフレームルール
#[derive(Clone, Debug)]
pub struct ParsedFrameRule {
    pub trigger: u8,
    pub length: Option<usize>,
    pub end_byte: Option<u8>,
    pub end_extra: usize,
    pub max_length: usize,
    pub checksum: Option<ChecksumSpec>,
}

impl FrameRule {
    /// パースして実行用構造体に変換
    pub fn parse(&self) -> Option<ParsedFrameRule> {
        let trigger = u8::from_str_radix(&self.trigger, 16).ok()?;
        let end_byte = self
            .end
            .as_ref()
            .and_then(|s| u8::from_str_radix(s, 16).ok());
        Some(ParsedFrameRule {
            trigger,
            length: self.length,
            end_byte,
            end_extra: self.end_extra,
            max_length: self.max_length,
            checksum: self.checksum.clone(),
        })
    }
}

/// メッセージ定義
#[derive(Clone, Debug, Deserialize)]
pub struct MessageDef {
    pub id: String,
    pub title: String,
    /// HEX文字列表現に対する正規表現
    #[allow(dead_code)]
    pub pattern: String,
    /// タイトル表示色 (HEX RGB、例: "FF8800")
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    /// パース済みタイトル色（ロード時に計算）
    #[serde(skip)]
    pub parsed_color: Option<egui::Color32>,
    /// シーケンス図の送信元式（グローバルデフォルトをオーバーライド）
    #[serde(default)]
    pub sequence_source: Option<String>,
    /// シーケンス図の宛先式（グローバルデフォルトをオーバーライド）
    #[serde(default)]
    pub sequence_destination: Option<String>,
    /// 先頭バイト hint (HEX 2桁)。指定があるとマッチ高速化に使われる
    #[serde(default)]
    pub first_byte: Option<String>,
    /// パース済み先頭バイト
    #[serde(skip)]
    pub parsed_first_byte: Option<u8>,
}

/// HEX RGB文字列をColor32にパース
fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

/// フィールド定義
#[derive(Clone, Debug, Deserialize)]
pub struct FieldDef {
    pub name: String,
    /// バイトオフセット（0始まり）
    pub offset: usize,
    /// バイト数
    pub size: usize,
    #[serde(default)]
    pub description: Option<String>,
    /// trueの場合、タイトル行にインライン表示する
    #[serde(default)]
    pub inline: bool,
}

/// 定義ファイルを1つ読み込み
pub fn load_protocol(path: &Path) -> Result<ProtocolFile, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut proto: ProtocolFile =
        toml::from_str(&content).map_err(|e| format!("{}: {}", path.display(), e))?;
    // 色 / first_byte をパースしてキャッシュ
    for msg in &mut proto.messages {
        msg.parsed_color = msg.color.as_deref().and_then(parse_hex_color);
        msg.parsed_first_byte = msg
            .first_byte
            .as_deref()
            .and_then(|s| u8::from_str_radix(s, 16).ok());
    }
    Ok(proto)
}

/// protocols/ディレクトリをスキャンして利用可能な定義ファイル一覧を返す
pub fn scan_protocols(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return result,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml")
            && let Ok(proto) = load_protocol(&path)
        {
            result.push((path, proto.protocol.title));
        }
    }
    result.sort_by(|a, b| a.1.cmp(&b.1));
    result
}

/// protocols/ディレクトリのパスを取得（exe隣）
pub fn protocols_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("protocols")
}
