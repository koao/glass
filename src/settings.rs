use std::path::PathBuf;

use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::serial::config::{ParitySetting, StopBitsSetting};
use crate::ui::theme;

/// モニタービュー文字色設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitorColors {
    /// データバイト色 (ASCII印字可能文字 0x21-0x7E)
    pub data_color: [u8; 3],
    /// 制御コード色 (0x00-0x20, 0x7F)
    pub control_color: [u8; 3],
    /// 高バイト色 (0x80-0xFF / Hexモード全バイト)
    pub high_byte_color: [u8; 3],
    /// IDLEカウンタ文字色
    pub idle_text: [u8; 3],
    /// IDLEカウンタ背景色
    pub idle_bg: [u8; 3],
}

const fn rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

impl Default for MonitorColors {
    fn default() -> Self {
        Self {
            data_color: rgb(theme::DATA_COLOR),
            control_color: rgb(theme::CONTROL_COLOR),
            high_byte_color: rgb(theme::HIGH_BYTE_COLOR),
            idle_text: rgb(theme::IDLE_TEXT),
            idle_bg: rgb(theme::IDLE_BG),
        }
    }
}

impl MonitorColors {
    pub fn data_color32(&self) -> Color32 {
        Color32::from_rgb(self.data_color[0], self.data_color[1], self.data_color[2])
    }
    pub fn control_color32(&self) -> Color32 {
        Color32::from_rgb(
            self.control_color[0],
            self.control_color[1],
            self.control_color[2],
        )
    }
    pub fn high_byte_color32(&self) -> Color32 {
        Color32::from_rgb(
            self.high_byte_color[0],
            self.high_byte_color[1],
            self.high_byte_color[2],
        )
    }
    pub fn idle_text_color32(&self) -> Color32 {
        Color32::from_rgb(self.idle_text[0], self.idle_text[1], self.idle_text[2])
    }
    pub fn idle_bg_color32(&self) -> Color32 {
        Color32::from_rgb(self.idle_bg[0], self.idle_bg[1], self.idle_bg[2])
    }
}

/// 永続化する設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: ParitySetting,
    pub stop_bits: StopBitsSetting,
    pub idle_threshold_ms: f64,
    pub display_mode: String,
    #[serde(default)]
    pub show_settings_window: bool,
    #[serde(default)]
    pub show_search_bar: bool,
    #[serde(default)]
    pub show_protocol_search_bar: bool,
    #[serde(default)]
    pub language: Language,
    /// アクティブタブ ("monitor" or "protocol")
    #[serde(default)]
    pub active_tab: String,
    /// プロトコルパネル表示モード ("list" or "wrap")
    #[serde(default)]
    pub protocol_view_mode: String,
    /// 選択中のプロトコル定義ファイル名
    #[serde(default)]
    pub selected_protocol: String,
    /// モニタービュー文字色
    #[serde(default)]
    pub monitor_colors: MonitorColors,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 9600,
            data_bits: 8,
            parity: ParitySetting::None,
            stop_bits: StopBitsSetting::One,
            idle_threshold_ms: 10.0,
            display_mode: "Hex".to_string(),
            show_settings_window: false,
            show_search_bar: false,
            show_protocol_search_bar: false,
            language: Language::default(),
            active_tab: String::new(),
            protocol_view_mode: String::new(),
            selected_protocol: String::new(),
            monitor_colors: MonitorColors::default(),
        }
    }
}

/// 設定ファイルのパスを取得（exe隣のglass_settings.json）
fn settings_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("glass_settings.json")
}

impl AppSettings {
    /// 設定を読み込み（失敗時はデフォルト値）
    pub fn load() -> Self {
        let path = settings_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 設定を保存
    pub fn save(&self) {
        let path = settings_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}
