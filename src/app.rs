use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;

use crate::model::buffer::MonitorBuffer;
use crate::model::entry::DataEntry;
use crate::model::grid::DisplayBuffer;
use crate::serial::config::SerialConfig;
use crate::serial::worker;
use crate::settings::AppSettings;
use crate::ui;
use crate::ui::search::SearchState;

/// 表示モード
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayMode {
    Hex,
    Ascii,
}

/// モニタ状態
#[derive(Clone, Debug, PartialEq)]
pub enum MonitorState {
    Stopped,
    Running,
    Paused,
}

/// 設定ウィンドウのタブ
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsTab {
    Serial,
    Display,
}

/// UI表示状態
pub struct UiState {
    /// 設定ウィンドウ表示フラグ
    pub show_settings_window: bool,
    /// 検索バー表示フラグ
    pub show_search_bar: bool,
    /// 設定ウィンドウの選択タブ
    pub settings_tab: SettingsTab,
}

/// アプリケーション本体
pub struct GlassApp {
    pub config: SerialConfig,
    pub available_ports: Vec<String>,
    pub state: MonitorState,
    pub display_mode: DisplayMode,
    pub idle_threshold_ms: f64,
    pub buffer: MonitorBuffer,
    pub display_buffer: DisplayBuffer,
    /// 最後のバイト受信時刻（ライブIDLEカウンタ用）
    pub last_byte_time: Option<Instant>,
    receiver: Option<Receiver<DataEntry>>,
    stop_sender: Option<Sender<()>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    pub last_error: Option<String>,
    pub search: SearchState,
    pub ui_state: UiState,
}

