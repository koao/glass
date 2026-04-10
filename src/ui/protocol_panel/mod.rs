//! プロトコルパネル UI モジュール
//!
//! - mod.rs: 公開エントリ（draw, extract_*）、ツールバー、検索バー、詳細ウィンドウ、
//!   行リスト構築、共通定数・ヘルパ
//! - list_view.rs: リスト表示の仮想スクロール描画
//! - wrap_view.rs: ラップ表示（ライブ／停止）と関連レイアウト

mod list_view;
mod wrap_view;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Align, Align2, ScrollArea, Ui, Vec2};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState, ProtocolViewMode};
use crate::protocol::checksum::{ChecksumStatus, format_value};
use crate::protocol::definition;
use crate::protocol::engine::ProtocolEngine;
use crate::ui::theme;

use list_view::draw_match_list;
use wrap_view::draw_wrap_view;

/// 行の高さ
pub(super) const ROW_HEIGHT: f32 = 28.0;

/// 仮想スクロールの行へスクロール。可視行カリングの前に呼ぶこと
/// (描画ループ内に置くと対象が範囲外のとき発火しない)。
/// `relative_row` は描画開始行 (row_offset) からの相対位置。
pub(super) fn scroll_to_virtual_row(
    ui: &mut Ui,
    container: egui::Rect,
    relative_row: usize,
    row_height: f32,
    width: f32,
) {
    let y = container.min.y + relative_row as f32 * row_height;
    let target =
        egui::Rect::from_min_size(egui::pos2(container.min.x, y), Vec2::new(width, row_height));
    ui.scroll_to_rect(target, Some(Align::Center));
}

/// CRC NG バッジの色
pub(super) const CHECKSUM_NG_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 110, 110);

/// CRC NG バッジを描画して、進めるべき幅（バッジ幅 + 余白）を返す。
/// matched.checksum が Invalid のときだけ描画し、それ以外は 0 を返す。
pub(super) fn paint_checksum_ng_badge(
    painter: &egui::Painter,
    font: egui::FontId,
    x: f32,
    center_y: f32,
    status: Option<crate::protocol::checksum::ChecksumStatus>,
) -> f32 {
    if !matches!(
        status,
        Some(crate::protocol::checksum::ChecksumStatus::Invalid { .. })
    ) {
        return 0.0;
    }
    let g = painter.layout_no_wrap(regular::X_CIRCLE.to_string(), font, CHECKSUM_NG_COLOR);
    let w = g.rect.width();
    painter.galley(
        egui::pos2(x, center_y - g.rect.height() / 2.0),
        g,
        CHECKSUM_NG_COLOR,
    );
    w + 8.0
}

/// フォントID
pub(super) const FONT: fn() -> egui::FontId = || egui::FontId::proportional(15.0);
pub(super) const MONO_FONT: fn() -> egui::FontId = || egui::FontId::monospace(13.0);

/// 表示行の種類
#[derive(Clone)]
pub(super) enum RowEntry {
    /// IDLE行（時間ms）
    Idle(f64),
    /// メッセージ行（matchesインデックス、偶数行フラグ）
    Message(usize, bool),
}

/// IDLE テキストを描画
pub(super) fn paint_idle_text(painter: &egui::Painter, idle_ms: f64, x: f32, center_y: f32) {
    let text = format!("IDLE {}ms", idle_ms as u64);
    let g = painter.layout_no_wrap(text, MONO_FONT(), theme::PROTOCOL_IDLE);
    painter.galley(
        egui::pos2(x + 8.0, center_y - g.rect.height() / 2.0),
        g,
        theme::PROTOCOL_IDLE,
    );
}

/// 展開トグル処理（1つだけ表示、既存は閉じる）
pub(super) fn handle_expand_toggle(app: &mut GlassApp, toggle_id: Option<u64>) {
    if let Some(id) = toggle_id {
        if app.ui_state.protocol_expanded.contains(&id) {
            app.ui_state.protocol_expanded.clear();
        } else {
            app.ui_state.protocol_expanded.clear();
            app.ui_state.protocol_expanded.insert(id);
        }
    }
}

