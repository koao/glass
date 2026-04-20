// === 配色テーマ ===
// 長時間モニタリングでの目の疲労を軽減するため、彩度を抑えた配色。
// すべてのテキスト色は暗背景に対しWCAG AA以上のコントラスト比を確保。

use egui::Color32;

// --- グリッド ---
pub const GRID_BG: Color32 = Color32::from_rgb(26, 27, 38);
pub const GRID_LINE: Color32 = Color32::from_rgb(46, 48, 68);

// --- データ表示 ---
/// データバイト: セージグリーン (対背景 ~7.0:1)
pub const DATA_COLOR: Color32 = Color32::from_rgb(156, 190, 156);
/// 制御コード: 琥珀色 (対背景 ~7.8:1)
pub const CONTROL_COLOR: Color32 = Color32::from_rgb(212, 165, 106);
/// 高バイト (0x80-0xFF): スチールブルー (対背景 ~7.2:1)
pub const HIGH_BYTE_COLOR: Color32 = Color32::from_rgb(160, 180, 212);
/// 送信バイト: ピンク寄りマゼンタ (受信系のどの色とも衝突しない)
pub const SENT_COLOR: Color32 = Color32::from_rgb(220, 140, 180);

// --- IDLEマーカー ---
pub const IDLE_BG: Color32 = Color32::from_rgb(42, 48, 56);
pub const IDLE_TEXT: Color32 = Color32::from_rgb(111, 181, 181);

// --- カーソル ---
pub const CURSOR_FILL: Color32 = Color32::from_rgba_premultiplied(184, 196, 208, 40);
pub const CURSOR_STROKE: Color32 = Color32::from_rgb(184, 196, 208);

// --- ステータスバー ---
pub const STATUS_STOPPED: Color32 = Color32::from_rgb(136, 144, 160);
pub const STATUS_RUNNING: Color32 = Color32::from_rgb(120, 184, 146);
pub const STATUS_PAUSED: Color32 = Color32::from_rgb(212, 165, 106);
pub const STATUS_ERROR: Color32 = Color32::from_rgb(212, 112, 112);

// --- 汎用 ---
pub const TEXT_MUTED: Color32 = Color32::from_rgb(136, 144, 160);

// --- 検索ハイライト ---
/// 全一致箇所の背景（暗い黄色）
pub const SEARCH_HIGHLIGHT_BG: Color32 = Color32::from_rgb(80, 80, 40);
/// 現在選択中の一致箇所の背景（暗い緑）
pub const SEARCH_CURRENT_BG: Color32 = Color32::from_rgb(60, 100, 60);

// --- ヘッダーバー ---
/// ステータスピル背景（停止中）
pub const PILL_BG_STOPPED: Color32 = Color32::from_rgb(60, 63, 80);
/// ステータスピル背景（受信中）
pub const PILL_BG_RUNNING: Color32 = Color32::from_rgb(30, 70, 50);
/// ステータスピル背景（一時停止）
pub const PILL_BG_PAUSED: Color32 = Color32::from_rgb(70, 55, 30);
/// ステータスピル背景（切断）
pub const PILL_BG_DISCONNECTED: Color32 = Color32::from_rgb(80, 40, 40);

// --- プロトコルパネル ---
/// IDLE表示（ミュート）
pub const PROTOCOL_IDLE: Color32 = Color32::from_rgb(136, 144, 160);
/// 未マッチフレーム（グレー）
pub const PROTOCOL_UNMATCHED: Color32 = Color32::from_rgb(100, 104, 116);
/// プロトコルパネル行背景（偶数行）
pub const PROTOCOL_ROW_EVEN: Color32 = Color32::from_rgb(30, 32, 44);
/// プロトコルパネル行背景（奇数行）
pub const PROTOCOL_ROW_ODD: Color32 = Color32::from_rgb(26, 27, 38);

// --- ラップ表示 ---
/// ラップ表示のカーソル行背景
pub const WRAP_CURSOR_LINE: Color32 = Color32::from_rgb(30, 50, 40);
/// ラップ表示のメッセージピル背景
pub const WRAP_PILL_BG: Color32 = Color32::from_rgb(40, 42, 58);
/// ラップ表示のメッセージピル枠線
pub const WRAP_PILL_BORDER: Color32 = Color32::from_rgb(56, 60, 80);
/// ラップ表示の書き込み位置キャレット
pub const WRAP_CURSOR_CARET: Color32 = Color32::from_rgb(120, 200, 140);

// --- プロトコル検索ハイライト ---
/// 検索ヒット行背景（List用）
pub const PROTO_SEARCH_HIGHLIGHT_BG: Color32 = Color32::from_rgb(60, 60, 40);
/// 現在ヒット行背景（List用）
pub const PROTO_SEARCH_CURRENT_BG: Color32 = Color32::from_rgb(50, 80, 50);
/// 検索ヒットpill枠線（Wrap用）
pub const PROTO_SEARCH_HIGHLIGHT_BORDER: Color32 = Color32::from_rgb(180, 170, 80);
/// 現在ヒットpill枠線（Wrap用）
pub const PROTO_SEARCH_CURRENT_BORDER: Color32 = Color32::from_rgb(100, 200, 120);

// --- 選択範囲 ---
/// 選択範囲背景（半透明ブルー）
pub const SELECTION_BG: Color32 = Color32::from_rgba_premultiplied(80, 120, 200, 60);