impl GlassApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ダークテーマ適用
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        // 日本語フォント設定
        Self::setup_fonts(&cc.egui_ctx);

        // UIスケール拡大（操作性向上）
        Self::setup_style(&cc.egui_ctx);

        // 保存済み設定を読み込み
        let settings = AppSettings::load();
        let display_mode = if settings.display_mode == "Ascii" {
            DisplayMode::Ascii
        } else {
            DisplayMode::Hex
        };

        let mut app = Self {
            config: SerialConfig {
                port_name: settings.port_name,
                baud_rate: settings.baud_rate,
                data_bits: settings.data_bits,
                parity: settings.parity,
                stop_bits: settings.stop_bits,
            },
            available_ports: Vec::new(),
            state: MonitorState::Stopped,
            display_mode,
            idle_threshold_ms: settings.idle_threshold_ms,
            buffer: MonitorBuffer::new(),
            display_buffer: DisplayBuffer::new(),
            last_byte_time: None,
            receiver: None,
            stop_sender: None,
            worker_handle: None,
            last_error: None,
            search: SearchState::new(),
            ui_state: UiState {
                show_settings_window: settings.show_settings_window,
                show_search_bar: settings.show_search_bar,
                settings_tab: SettingsTab::Serial,
            },
        };
        app.refresh_ports();
        app
    }

    /// 日本語対応フォントを設定
    fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        let font_paths = [
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "C:\\Windows\\Fonts\\YuGothR.ttc",
        ];

        for path in &font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "japanese".to_string(),
                    egui::FontData::from_owned(font_data).into(),
                );
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .push("japanese".to_string());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .push("japanese".to_string());
                break;
            }
        }

        ctx.set_fonts(fonts);
    }

    /// UIスタイル設定（大きめの操作要素）
    fn setup_style(ctx: &egui::Context) {
        let mut style = (*ctx.global_style()).clone();
        // フォントサイズを拡大
        for (_, font_id) in style.text_styles.iter_mut() {
            font_id.size = (font_id.size * 1.3).max(15.0);
        }
        // ボタン余白を拡大
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        ctx.set_global_style(style);
    }

    pub fn refresh_ports(&mut self) {
        self.available_ports = serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.port_name)
            .collect();
    }

    pub fn start(&mut self) {
        if self.config.port_name.is_empty() {
            self.last_error = Some("COMポートを選択してください".to_string());
            return;
        }

        let (data_tx, data_rx) = crossbeam_channel::unbounded();
        let (stop_tx, stop_rx) = crossbeam_channel::bounded(1);
        let idle_threshold = Duration::from_secs_f64(self.idle_threshold_ms / 1000.0);

        match worker::spawn_receiver(&self.config, idle_threshold, data_tx, stop_rx) {
            Ok(handle) => {
                self.receiver = Some(data_rx);
                self.stop_sender = Some(stop_tx);
                self.worker_handle = Some(handle);
                self.state = MonitorState::Running;
                self.last_byte_time = Some(Instant::now());
                self.last_error = None;
            }
            Err(e) => {
                self.last_error = Some(format!("ポートオープン失敗: {}", e));
            }
        }
    }

    pub fn pause(&mut self) {
        self.state = MonitorState::Paused;
    }

    pub fn resume(&mut self) {
        // 一時停止中のデータを表示バッファに同期
        self.display_buffer
            .sync_entries(self.buffer.entries(), self.idle_threshold_ms);
        self.state = MonitorState::Running;
    }

    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.receiver = None;
        self.state = MonitorState::Stopped;
        // 停止時にバッファを同期（スクロール表示用）
        self.display_buffer
            .sync_entries(self.buffer.entries(), self.idle_threshold_ms);
    }

    /// チャネルからデータをドレイン
    pub fn drain_channel(&mut self) {
        if let Some(rx) = &self.receiver {
            for entry in rx.try_iter() {
                if let DataEntry::Byte(_, ts) = &entry {
                    self.last_byte_time = Some(*ts);
                }
                self.buffer.push(entry);
            }
        }
    }

    /// バッファを全クリア
    pub fn clear_all(&mut self) {
        self.buffer.clear();
        self.display_buffer.clear();
        self.last_byte_time = None;
        self.search = SearchState::new();
    }

    pub fn save_settings(&self) {
        let settings = AppSettings {
            port_name: self.config.port_name.clone(),
            baud_rate: self.config.baud_rate,
            data_bits: self.config.data_bits,
            parity: self.config.parity.clone(),
            stop_bits: self.config.stop_bits.clone(),
            idle_threshold_ms: self.idle_threshold_ms,
            display_mode: match self.display_mode {
                DisplayMode::Hex => "Hex".to_string(),
                DisplayMode::Ascii => "Ascii".to_string(),
            },
            show_settings_window: self.ui_state.show_settings_window,
            show_search_bar: self.ui_state.show_search_bar,
        };
        settings.save();
    }
}

impl eframe::App for GlassApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // チャネルからデータ受信
        self.drain_channel();

        // キーボードショートカット
        let (ctrl_f, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::F) && i.modifiers.ctrl,
                i.key_pressed(egui::Key::Escape),
            )
        });
        if ctrl_f {
            self.ui_state.show_search_bar = !self.ui_state.show_search_bar;
        }
        if escape {
            self.ui_state.show_search_bar = false;
        }

        // ヘッダーバー（スリム1行）
        egui::Panel::top("header_bar").show_inside(ui, |ui| {
            ui::header_bar::draw(ui, self);
        });

        // ステータスバー（表示設定統合）
        egui::Panel::bottom("status_bar").show_inside(ui, |ui| {
            ui::status_bar::draw(ui, self);
        });

        // メインモニタ領域（検索バー含む）
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.ui_state.show_search_bar {
                ui::search_bar::draw(ui, self);
                ui.separator();
            }
            ui::monitor_view::draw(ui, self);
        });

        // フローティング設定ウィンドウ
        ui::settings_window::draw(ui, self);

        // Running状態では継続的に再描画
        if self.state == MonitorState::Running {
            ui.ctx().request_repaint();
        }
    }

    fn on_exit(&mut self) {
        self.stop();
        self.save_settings();
    }
}