/// プロトコルパネル描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    if app.state == MonitorState::Running {
        app.ui_state.protocol_selection.clear();
    }

    let rows = build_row_entries(app);
    let msg_count = rows
        .iter()
        .filter(|r| matches!(r, RowEntry::Message(..)))
        .count();

    draw_toolbar(ui, app, msg_count);
    ui.separator();

    if app.ui_state.show_protocol_search_bar {
        draw_protocol_search_bar(ui, app);
        ui.separator();
    }

    if app.loaded_protocol.is_none() {
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme::TEXT_MUTED, app.t.protocol_no_file);
        });
        return;
    }

    match app.ui_state.protocol_view_mode {
        ProtocolViewMode::List => {
            if rows.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(theme::TEXT_MUTED, app.t.protocol_no_match);
                });
            } else {
                let is_stopped = app.state.is_idle();
                let scroll_to_row =
                    app.protocol_search
                        .take_scroll_target()
                        .and_then(|target_id| {
                            let target_idx = app.protocol_state.position_by_id(target_id)?;
                            rows.iter().position(
                                |r| matches!(r, RowEntry::Message(idx, _) if *idx == target_idx),
                            )
                        });
                if is_stopped {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .scroll_source(egui::scroll_area::ScrollSource {
                            drag: false,
                            ..Default::default()
                        })
                        .show(ui, |ui| {
                            draw_match_list(ui, app, &rows, false, scroll_to_row);
                        });
                } else {
                    draw_match_list(ui, app, &rows, true, None);
                }
            }
        }
        ProtocolViewMode::Wrap => {
            draw_wrap_view(ui, app);
        }
    }

    draw_filter_window(ui, app);
    draw_protocol_search_help(ui, app);
}

/// ツールバー描画
fn draw_toolbar(ui: &mut Ui, app: &mut GlassApp, visible_msg_count: usize) {
    ui.horizontal(|ui| {
        ui.set_min_height(ui.spacing().interact_size.y);

        if app.protocol_files.is_empty() {
            ui.colored_label(theme::TEXT_MUTED, "—");
        } else {
            let selected_idx = app.ui_state.selected_protocol_idx.unwrap_or(0);
            let selected_title = app
                .protocol_files
                .get(selected_idx)
                .map(|(_, t)| t.as_str())
                .unwrap_or("—");

            let titles: Vec<String> = app.protocol_files.iter().map(|(_, t)| t.clone()).collect();
            let mut new_idx: Option<usize> = None;

            egui::ComboBox::from_id_salt("protocol_select")
                .width(180.0)
                .truncate()
                .selected_text(selected_title)
                .show_ui(ui, |ui| {
                    for (i, title) in titles.iter().enumerate() {
                        if ui
                            .selectable_label(app.ui_state.selected_protocol_idx == Some(i), title)
                            .clicked()
                        {
                            new_idx = Some(i);
                        }
                    }
                });

            if let Some(idx) = new_idx {
                app.ui_state.selected_protocol_idx = Some(idx);
                load_selected_protocol(app, idx);
            }
        }

        if ui
            .button(regular::ARROWS_CLOCKWISE)
            .on_hover_text(app.t.protocol_reload)
            .clicked()
        {
            reload_protocols(app);
        }

        ui.separator();

        if ui
            .button(format!("{} {}", regular::FUNNEL, app.t.protocol_filter))
            .clicked()
        {
            app.ui_state.show_protocol_filter = !app.ui_state.show_protocol_filter;
        }

        let (mode_icon, mode_short, mode_tooltip) = match app.ui_state.protocol_view_mode {
            ProtocolViewMode::List => (
                regular::REPEAT,
                app.t.protocol_mode_wrap_short,
                app.t.protocol_mode_wrap,
            ),
            ProtocolViewMode::Wrap => (
                regular::LIST_BULLETS,
                app.t.protocol_mode_list_short,
                app.t.protocol_mode_list,
            ),
        };
        if ui
            .button(format!("{} {}", mode_icon, mode_short))
            .on_hover_text(mode_tooltip)
            .clicked()
        {
            app.ui_state.protocol_view_mode = match app.ui_state.protocol_view_mode {
                ProtocolViewMode::List => ProtocolViewMode::Wrap,
                ProtocolViewMode::Wrap => ProtocolViewMode::List,
            };
            app.ui_state.wrap.reset();
        }

        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let total = app.protocol_state.matches.len();
            if total > 0 {
                if visible_msg_count == total {
                    ui.colored_label(theme::TEXT_MUTED, format!("{} messages", total));
                } else {
                    ui.colored_label(
                        theme::TEXT_MUTED,
                        format!("{}/{} messages", visible_msg_count, total),
                    );
                }
            }
        });
    });
}

