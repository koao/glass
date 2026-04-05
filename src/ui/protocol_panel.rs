use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Align, Align2, Rect, ScrollArea, Sense, Ui, Vec2};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState, ProtocolViewMode, WrapSlot, WrapSlotKind, WrapViewState};
use crate::protocol::definition;
use crate::protocol::engine::ProtocolEngine;
use crate::ui::theme;

/// 行の高さ
const ROW_HEIGHT: f32 = 28.0;

/// フォントID
const FONT: fn() -> egui::FontId = || egui::FontId::proportional(15.0);
const MONO_FONT: fn() -> egui::FontId = || egui::FontId::monospace(13.0);

/// 方向アイコン・色・ラベルを取得
fn direction_info(dir: &str, t: &crate::i18n::Texts) -> (&'static str, egui::Color32, &'static str) {
    match dir {
        "send" => (regular::ARROW_RIGHT, theme::PROTOCOL_SEND, t.protocol_send),
        "receive" => (regular::ARROW_LEFT, theme::PROTOCOL_RECV, t.protocol_recv),
        _ => (regular::MINUS, theme::TEXT_MUTED, ""),
    }
}

/// IDLE テキストを描画
fn paint_idle_text(painter: &egui::Painter, idle_ms: f64, x: f32, center_y: f32) {
    let text = format!("IDLE {}ms", idle_ms as u64);
    let g = painter.layout_no_wrap(text, MONO_FONT(), theme::PROTOCOL_IDLE);
    painter.galley(
        egui::pos2(x + 8.0, center_y - g.rect.height() / 2.0),
        g,
        theme::PROTOCOL_IDLE,
    );
}

/// 展開トグル処理（1つだけ表示、既存は閉じる）
fn handle_expand_toggle(app: &mut GlassApp, toggle_idx: Option<usize>) {
    if let Some(idx) = toggle_idx {
        if app.ui_state.protocol_expanded.contains(&idx) {
            app.ui_state.protocol_expanded.clear();
        } else {
            app.ui_state.protocol_expanded.clear();
            app.ui_state.protocol_expanded.insert(idx);
        }
    }
}

/// スロット行の描画（メッセージ・IDLE描画＋クリック判定）
fn paint_wrap_slots(
    ui: &mut Ui,
    painter: &egui::Painter,
    app: &GlassApp,
    slots: &[WrapSlot],
    rect_min_x: f32,
    row_rect: &Rect,
    row_h: f32,
    id_salt: &str,
) -> Option<usize> {
    let center_y = row_rect.center().y;
    let mut clicked = None;
    for slot in slots {
        let slot_x = rect_min_x + slot.x;
        match &slot.kind {
            WrapSlotKind::Message(match_idx) => {
                if *match_idx < app.protocol_state.matches.len() {
                    paint_inline_message(painter, app, *match_idx, slot_x, center_y, slot.width, row_h);
                    let slot_rect = Rect::from_min_size(
                        egui::pos2(slot_x, row_rect.min.y),
                        Vec2::new(slot.width, row_h),
                    );
                    let resp = ui.interact(slot_rect, egui::Id::new((id_salt, *match_idx)), Sense::click());
                    if resp.clicked() {
                        clicked = Some(*match_idx);
                    }
                }
            }
            WrapSlotKind::Idle(idle_ms) => {
                paint_idle_text(painter, *idle_ms, slot_x, center_y);
            }
        }
    }
    clicked
}

/// プロトコルパネル描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    // 表示行リストを1回だけ構築
    let rows = build_row_entries(app);
    let msg_count = rows.iter().filter(|r| matches!(r, RowEntry::Message(..))).count();

    // ツールバー
    draw_toolbar(ui, app, msg_count);
    ui.separator();

    // 定義未読込の場合
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
                let is_stopped = app.state == MonitorState::Stopped;
                if is_stopped {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            draw_match_list(ui, app, &rows, false);
                        });
                } else {
                    draw_match_list(ui, app, &rows, true);
                }
            }
        }
        ProtocolViewMode::Wrap => {
            draw_wrap_view(ui, app);
        }
    }

    // フィルタウィンドウ（フローティング）
    draw_filter_window(ui, app);
}

