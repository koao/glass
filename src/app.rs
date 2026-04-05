use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;

/// ウィンドウ最小サイズ
pub const MIN_WINDOW_SIZE: [f32; 2] = [800.0, 400.0];

use crate::i18n::{Language, Texts};
use crate::model::buffer::MonitorBuffer;
use crate::model::entry::DataEntry;
use crate::model::file_format::GlassFile;
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
    /// 検索ヘルプウィンドウ表示フラグ
    pub show_search_help: bool,
    /// 設定ウィンドウの選択タブ
    pub settings_tab: SettingsTab,
    /// スクリーンショット要求フラグ（ボタン押下時にtrue）
    pub screenshot_requested: bool,
    /// スクリーンショット結果待ちフラグ（ViewportCommand送信後にtrue）
    pub screenshot_pending: bool,
    /// 最小ウィンドウサイズ適用済みフラグ
    pub min_size_applied: bool,
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
    pub lang: Language,
    pub t: &'static Texts,
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
        let lang = settings.language;

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
                show_search_help: false,
                settings_tab: SettingsTab::Serial,
                screenshot_requested: false,
                screenshot_pending: false,
                min_size_applied: false,
            },
            lang,
            t: lang.texts(),
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

        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
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
            self.last_error = Some(self.t.err_no_port.to_string());
            return;
        }

        self.clear_all();

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
                self.last_error = Some(format!("{}: {}", self.t.err_port_open, e));
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

    /// バッファをファイルに保存
    pub fn save_to_file(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("Glass Monitor", &["glm"])
            .set_file_name("monitor.glm")
            .save_file();
        if let Some(path) = path {
            let glass_file = GlassFile::from_entries(self.buffer.entries());
            match serde_json::to_string_pretty(&glass_file) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&path, json) {
                        self.last_error = Some(format!("{}: {}", self.t.err_save_file, e));
                    }
                }
                Err(e) => {
                    self.last_error = Some(format!("{}: {}", self.t.err_save_file, e));
                }
            }
        }
    }

    /// ファイルからバッファに読み込み
    pub fn load_from_file(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter("Glass Monitor", &["glm"])
            .pick_file();
        if let Some(path) = path {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<GlassFile>(&content) {
                    Ok(glass_file) => {
                        let entries = glass_file.to_entries();
                        self.buffer.load_entries(entries);
                        self.display_buffer
                            .sync_entries(self.buffer.entries(), self.idle_threshold_ms);
                        self.search = SearchState::new();
                        self.last_error = None;
                    }
                    Err(e) => {
                        self.last_error = Some(format!("{}: {}", self.t.err_load_file, e));
                    }
                },
                Err(e) => {
                    self.last_error = Some(format!("{}: {}", self.t.err_load_file, e));
                }
            }
        }
    }

    /// スクリーンショットをPNG保存
    fn save_screenshot(&mut self, image: &Arc<egui::ColorImage>) {
        let path = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("glass_screenshot.png")
            .save_file();
        if let Some(path) = path {
            let [w, h] = image.size;
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                if let Err(e) = img.save(&path) {
                    self.last_error = Some(format!("{}: {}", self.t.err_screenshot, e));
                }
            }
        }
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
            language: self.lang,
        };
        settings.save();
    }
}

impl eframe::App for GlassApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ウィンドウ最小サイズを設定（NativeOptions だけでは効かないため初回のみ送信）
        if !self.ui_state.min_size_applied {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                egui::vec2(MIN_WINDOW_SIZE[0], MIN_WINDOW_SIZE[1]),
            ));
            self.ui_state.min_size_applied = true;
        }

        // スクリーンショット結果の受信（要求後のフレームのみ検査）
        if self.ui_state.screenshot_pending {
            let screenshot_image = ui.input(|i| {
                i.events.iter().find_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot_image {
                self.ui_state.screenshot_pending = false;
                self.save_screenshot(&image);
            }
        }

        // チャネルからデータ受信
        self.drain_channel();

        // 受信中の検索自動更新
        if self.state == MonitorState::Running && self.ui_state.show_search_bar {
            let entries = self.buffer.entries();
            self.search.auto_refresh(entries);
        }

        // キーボードショートカット
        let (ctrl_f, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::F) && i.modifiers.ctrl,
                i.key_pressed(egui::Key::Escape),
            )
        });
        if ctrl_f {
            self.ui_state.show_search_bar = !self.ui_state.show_search_bar;
            if !self.ui_state.show_search_bar {
                self.search.reset();
            }
        }
        if escape && self.ui_state.show_search_bar {
            self.ui_state.show_search_bar = false;
            self.search.reset();
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

        // フローティングウィンドウ
        ui::settings_window::draw(ui, self);
        ui::search_bar::draw_help(ui, self);

        // スクリーンショット要求の送信
        if self.ui_state.screenshot_requested {
            self.ui_state.screenshot_requested = false;
            self.ui_state.screenshot_pending = true;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }

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
