use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::serial::config::{ParitySetting, StopBitsSetting};

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
    pub language: Language,
    /// アクティブタブ ("monitor" or "protocol")
    #[serde(default)]
    pub active_tab: String,
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
            language: Language::default(),
            active_tab: String::new(),
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