/// ツールバー描画
fn draw_toolbar(ui: &mut Ui, app: &mut GlassApp, visible_msg_count: usize) {
    ui.horizontal(|ui| {
        ui.set_min_height(ui.spacing().interact_size.y);

        if app.protocol_files.is_empty() {
            ui.colored_label(theme::TEXT_MUTED, "—");
        } else {
            let selected_idx = app.ui_state.selected_protocol_idx.unwrap_or(0);
            let selected_title = app.protocol_files.get(selected_idx)
                .map(|(_, t)| t.as_str())
                .unwrap_or("—");

            let titles: Vec<String> = app.protocol_files.iter().map(|(_, t)| t.clone()).collect();
            let mut new_idx: Option<usize> = None;

            egui::ComboBox::from_id_salt("protocol_select")
                .selected_text(selected_title)
                .show_ui(ui, |ui| {
                    for (i, title) in titles.iter().enumerate() {
                        if ui.selectable_label(
                            app.ui_state.selected_protocol_idx == Some(i),
                            title,
                        ).clicked() {
                            new_idx = Some(i);
                        }
                    }
                });

            if let Some(idx) = new_idx {
                app.ui_state.selected_protocol_idx = Some(idx);
                load_selected_protocol(app, idx);
            }
        }

        // 再読み込みボタン
        if ui.button(regular::ARROWS_CLOCKWISE)
            .on_hover_text(app.t.protocol_reload)
            .clicked()
        {
            reload_protocols(app);
        }

        // フィルタボタン
        if ui.button(regular::FUNNEL)
            .on_hover_text(app.t.protocol_filter)
            .clicked()
        {
            app.ui_state.show_protocol_filter = !app.ui_state.show_protocol_filter;
        }

        // 表示モード切り替えボタン
        let (mode_icon, mode_tooltip) = match app.ui_state.protocol_view_mode {
            ProtocolViewMode::List => (regular::REPEAT, app.t.protocol_mode_wrap),
            ProtocolViewMode::Wrap => (regular::LIST_BULLETS, app.t.protocol_mode_list),
        };
        if ui.button(mode_icon)
            .on_hover_text(mode_tooltip)
            .clicked()
        {
            app.ui_state.protocol_view_mode = match app.ui_state.protocol_view_mode {
                ProtocolViewMode::List => ProtocolViewMode::Wrap,
                ProtocolViewMode::Wrap => ProtocolViewMode::List,
            };
            app.ui_state.wrap.reset();
        }

        // マッチ数表示（右寄せ）
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let total = app.protocol_state.matches.len();
            if total > 0 {
                if visible_msg_count == total {
                    ui.colored_label(theme::TEXT_MUTED, format!("{} messages", total));
                } else {
                    ui.colored_label(theme::TEXT_MUTED, format!("{}/{} messages", visible_msg_count, total));
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
    // clone回避: ID・タイトルだけ抽出
    let msg_info: Vec<(String, String)> = match &app.loaded_protocol {
        Some(p) => p.messages.iter().map(|m| (m.id.clone(), m.title.clone())).collect(),
        None => return,
    };

    let mut open = app.ui_state.show_protocol_filter;
    egui::Window::new(app.t.protocol_filter_title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(300.0)
        .show(ui.ctx(), |ui| {
            ui.checkbox(&mut app.ui_state.protocol_show_idle, app.t.protocol_show_idle);
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

            ScrollArea::vertical()
                .max_height(400.0)
                .show(ui, |ui| {
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

/// 表示行の種類
#[derive(Clone)]
enum RowEntry {
    /// IDLE行（時間ms）
    Idle(f64),
    /// メッセージ行（matchesインデックス、偶数行フラグ）
    Message(usize, bool),
}

/// 表示行リストを構築（フィルタ・IDLE挿入済み）
fn build_row_entries(app: &GlassApp) -> Vec<RowEntry> {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return Vec::new(),
    };
    let show_idle = app.ui_state.protocol_show_idle;
    let mut rows = Vec::new();
    let mut msg_count = 0usize;

    for (i, matched) in app.protocol_state.matches.iter().enumerate() {
        // フィルタチェック
        if let Some(def_idx) = matched.message_def_idx {
            if app.ui_state.protocol_hidden_ids.contains(&proto.messages[def_idx].id) {
                continue;
            }
        }
        // IDLE行を挿入
        if show_idle {
            if let Some(idle_ms) = matched.preceding_idle_ms {
                rows.push(RowEntry::Idle(idle_ms));
            }
        }
        rows.push(RowEntry::Message(i, msg_count % 2 == 0));
        msg_count += 1;
    }
    rows
}

/// マッチ結果一覧描画（仮想スクロール）
fn draw_match_list(ui: &mut Ui, app: &mut GlassApp, rows: &[RowEntry], latest_only: bool) {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return,
    };
    let total_rows = rows.len();
    if total_rows == 0 {
        return;
    }

    let expanded = &app.ui_state.protocol_expanded;
    let row_h = ROW_HEIGHT;
    let available_width = ui.available_width();
    let available_height = ui.available_height();

    // 表示範囲
    let (draw_first, draw_last, total_height);
    if latest_only {
        let max_rows = (available_height / row_h).floor() as usize;
        let display_rows = max_rows.min(total_rows);
        draw_first = total_rows - display_rows;
        draw_last = total_rows;
        total_height = display_rows as f32 * row_h;
    } else {
        total_height = total_rows as f32 * row_h;
        draw_first = 0;
        draw_last = total_rows;
    }

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(available_width, total_height),
        Sense::hover(),
    );

    // 実際に描画する範囲
    let (draw_f, draw_l) = if latest_only {
        (draw_first, draw_last)
    } else {
        let clip = ui.clip_rect();
        let visible_top = (clip.min.y - rect.min.y).max(0.0);
        let visible_bottom = (clip.max.y - rect.min.y).max(0.0);
        let f = (visible_top / row_h).floor() as usize;
        let l = ((visible_bottom / row_h).ceil() as usize).min(total_rows);
        (f, l)
    };

    let mut toggle_idx: Option<usize> = None;
    let painter = ui.painter_at(rect);
    let row_offset = if latest_only { draw_first } else { 0 };
    let font = FONT();
    let mono_font = MONO_FONT();

    for row_idx in draw_f..draw_l {
        let y_offset = (row_idx - row_offset) as f32 * row_h;
        let row_rect = Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + y_offset),
            Vec2::new(available_width, ROW_HEIGHT),
        );
        let center_y = row_rect.center().y;

        match &rows[row_idx] {
            RowEntry::Idle(idle_ms) => {
                paint_idle_text(&painter, *idle_ms, row_rect.min.x + 8.0, center_y);
            }
            RowEntry::Message(match_idx, even) => {
                let matched = &app.protocol_state.matches[*match_idx];

                // 行背景色
                let bg = if *even { theme::PROTOCOL_ROW_EVEN } else { theme::PROTOCOL_ROW_ODD };
                painter.rect_filled(row_rect, 0.0, bg);

                let is_expanded = expanded.contains(match_idx);
                let text_x = row_rect.min.x + 8.0;

                // 展開三角
                let arrow = if is_expanded { regular::CARET_DOWN } else { regular::CARET_RIGHT };
                let arrow_g = painter.layout_no_wrap(arrow.to_string(), font.clone(), theme::TEXT_MUTED);
                let arrow_w = arrow_g.rect.width();
                painter.galley(egui::pos2(text_x, center_y - arrow_g.rect.height() / 2.0), arrow_g, theme::TEXT_MUTED);
                let mut cur_x = text_x + arrow_w + 6.0;

                match matched.message_def_idx {
                    Some(def_idx) => {
                        let msg_def = &proto.messages[def_idx];

                        // 方向
                        if let Some(dir) = &msg_def.direction {
                            let (icon, color, label) = direction_info(dir, app.t);
                            let dir_text = format!("{} {}", icon, label);
                            let g = painter.layout_no_wrap(dir_text, font.clone(), color);
                            let w = g.rect.width();
                            painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, color);
                            cur_x += w + 8.0;
                        }

                        // タイトル
                        let title = &msg_def.title;
                        let title_color = msg_def.parsed_color.unwrap_or(egui::Color32::WHITE);
                        let g = painter.layout_no_wrap(title.to_string(), font.clone(), title_color);
                        let w = g.rect.width();
                        painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, title_color);
                        cur_x += w + 8.0;

                        // インラインフィールド
                        for field in msg_def.fields.iter().filter(|f| f.inline) {
                            let name = &field.name;
                            let ascii = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                            let text = format!("{}:{}", name, ascii);
                            let g = painter.layout_no_wrap(text, mono_font.clone(), theme::TEXT_MUTED);
                            let w = g.rect.width();
                            painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, theme::TEXT_MUTED);
                            cur_x += w + 8.0;
                        }
                    }
                    None => {
                        let text = format!("{} {}", regular::QUESTION, app.t.protocol_unmatched);
                        let g = painter.layout_no_wrap(text, font.clone(), theme::PROTOCOL_UNMATCHED);
                        painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, theme::PROTOCOL_UNMATCHED);
                    }
                }

                // バイト数（右寄せ）
                let size_text = format!("{}B", matched.frame.bytes.len());
                let g = painter.layout_no_wrap(size_text, mono_font.clone(), theme::TEXT_MUTED);
                let right_x = row_rect.max.x - g.rect.width() - 8.0;
                painter.galley(egui::pos2(right_x, center_y - g.rect.height() / 2.0), g, theme::TEXT_MUTED);

                // クリック判定（停止中のみ）
                if !latest_only {
                    let click_resp = ui.interact(row_rect, egui::Id::new(("proto_row", *match_idx)), Sense::click());
                    if click_resp.clicked() {
                        toggle_idx = Some(*match_idx);
                    }
                }
            }
        }
    }

    handle_expand_toggle(app, toggle_idx);
    draw_expanded_windows(ui, app);
}

/// 展開中メッセージの詳細をフローティングウィンドウで表示
fn draw_expanded_windows(ui: &mut Ui, app: &mut GlassApp) {
    let expanded: Vec<usize> = app.ui_state.protocol_expanded.iter().copied().collect();
    let mut to_close: Vec<usize> = Vec::new();

    // ウィンドウタイトルを事前収集（clone回避）
    let titles: Vec<(usize, String)> = expanded.iter().map(|&idx| {
        let title = if idx < app.protocol_state.matches.len() {
            let matched = &app.protocol_state.matches[idx];
            match matched.message_def_idx {
                Some(def_idx) => app.loaded_protocol.as_ref()
                    .map(|p| p.messages[def_idx].title.clone())
                    .unwrap_or_default(),
                None => format!("{} #{}", app.t.protocol_unmatched, idx),
            }
        } else {
            String::new()
        };
        (idx, title)
    }).collect();

    let default_pos = ui.ctx().content_rect().center();

    for (match_idx, title) in &titles {
        let match_idx = *match_idx;
        if match_idx >= app.protocol_state.matches.len() {
            to_close.push(match_idx);
            continue;
        }

        let mut open = true;
        egui::Window::new(format!("#{} {}", match_idx, title))
            .id(egui::Id::new(("proto_detail", match_idx)))
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
            to_close.push(match_idx);
        }
    }

    for idx in to_close {
        app.ui_state.protocol_expanded.remove(&idx);
    }
}