/// フィルタ設定ウィンドウ描画
fn draw_filter_window(ui: &mut Ui, app: &mut GlassApp) {
    if !app.ui_state.show_protocol_filter {
        return;
    }
    let msg_info: Vec<(String, String)> = match &app.loaded_protocol {
        Some(p) => p
            .messages
            .iter()
            .map(|m| (m.id.clone(), m.title.clone()))
            .collect(),
        None => return,
    };

    let mut open = app.ui_state.show_protocol_filter;
    egui::Window::new(app.t.protocol_filter_title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(300.0)
        .show(ui.ctx(), |ui| {
            ui.checkbox(
                &mut app.ui_state.protocol_show_idle,
                app.t.protocol_show_idle,
            );
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button(app.t.protocol_show_all).clicked() {
                    app.ui_state.protocol_hidden_ids.clear();
                }
                if ui.button(app.t.protocol_hide_all).clicked() {
                    for (id, _) in &msg_info {
                        app.ui_state.protocol_hidden_ids.insert(id.clone());
                    }
                }
            });
            ui.separator();

            ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                for (id, title) in &msg_info {
                    let mut visible = !app.ui_state.protocol_hidden_ids.contains(id);
                    if ui.checkbox(&mut visible, title).changed() {
                        if visible {
                            app.ui_state.protocol_hidden_ids.remove(id);
                        } else {
                            app.ui_state.protocol_hidden_ids.insert(id.clone());
                        }
                    }
                }
            });
        });
    app.ui_state.show_protocol_filter = open;
}

/// プロトコル検索バー描画
fn draw_protocol_search_bar(ui: &mut Ui, app: &mut GlassApp) {
    let row_height = ui.text_style_height(&egui::TextStyle::Button)
        + ui.spacing().button_padding.y * 2.0
        + ui.spacing().item_spacing.y;
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(app.t.search_label);

            let response = ui.add(
                egui::TextEdit::singleline(&mut app.protocol_search.query).desired_width(200.0),
            );
            let enter_pressed =
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            let search_clicked = ui.button(app.t.search_button).clicked() || enter_pressed;

            if ui
                .add_enabled(
                    app.protocol_search.has_searched,
                    egui::Button::new(format!("{} {}", regular::ERASER, app.t.search_clear)),
                )
                .clicked()
            {
                app.protocol_search.reset();
            }

            let is_stopped = app.state.is_idle();
            let has_results = app.protocol_search.result_count() > 0;
            let can_navigate = has_results && is_stopped;
            let prev_clicked = ui
                .add_enabled(can_navigate, egui::Button::new(regular::CARET_LEFT))
                .clicked();
            let next_clicked = ui
                .add_enabled(can_navigate, egui::Button::new(regular::CARET_RIGHT))
                .clicked();

            if search_clicked {
                app.protocol_search.search(
                    &app.protocol_state.matches,
                    app.loaded_protocol.as_ref(),
                    &app.ui_state.protocol_hidden_ids,
                );
            } else if prev_clicked {
                app.protocol_search.prev();
            } else if next_clicked {
                app.protocol_search.next();
            }

            if app.protocol_search.has_searched {
                let count = app.protocol_search.result_count();
                if count > 0 {
                    ui.label(format!(
                        "{}/{}",
                        app.protocol_search.current_index() + 1,
                        count
                    ));
                } else {
                    ui.colored_label(theme::TEXT_MUTED, app.t.protocol_search_no_match);
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(format!("{} {}", regular::INFO, app.t.help))
                    .clicked()
                {
                    app.ui_state.show_protocol_search_help =
                        !app.ui_state.show_protocol_search_help;
                }
            });
        },
    );
}

