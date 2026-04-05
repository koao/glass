use serde::{Deserialize, Serialize};

/// 言語設定
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Language {
    Ja,
    En,
}

impl Language {
    pub const ALL: &[Self] = &[Self::Ja, Self::En];

    /// 表示用ラベル（各言語の自称）
    pub fn label(self) -> &'static str {
        match self {
            Self::Ja => "日本語",
            Self::En => "English",
        }
    }

    /// 翻訳テーブルを取得
    pub fn texts(self) -> &'static Texts {
        match self {
            Self::Ja => &JA,
            Self::En => &EN,
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::Ja
    }
}

/// 全UIテキスト
pub struct Texts {
    // -- header_bar --
    pub resume: &'static str,
    pub start: &'static str,
    pub pause: &'static str,
    pub stop: &'static str,
    pub settings: &'static str,
    pub settings_stopped_only: &'static str,
    pub clear: &'static str,
    pub search_shortcut: &'static str,

    // -- status_bar --
    pub received: &'static str,
    pub errors: &'static str,
    pub unselected: &'static str,
    pub status_stopped: &'static str,
    pub status_receiving: &'static str,
    pub status_paused: &'static str,

    // -- search_bar --
    pub search_label: &'static str,
    pub search_hint: &'static str,
    pub search_button: &'static str,
    pub search_clear: &'static str,
    pub no_match: &'static str,
    pub help: &'static str,

    // -- 検索ヘルプウィンドウ --
    pub search_help_title: &'static str,
    pub search_help_desc: &'static str,
    pub search_help_input: &'static str,
    pub search_help_meaning: &'static str,
    pub search_help_hex_byte: &'static str,
    pub search_help_other_chars: &'static str,
    pub search_help_ascii_literal: &'static str,
    pub search_help_examples: &'static str,

    // -- settings_window --
    pub settings_title: &'static str,
    pub tab_serial: &'static str,
    pub tab_display: &'static str,
    pub port_label: &'static str,
    pub port_refresh: &'static str,
    pub port_select: &'static str,
    pub baud_rate: &'static str,
    pub data_bits: &'static str,
    pub parity: &'static str,
    pub stop_bits: &'static str,
    pub settings_stopped_msg: &'static str,
    pub idle_threshold: &'static str,
    pub idle_desc: &'static str,
    pub language: &'static str,

    // -- monitor_view --
    pub no_data: &'static str,

    // -- app エラー --
    pub err_no_port: &'static str,
    pub err_port_open: &'static str,
}

const JA: Texts = Texts {
    // header_bar
    resume: "再開",
    start: "開始",
    pause: "一時停止",
    stop: "停止",
    settings: "設定",
    settings_stopped_only: "設定 (停止中のみ)",
    clear: "クリア",
    search_shortcut: "検索 (Ctrl+F)",

    // status_bar
    received: "受信",
    errors: "エラー",
    unselected: "未選択",
    status_stopped: "停止",
    status_receiving: "受信中",
    status_paused: "一時停止",

    // search_bar
    search_label: "検索:",
    search_hint: "例: OK$0D$0A",
    search_button: "検索",
    search_clear: "クリア",
    no_match: "一致なし",
    help: "ヘルプ",

    // 検索ヘルプ
    search_help_title: "検索ヘルプ",
    search_help_desc: "テキストと16進数を混在して検索できます。",
    search_help_input: "入力",
    search_help_meaning: "意味",
    search_help_hex_byte: "16進数バイト",
    search_help_other_chars: "その他の文字",
    search_help_ascii_literal: "ASCII文字そのまま",
    search_help_examples: "入力例:",

    // settings_window
    settings_title: "設定",
    tab_serial: "シリアルポート",
    tab_display: "表示",
    port_label: "ポート:",
    port_refresh: "ポート一覧を更新",
    port_select: "選択してください",
    baud_rate: "ボーレート:",
    data_bits: "データビット:",
    parity: "パリティ:",
    stop_bits: "ストップビット:",
    settings_stopped_msg: "設定変更は停止中のみ可能",
    idle_threshold: "IDLE閾値:",
    idle_desc: "バイト間の無通信時間がこの値を超えるとIDLEマーカーを表示",
    language: "言語:",

    // monitor_view
    no_data: "データなし — COMポートを選択して開始してください",

    // app エラー
    err_no_port: "COMポートを選択してください",
    err_port_open: "ポートオープン失敗",
};

const EN: Texts = Texts {
    // header_bar
    resume: "Resume",
    start: "Start",
    pause: "Pause",
    stop: "Stop",
    settings: "Settings",
    settings_stopped_only: "Settings (stopped only)",
    clear: "Clear",
    search_shortcut: "Search (Ctrl+F)",

    // status_bar
    received: "Received",
    errors: "Errors",
    unselected: "Not selected",
    status_stopped: "Stopped",
    status_receiving: "Receiving",
    status_paused: "Paused",

    // search_bar
    search_label: "Search:",
    search_hint: "e.g. OK$0D$0A",
    search_button: "Search",
    search_clear: "Clear",
    no_match: "No match",
    help: "Help",

    // 検索ヘルプ
    search_help_title: "Search Help",
    search_help_desc: "Search using mixed text and hex bytes.",
    search_help_input: "Input",
    search_help_meaning: "Meaning",
    search_help_hex_byte: "Hex byte",
    search_help_other_chars: "Other characters",
    search_help_ascii_literal: "Literal ASCII",
    search_help_examples: "Examples:",

    // settings_window
    settings_title: "Settings",
    tab_serial: "Serial Port",
    tab_display: "Display",
    port_label: "Port:",
    port_refresh: "Refresh port list",
    port_select: "Select a port",
    baud_rate: "Baud rate:",
    data_bits: "Data bits:",
    parity: "Parity:",
    stop_bits: "Stop bits:",
    settings_stopped_msg: "Settings can only be changed while stopped",
    idle_threshold: "IDLE threshold:",
    idle_desc: "Shows IDLE marker when silence between bytes exceeds this value",
    language: "Language:",

    // monitor_view
    no_data: "No data — select a COM port and start",

    // app エラー
    err_no_port: "Please select a COM port",
    err_port_open: "Failed to open port",
};