/// 展開時の詳細描画
fn draw_expanded_detail(ui: &mut Ui, app: &GlassApp, match_idx: usize) {
    let matched = &app.protocol_state.matches[match_idx];
    let proto = app.loaded_protocol.as_ref().unwrap();

    // フィールド表示
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

                        let ascii_val = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                        ui.monospace(&ascii_val);

                        let desc = field.description.as_deref().unwrap_or("");
                        ui.label(desc);
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
        }
    }

    // 生データ表示
    ui.colored_label(theme::TEXT_MUTED, format!("{}:", app.t.protocol_raw));
    let hex_dump = extract_hex(&matched.frame.bytes, 0, matched.frame.bytes.len());
    ui.monospace(&hex_dump);
}

/// バイト列からHEX文字列を抽出
fn extract_hex(bytes: &[u8], offset: usize, size: usize) -> String {
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
fn extract_ascii(bytes: &[u8], offset: usize, size: usize) -> String {
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
fn compute_filter_hash(app: &GlassApp) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut ids: Vec<&String> = app.ui_state.protocol_hidden_ids.iter().collect();
    ids.sort();
    for id in ids {
        id.hash(&mut hasher);
    }
    app.ui_state.protocol_show_idle.hash(&mut hasher);
    hasher.finish()
}

/// メッセージのインライン表示幅を計測
fn measure_message_width(ui: &Ui, app: &GlassApp, match_idx: usize) -> f32 {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return 0.0,
    };
    let matched = &app.protocol_state.matches[match_idx];
    let font = FONT();
    let mono_font = MONO_FONT();
    let painter = ui.painter();
    let mut w = 8.0; // 左マージン

    match matched.message_def_idx {
        Some(def_idx) => {
            let msg_def = &proto.messages[def_idx];
            if let Some(dir) = &msg_def.direction {
                let (icon, _, label) = direction_info(dir, app.t);
                let dir_text = format!("{} {}", icon, label);
                w += painter.layout_no_wrap(dir_text, font.clone(), egui::Color32::WHITE).rect.width() + 8.0;
            }
            w += painter.layout_no_wrap(msg_def.title.clone(), font.clone(), egui::Color32::WHITE).rect.width() + 8.0;
            for field in msg_def.fields.iter().filter(|f| f.inline) {
                let ascii = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                let text = format!("{}:{}", field.name, ascii);
                w += painter.layout_no_wrap(text, mono_font.clone(), egui::Color32::WHITE).rect.width() + 8.0;
            }
        }
        None => {
            let text = format!("{} {}", regular::QUESTION, app.t.protocol_unmatched);
            w += painter.layout_no_wrap(text, font.clone(), egui::Color32::WHITE).rect.width() + 8.0;
        }
    }
    w + 8.0 // 右マージン
}