/// プロトコル検索ヘルプウィンドウ描画
fn draw_protocol_search_help(ui: &mut Ui, app: &mut GlassApp) {
    if !app.ui_state.show_protocol_search_help {
        return;
    }
    egui::Window::new(app.t.protocol_search_help_title)
        .id(egui::Id::new("protocol_search_help_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(320.0)
        .open(&mut app.ui_state.show_protocol_search_help)
        .show(ui.ctx(), |ui| {
            ui.label(app.t.protocol_search_help_desc);
            ui.add_space(6.0);

            egui::Grid::new("proto_search_help_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.monospace("A AND B");
                    ui.label(app.t.protocol_search_help_and);
                    ui.end_row();

                    ui.monospace("A OR B");
                    ui.label(app.t.protocol_search_help_or);
                    ui.end_row();

                    ui.monospace("$XX");
                    ui.label(app.t.protocol_search_help_hex);
                    ui.end_row();

                    ui.monospace("\"A B\"");
                    ui.label(app.t.protocol_search_help_quote);
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.colored_label(theme::TEXT_MUTED, "例 / Examples:");
            ui.add_space(2.0);

            egui::Grid::new("proto_search_help_examples")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.monospace("応答 宛先:001");
                    ui.label("→ AND (2語)");
                    ui.end_row();

                    ui.monospace("\"宛先:0 1\"");
                    ui.label("→ 1語 (スペース含む)");
                    ui.end_row();

                    ui.monospace("MsgA OR MsgB");
                    ui.label("→ OR");
                    ui.end_row();

                    ui.monospace("$02$03");
                    ui.label("→ HEX bytes");
                    ui.end_row();
                });
        });
}

/// 表示行リストを構築（フィルタ・IDLE挿入済み）
pub(super) fn build_row_entries(app: &GlassApp) -> Vec<RowEntry> {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return Vec::new(),
    };
    let show_idle = app.ui_state.protocol_show_idle;
    let mut rows = Vec::new();
    let mut msg_count = 0usize;

    for (i, matched) in app.protocol_state.matches.iter().enumerate() {
        if let Some(def_idx) = matched.message_def_idx
            && app
                .ui_state
                .protocol_hidden_ids
                .contains(&proto.messages[def_idx].id)
        {
            continue;
        }
        if show_idle && let Some(idle_ms) = matched.preceding_idle_ms {
            rows.push(RowEntry::Idle(idle_ms));
        }
        rows.push(RowEntry::Message(i, msg_count.is_multiple_of(2)));
        msg_count += 1;
    }
    rows
}

/// 展開中メッセージの詳細をフローティングウィンドウで表示
pub(super) fn draw_expanded_windows(ui: &mut Ui, app: &mut GlassApp) {
    let expanded: Vec<u64> = app.ui_state.protocol_expanded.iter().copied().collect();
    let mut to_close: Vec<u64> = Vec::new();

    let titles: Vec<(u64, usize, String)> = expanded
        .iter()
        .filter_map(|&id| {
            let idx = app.protocol_state.position_by_id(id)?;
            let matched = &app.protocol_state.matches[idx];
            let title = match matched.message_def_idx {
                Some(def_idx) => app
                    .loaded_protocol
                    .as_ref()
                    .map(|p| p.messages[def_idx].title.clone())
                    .unwrap_or_default(),
                None => format!("{} #{}", app.t.protocol_unmatched, id),
            };
            Some((id, idx, title))
        })
        .collect();

    for &id in &expanded {
        if !titles.iter().any(|(eid, _, _)| *eid == id) {
            to_close.push(id);
        }
    }

    let default_pos = ui.ctx().content_rect().center();

    for (id, match_idx, title) in &titles {
        let id = *id;
        let match_idx = *match_idx;
        let mut open = true;
        egui::Window::new(format!("#{} {}", id, title))
            .id(egui::Id::new(("proto_detail", id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(500.0)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(default_pos)
            .show(ui.ctx(), |ui| {
                draw_expanded_detail(ui, app, match_idx);
            });
        if !open {
            to_close.push(id);
        }
    }

    for id in to_close {
        app.ui_state.protocol_expanded.remove(&id);
    }
}

/// 展開時の詳細描画
fn draw_expanded_detail(ui: &mut Ui, app: &GlassApp, match_idx: usize) {
    let matched = &app.protocol_state.matches[match_idx];
    let proto = app.loaded_protocol.as_ref().unwrap();

    if let Some(def_idx) = matched.message_def_idx {
        let msg_def = &proto.messages[def_idx];
        if !msg_def.fields.is_empty() {
            ui.colored_label(theme::TEXT_MUTED, format!("{}:", app.t.protocol_fields));
            egui::Grid::new(format!("fields_{}", match_idx))
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Offset");
                    ui.strong("Size");
                    ui.strong("Name");
                    ui.strong("HEX");
                    ui.strong("ASCII");
                    ui.strong("Description");
                    ui.end_row();

                    for field in &msg_def.fields {
                        ui.label(format!("{}", field.offset));
                        ui.label(format!("{}", field.size));
                        ui.label(&field.name);

                        let hex_val = extract_hex(&matched.frame.bytes, field.offset, field.size);
                        ui.monospace(&hex_val);

                        let ascii_val =
                            extract_ascii(&matched.frame.bytes, field.offset, field.size);
                        ui.monospace(&ascii_val);

                        let desc = field.description.as_deref().unwrap_or("");
                        ui.label(desc);
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
        }
    }

    // チェックサム検証結果
    if let Some(status) = matched.checksum
        && let Some(spec) = matched
            .frame
            .bytes
            .first()
            .and_then(|first| app.protocol_engine.as_ref()?.find_rule(*first))
            .and_then(|rule| rule.checksum.as_ref())
    {
        let algo_label = spec.algorithm.label();
        let size = spec.effective_size();

        match status {
            ChecksumStatus::Valid { value } => {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 200, 140),
                    format!(
                        "{}: {} {} {} ({})",
                        app.t.protocol_checksum,
                        algo_label,
                        regular::CHECK_CIRCLE,
                        app.t.protocol_checksum_ok,
                        format_value(value, size)
                    ),
                );
            }
            ChecksumStatus::Invalid { expected, actual } => {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 110, 110),
                    format!(
                        "{}: {} {} {} ({} {} / {} {})",
                        app.t.protocol_checksum,
                        algo_label,
                        regular::X_CIRCLE,
                        app.t.protocol_checksum_ng,
                        app.t.protocol_checksum_expected,
                        format_value(expected, size),
                        app.t.protocol_checksum_actual,
                        format_value(actual, size),
                    ),
                );
            }
            ChecksumStatus::NotApplicable => {}
        }
        ui.add_space(2.0);
    }

    ui.colored_label(theme::TEXT_MUTED, format!("{}:", app.t.protocol_raw));
    let hex_dump = extract_hex(&matched.frame.bytes, 0, matched.frame.bytes.len());
    ui.monospace(&hex_dump);
}

/// バイト列からHEX文字列を抽出
pub(crate) fn extract_hex(bytes: &[u8], offset: usize, size: usize) -> String {
    if offset >= bytes.len() {
        return "—".to_string();
    }
    let end = (offset + size).min(bytes.len());
    bytes[offset..end]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// バイト列からASCII文字列を抽出
pub(crate) fn extract_ascii(bytes: &[u8], offset: usize, size: usize) -> String {
    if offset >= bytes.len() {
        return "—".to_string();
    }
    let end = (offset + size).min(bytes.len());
    bytes[offset..end]
        .iter()
        .map(|b| {
            if *b >= 0x20 && *b <= 0x7E {
                *b as char
            } else {
                '.'
            }
        })
        .collect()
}

/// フィルタ状態のハッシュを計算（変更検知用）
pub(super) fn compute_filter_hash(app: &GlassApp) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut ids: Vec<&String> = app.ui_state.protocol_hidden_ids.iter().collect();
    ids.sort();
    for id in ids {
        id.hash(&mut hasher);
    }
    app.ui_state.protocol_show_idle.hash(&mut hasher);
    hasher.finish()
}

/// 選択されたプロトコル定義を読み込む
fn load_selected_protocol(app: &mut GlassApp, idx: usize) {
    if let Some((path, _)) = app.protocol_files.get(idx) {
        match definition::load_protocol(path) {
            Ok(proto) => {
                let engine = ProtocolEngine::new(&proto);
                app.protocol_state.clear();
                app.protocol_state
                    .sync_entries(app.buffer.entries(), &engine);
                app.protocol_state.flush(&engine);
                app.protocol_engine = Some(engine);
                app.loaded_protocol = Some(proto);
                app.ui_state.protocol_expanded.clear();
                app.ui_state.protocol_hidden_ids.clear();
                app.ui_state.wrap.reset();
                app.protocol_search.clear();
            }
            Err(e) => {
                app.show_error(&e);
            }
        }
    }
}

/// プロトコル定義ファイル一覧を再スキャン
fn reload_protocols(app: &mut GlassApp) {
    let dir = definition::protocols_dir();
    app.protocol_files = definition::scan_protocols(&dir);
    if let Some(idx) = app.ui_state.selected_protocol_idx {
        if idx < app.protocol_files.len() {
            load_selected_protocol(app, idx);
        } else if !app.protocol_files.is_empty() {
            app.ui_state.selected_protocol_idx = Some(0);
            load_selected_protocol(app, 0);
        } else {
            app.ui_state.selected_protocol_idx = None;
            app.protocol_engine = None;
            app.loaded_protocol = None;
            app.protocol_state.clear();
        }
    } else if !app.protocol_files.is_empty() {
        app.ui_state.selected_protocol_idx = Some(0);
        load_selected_protocol(app, 0);
    }
}