/// IDLE テキストの表示幅を計測
fn measure_idle_width(painter: &egui::Painter, idle_ms: f64) -> f32 {
    let text = format!("IDLE {}ms", idle_ms as u64);
    painter.layout_no_wrap(text, MONO_FONT(), egui::Color32::WHITE).rect.width() + 16.0
}

/// インラインメッセージを描画（ラップ表示用、ピル背景付き）
fn paint_inline_message(
    painter: &egui::Painter,
    app: &GlassApp,
    match_idx: usize,
    x: f32,
    center_y: f32,
    width: f32,
    row_h: f32,
) {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return,
    };
    if match_idx >= app.protocol_state.matches.len() {
        return;
    }

    // ピル背景
    let pill_margin = 2.0;
    let pill_rect = Rect::from_min_size(
        egui::pos2(x + pill_margin, center_y - row_h / 2.0 + pill_margin),
        Vec2::new(width - pill_margin * 2.0, row_h - pill_margin * 2.0),
    );
    painter.rect(pill_rect, 4.0, theme::WRAP_PILL_BG, egui::Stroke::new(1.0, theme::WRAP_PILL_BORDER), egui::StrokeKind::Inside);

    let matched = &app.protocol_state.matches[match_idx];
    let font = FONT();
    let mono_font = MONO_FONT();
    let mut cur_x = x + 8.0;

    match matched.message_def_idx {
        Some(def_idx) => {
            let msg_def = &proto.messages[def_idx];
            if let Some(dir) = &msg_def.direction {
                let (icon, color, label) = direction_info(dir, app.t);
                let dir_text = format!("{} {}", icon, label);
                let g = painter.layout_no_wrap(dir_text, font.clone(), color);
                let w = g.rect.width();
                painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, color);
                cur_x += w + 8.0;
            }
            let title_color = msg_def.parsed_color.unwrap_or(egui::Color32::WHITE);
            let g = painter.layout_no_wrap(msg_def.title.clone(), font.clone(), title_color);
            let w = g.rect.width();
            painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, title_color);
            cur_x += w + 8.0;
            for field in msg_def.fields.iter().filter(|f| f.inline) {
                let ascii = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                let text = format!("{}:{}", field.name, ascii);
                let g = painter.layout_no_wrap(text, mono_font.clone(), theme::TEXT_MUTED);
                let w = g.rect.width();
                painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, theme::TEXT_MUTED);
                cur_x += w + 8.0;
            }
        }
        None => {
            let text = format!("{} {}", regular::QUESTION, app.t.protocol_unmatched);
            let g = painter.layout_no_wrap(text, font.clone(), theme::PROTOCOL_UNMATCHED);
            painter.galley(egui::pos2(cur_x, center_y - g.rect.height() / 2.0), g, theme::PROTOCOL_UNMATCHED);
        }
    }
}

/// ラップ表示描画
fn draw_wrap_view(ui: &mut Ui, app: &mut GlassApp) {
    if app.loaded_protocol.is_none() {
        return;
    }

    // 停止中はキャッシュ済みレイアウトでスクロール表示
    if app.state == MonitorState::Stopped {
        draw_wrap_view_stopped(ui, app);
        return;
    }

    let row_h = ROW_HEIGHT;
    let available_width = ui.available_width();
    let available_height = ui.available_height();
    let max_rows = (available_height / row_h).floor().max(1.0) as usize;

    // 画面サイズ変更やフィルタ変更時はリセット
    let filter_hash = compute_filter_hash(app);
    let wrap = &app.ui_state.wrap;
    if max_rows != wrap.max_rows
        || (available_width - wrap.available_width).abs() > 1.0
        || filter_hash != wrap.filter_hash
    {
        app.ui_state.wrap.reset();
        app.ui_state.wrap.max_rows = max_rows;
        app.ui_state.wrap.available_width = available_width;
        app.ui_state.wrap.filter_hash = filter_hash;
    }

    // スロット配列の初期化
    if app.ui_state.wrap.slots.len() != max_rows {
        app.ui_state.wrap.slots.resize(max_rows, Vec::new());
    }

    // matches数が減った場合（clear等）はリセット
    let total_matches = app.protocol_state.matches.len();
    if total_matches < app.ui_state.wrap.rendered_count {
        app.ui_state.wrap.reset();
        app.ui_state.wrap.max_rows = max_rows;
        app.ui_state.wrap.available_width = available_width;
        app.ui_state.wrap.filter_hash = filter_hash;
        app.ui_state.wrap.slots.resize(max_rows, Vec::new());
    }

    // 新規メッセージのレイアウト計算
    let proto = app.loaded_protocol.as_ref().unwrap();
    let show_idle = app.ui_state.protocol_show_idle;
    let start = app.ui_state.wrap.rendered_count;
    for i in start..total_matches {
        let matched = &app.protocol_state.matches[i];
        if let Some(def_idx) = matched.message_def_idx {
            if app.ui_state.protocol_hidden_ids.contains(&proto.messages[def_idx].id) {
                continue;
            }
        }

        if show_idle {
            if let Some(idle_ms) = matched.preceding_idle_ms {
                let idle_width = measure_idle_width(ui.painter(), idle_ms);
                wrap_push_slot(&mut app.ui_state.wrap, max_rows, available_width, WrapSlotKind::Idle(idle_ms), idle_width);
            }
        }

        let msg_width = measure_message_width(ui, app, i);
        wrap_push_slot(&mut app.ui_state.wrap, max_rows, available_width, WrapSlotKind::Message(i), msg_width);
    }
    app.ui_state.wrap.rendered_count = total_matches;

    // 描画
    let total_height = max_rows as f32 * row_h;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(available_width, total_height),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let cursor_row = app.ui_state.wrap.cursor;

    let mut toggle_idx: Option<usize> = None;

    for row in 0..max_rows {
        let y_top = rect.min.y + row as f32 * row_h;
        let row_rect = Rect::from_min_size(
            egui::pos2(rect.min.x, y_top),
            Vec2::new(available_width, row_h),
        );

        if row == cursor_row {
            painter.rect_filled(row_rect, 0.0, theme::WRAP_CURSOR_LINE);
        } else {
            let bg = if row % 2 == 0 { theme::PROTOCOL_ROW_EVEN } else { theme::PROTOCOL_ROW_ODD };
            painter.rect_filled(row_rect, 0.0, bg);
        }

        if row < app.ui_state.wrap.slots.len() {
            if let Some(idx) = paint_wrap_slots(ui, &painter, app, &app.ui_state.wrap.slots[row], rect.min.x, &row_rect, row_h, "wrap_slot") {
                toggle_idx = Some(idx);
            }
        }

        // カーソル行に書き込み位置のキャレットを描画
        if row == cursor_row && app.ui_state.wrap.current_x > 0.0 {
            let caret_x = rect.min.x + app.ui_state.wrap.current_x;
            let caret_top = row_rect.min.y + 3.0;
            let caret_bottom = row_rect.max.y - 3.0;
            painter.line_segment(
                [egui::pos2(caret_x, caret_top), egui::pos2(caret_x, caret_bottom)],
                egui::Stroke::new(2.0, theme::WRAP_CURSOR_CARET),
            );
        }
    }

    handle_expand_toggle(app, toggle_idx);
    draw_expanded_windows(ui, app);
}

/// 停止中のラップ表示（キャッシュ済みレイアウトでスクロール表示）
fn draw_wrap_view_stopped(ui: &mut Ui, app: &mut GlassApp) {
    if app.loaded_protocol.is_none() {
        return;
    }

    let row_h = ROW_HEIGHT;
    let available_width = ui.available_width();

    // キャッシュの有効性チェック
    let filter_hash = compute_filter_hash(app);
    let total_matches = app.protocol_state.matches.len();
    let wrap = &app.ui_state.wrap;
    if wrap.stopped_match_count != total_matches
        || wrap.stopped_filter_hash != filter_hash
        || (wrap.stopped_width - available_width).abs() > 1.0
    {
        // レイアウトを再構築してキャッシュ
        build_stopped_layout(ui, app, available_width);
        app.ui_state.wrap.stopped_match_count = total_matches;
        app.ui_state.wrap.stopped_filter_hash = filter_hash;
        app.ui_state.wrap.stopped_width = available_width;
    }

    let lines = &app.ui_state.wrap.stopped_lines;

    if lines.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme::TEXT_MUTED, app.t.protocol_no_match);
        });
        return;
    }

    let total_rows = lines.len();
    let total_height = total_rows as f32 * row_h;
    let mut toggle_idx: Option<usize> = None;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(available_width, total_height),
                Sense::hover(),
            );

            let clip = ui.clip_rect();
            let visible_top = (clip.min.y - rect.min.y).max(0.0);
            let visible_bottom = (clip.max.y - rect.min.y).max(0.0);
            let first_row = (visible_top / row_h).floor() as usize;
            let last_row = ((visible_bottom / row_h).ceil() as usize).min(total_rows);

            let painter = ui.painter_at(rect);

            for row in first_row..last_row {
                let y_top = rect.min.y + row as f32 * row_h;
                let row_rect = Rect::from_min_size(
                    egui::pos2(rect.min.x, y_top),
                    Vec2::new(available_width, row_h),
                );
                let bg = if row % 2 == 0 { theme::PROTOCOL_ROW_EVEN } else { theme::PROTOCOL_ROW_ODD };
                painter.rect_filled(row_rect, 0.0, bg);

                if let Some(idx) = paint_wrap_slots(ui, &painter, app, &lines[row], rect.min.x, &row_rect, row_h, "wrap_stopped") {
                    toggle_idx = Some(idx);
                }
            }
        });

    handle_expand_toggle(app, toggle_idx);
    draw_expanded_windows(ui, app);
}

/// 停止時レイアウトをキャッシュに構築
fn build_stopped_layout(ui: &Ui, app: &mut GlassApp, available_width: f32) {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return,
    };

    let show_idle = app.ui_state.protocol_show_idle;
    let painter = ui.painter();
    let mut lines: Vec<Vec<WrapSlot>> = Vec::new();
    let mut current_line: Vec<WrapSlot> = Vec::new();
    let mut current_x: f32 = 0.0;

    let total_matches = app.protocol_state.matches.len();
    for i in 0..total_matches {
        let matched = &app.protocol_state.matches[i];
        if let Some(def_idx) = matched.message_def_idx {
            if app.ui_state.protocol_hidden_ids.contains(&proto.messages[def_idx].id) {
                continue;
            }
        }

        if show_idle {
            if let Some(idle_ms) = matched.preceding_idle_ms {
                let idle_width = measure_idle_width(painter, idle_ms);
                if current_x + idle_width > available_width && current_x > 0.0 {
                    lines.push(std::mem::take(&mut current_line));
                    current_x = 0.0;
                }
                current_line.push(WrapSlot { kind: WrapSlotKind::Idle(idle_ms), x: current_x, width: idle_width });
                current_x += idle_width;
            }
        }

        let msg_width = measure_message_width(ui, app, i);
        if current_x + msg_width > available_width && current_x > 0.0 {
            lines.push(std::mem::take(&mut current_line));
            current_x = 0.0;
        }
        current_line.push(WrapSlot { kind: WrapSlotKind::Message(i), x: current_x, width: msg_width });
        current_x += msg_width;
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    app.ui_state.wrap.stopped_lines = lines;
}

/// ラップ表示にスロットを追加（行送り・ラップアラウンド処理）
fn wrap_push_slot(wrap: &mut WrapViewState, max_rows: usize, available_width: f32, kind: WrapSlotKind, width: f32) {
    if wrap.current_x + width > available_width && wrap.current_x > 0.0 {
        wrap.cursor = (wrap.cursor + 1) % max_rows;
        wrap.slots[wrap.cursor].clear();
        wrap.current_x = 0.0;
    }

    let slot = WrapSlot {
        kind,
        x: wrap.current_x,
        width,
    };
    if wrap.current_x == 0.0 && !wrap.slots[wrap.cursor].is_empty() {
        wrap.slots[wrap.cursor].clear();
    }
    wrap.slots[wrap.cursor].push(slot);
    wrap.current_x += width;
}

/// 選択されたプロトコル定義を読み込む
fn load_selected_protocol(app: &mut GlassApp, idx: usize) {
    if let Some((path, _)) = app.protocol_files.get(idx) {
        match definition::load_protocol(path) {
            Ok(proto) => {
                let engine = ProtocolEngine::new(&proto);
                app.protocol_state.clear();
                app.protocol_state.sync_entries(app.buffer.entries(), &engine);
                app.protocol_state.flush(&engine);
                app.protocol_engine = Some(engine);
                app.loaded_protocol = Some(proto);
                app.ui_state.protocol_expanded.clear();
                app.ui_state.protocol_hidden_ids.clear();
                app.ui_state.wrap.reset();
            }
            Err(e) => {
                app.last_error = Some(e);
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
