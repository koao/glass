use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{Align, Align2, Rect, ScrollArea, Sense, Ui, Vec2};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState, ProtocolViewMode, WrapSlot, WrapSlotKind, WrapViewState};
use crate::protocol::definition;
use crate::protocol::engine::ProtocolEngine;
use crate::ui::selection;
use crate::ui::theme;

/// 行の高さ
const ROW_HEIGHT: f32 = 28.0;

/// フォントID
const FONT: fn() -> egui::FontId = || egui::FontId::proportional(15.0);
const MONO_FONT: fn() -> egui::FontId = || egui::FontId::monospace(13.0);

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
    painter: &egui::Painter,
    app: &GlassApp,
    slots: &[WrapSlot],
    rect_min_x: f32,
    row_rect: &Rect,
    row_h: f32,
) {
    let center_y = row_rect.center().y;

    for slot in slots {
        let slot_x = rect_min_x + slot.x;
        match &slot.kind {
            WrapSlotKind::Message(match_idx) => {
                if *match_idx < app.protocol_state.matches.len() {
                    paint_inline_message(painter, app, *match_idx, slot_x, center_y, slot.width, row_h);
                    // 選択ハイライト
                    if app.ui_state.protocol_selection.contains(*match_idx) {
                        let slot_rect = Rect::from_min_size(
                            egui::pos2(slot_x, row_rect.min.y),
                            Vec2::new(slot.width, row_h),
                        );
                        painter.rect_filled(slot_rect, 4.0, theme::SELECTION_BG);
                    }
                }
            }
            WrapSlotKind::Idle(idle_ms) => {
                paint_idle_text(painter, *idle_ms, slot_x, center_y);
            }
        }
    }
}

/// プロトコルパネル描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    // Running時は選択をクリア
    if app.state == MonitorState::Running {
        app.ui_state.protocol_selection.clear();
    }

    // 表示行リストを1回だけ構築
    let rows = build_row_entries(app);
    let msg_count = rows.iter().filter(|r| matches!(r, RowEntry::Message(..))).count();

    // ツールバー
    draw_toolbar(ui, app, msg_count);
    ui.separator();

    // 検索バー
    if app.ui_state.show_protocol_search_bar {
        draw_protocol_search_bar(ui, app);
        ui.separator();
    }

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
                let is_running = app.state == MonitorState::Running;
                // スクロールターゲットを行インデックスに変換
                let scroll_to_row = app.protocol_search.take_scroll_target().and_then(|match_idx| {
                    rows.iter().position(|r| matches!(r, RowEntry::Message(idx, _) if *idx == match_idx))
                });
                if !is_running {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() })
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

    // フローティングウィンドウ
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
            let selected_title = app.protocol_files.get(selected_idx)
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

        ui.separator();

        // フィルタボタン
        if ui.button(format!("{} {}", regular::FUNNEL, app.t.protocol_filter))
            .clicked()
        {
            app.ui_state.show_protocol_filter = !app.ui_state.show_protocol_filter;
        }

        // 表示モード切り替えボタン
        let (mode_icon, mode_short, mode_tooltip) = match app.ui_state.protocol_view_mode {
            ProtocolViewMode::List => (regular::REPEAT, app.t.protocol_mode_wrap_short, app.t.protocol_mode_wrap),
            ProtocolViewMode::Wrap => (regular::LIST_BULLETS, app.t.protocol_mode_list_short, app.t.protocol_mode_list),
        };
        if ui.button(format!("{} {}", mode_icon, mode_short))
            .on_hover_text(mode_tooltip)
            .clicked()
        {
            app.ui_state.protocol_view_mode = match app.ui_state.protocol_view_mode {
                ProtocolViewMode::List => ProtocolViewMode::Wrap,
                ProtocolViewMode::Wrap => ProtocolViewMode::List,
            };
            app.ui_state.wrap.reset();
        }

        // マッチ数表示（右���せ）
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
                egui::TextEdit::singleline(&mut app.protocol_search.query)
                    .desired_width(200.0),
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

            let is_stopped = app.state == MonitorState::Stopped;
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
                    ui.label(format!("{}/{}", app.protocol_search.current_index() + 1, count));
                } else {
                    ui.colored_label(theme::TEXT_MUTED, app.t.protocol_search_no_match);
                }
            }

            // 右寄せ: ヘルプボタン
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{} {}", regular::INFO, app.t.help)).clicked() {
                    app.ui_state.show_protocol_search_help = !app.ui_state.show_protocol_search_help;
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
fn draw_match_list(ui: &mut Ui, app: &mut GlassApp, rows: &[RowEntry], latest_only: bool, scroll_to_row: Option<usize>) {
    let proto = match &app.loaded_protocol {
        Some(p) => p,
        None => return,
    };
    let total_rows = rows.len();
    if total_rows == 0 {
        return;
    }

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

    let sense = if latest_only { Sense::hover() } else { Sense::click_and_drag() };
    let (rect, area_resp) = ui.allocate_exact_size(
        Vec2::new(available_width, total_height),
        sense,
    );

    let mut toggle_idx: Option<usize> = None;

    // 選択・ドラッグ処理（全エリア）— IDLE行は選択対象外
    if !latest_only {
        // メッセージ行のみヒット（IDLE行はNone）
        let hit_row_match = |pos: egui::Pos2| -> Option<usize> {
            if !rect.contains(pos) { return None; }
            let row_idx = ((pos.y - rect.min.y) / row_h).floor() as usize;
            if row_idx >= total_rows { return None; }
            match &rows[row_idx] {
                RowEntry::Message(idx, _) => Some(*idx),
                RowEntry::Idle(_) => None,
            }
        };

        // ダブルクリック: 詳細展開
        if area_resp.double_clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mi) = hit_row_match(pos) {
                    toggle_idx = Some(mi);
                }
            }
        }
        // クリック: 単体選択 / Shift+クリック: 範囲拡張
        else if area_resp.clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mi) = hit_row_match(pos) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    if shift {
                        app.ui_state.protocol_selection.extend(mi);
                    } else {
                        app.ui_state.protocol_selection.start(mi);
                    }
                }
            }
        }
        // ドラッグ開始: 選択開始
        if area_resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mi) = hit_row_match(pos) {
                    app.ui_state.protocol_selection.start(mi);
                }
            }
        }
        // ドラッグ中: 選択範囲を拡張
        if area_resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if let Some(mi) = hit_row_match(pos) {
                    app.ui_state.protocol_selection.extend(mi);
                }
            }
        }

        // 右クリックコンテキストメニュー（選択がある場合のみ）
        if app.ui_state.protocol_selection.range().is_some() {
            let copy_label = app.t.copy;
            let sel_range = app.ui_state.protocol_selection.range().unwrap();
            let matches_ref = &app.protocol_state.matches;
            area_resp.context_menu(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if ui.button(copy_label).clicked() {
                    let indices: Vec<usize> = (sel_range.0..=sel_range.1).collect();
                    let text = selection::format_protocol_copy(matches_ref, proto, &indices);
                    if !text.is_empty() {
                        ui.ctx().copy_text(text);
                    }
                    ui.close();
                }
            });
        }
    }

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
                // 選択ハイライト: 前後両方のメッセージが選択範囲内の場合のみ
                if let Some((sel_lo, sel_hi)) = app.ui_state.protocol_selection.range() {
                    let prev = rows[..row_idx].iter().rev().find_map(|r| match r {
                        RowEntry::Message(idx, _) => Some(*idx),
                        _ => None,
                    });
                    let next = rows[row_idx + 1..].iter().find_map(|r| match r {
                        RowEntry::Message(idx, _) => Some(*idx),
                        _ => None,
                    });
                    let between = match (prev, next) {
                        (Some(p), Some(n)) => p >= sel_lo && n <= sel_hi,
                        _ => false,
                    };
                    if between {
                        painter.rect_filled(row_rect, 0.0, theme::SELECTION_BG);
                    }
                }
                paint_idle_text(&painter, *idle_ms, row_rect.min.x + 8.0, center_y);
            }
            RowEntry::Message(match_idx, even) => {
                let matched = &app.protocol_state.matches[*match_idx];

                // 行背景色（検索ヒット時はハイライト）
                let bg = if app.protocol_search.is_current_hit(*match_idx) {
                    theme::PROTO_SEARCH_CURRENT_BG
                } else if app.protocol_search.is_hit(*match_idx) {
                    theme::PROTO_SEARCH_HIGHLIGHT_BG
                } else if *even {
                    theme::PROTOCOL_ROW_EVEN
                } else {
                    theme::PROTOCOL_ROW_ODD
                };
                painter.rect_filled(row_rect, 0.0, bg);

                // 検索結果へのスクロール
                if scroll_to_row == Some(row_idx) {
                    ui.scroll_to_rect(row_rect, Some(Align::Center));
                }

                let text_x = row_rect.min.x + 8.0;
                let mut cur_x = text_x;

                match matched.message_def_idx {
                    Some(def_idx) => {
                        let msg_def = &proto.messages[def_idx];

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

                // 選択ハイライト
                if app.ui_state.protocol_selection.contains(*match_idx) {
                    painter.rect_filled(row_rect, 0.0, theme::SELECTION_BG);
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

    // ピル背景（検索ヒット時は枠線を変更）
    let pill_margin = 2.0;
    let pill_rect = Rect::from_min_size(
        egui::pos2(x + pill_margin, center_y - row_h / 2.0 + pill_margin),
        Vec2::new(width - pill_margin * 2.0, row_h - pill_margin * 2.0),
    );
    let (stroke_width, stroke_color) = if app.protocol_search.is_current_hit(match_idx) {
        (2.0, theme::PROTO_SEARCH_CURRENT_BORDER)
    } else if app.protocol_search.is_hit(match_idx) {
        (2.0, theme::PROTO_SEARCH_HIGHLIGHT_BORDER)
    } else {
        (1.0, theme::WRAP_PILL_BORDER)
    };
    painter.rect(pill_rect, 4.0, theme::WRAP_PILL_BG, egui::Stroke::new(stroke_width, stroke_color), egui::StrokeKind::Inside);

    let matched = &app.protocol_state.matches[match_idx];
    let font = FONT();
    let mono_font = MONO_FONT();
    let mut cur_x = x + 8.0;

    match matched.message_def_idx {
        Some(def_idx) => {
            let msg_def = &proto.messages[def_idx];
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
    let is_paused = app.state == MonitorState::Paused;
    let total_height = max_rows as f32 * row_h;
    let sense = if is_paused { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, area_resp) = ui.allocate_exact_size(
        Vec2::new(available_width, total_height),
        sense,
    );

    let mut toggle_idx: Option<usize> = None;

    // Paused時: ドラッグ選択
    if is_paused {
        let slots_ref = &app.ui_state.wrap.slots;
        let hit_slot_match = |pos: egui::Pos2| -> Option<usize> {
            if !rect.contains(pos) { return None; }
            let row = ((pos.y - rect.min.y) / row_h).floor() as usize;
            if row >= slots_ref.len() { return None; }
            let local_x = pos.x - rect.min.x;
            for slot in &slots_ref[row] {
                if local_x >= slot.x && local_x <= slot.x + slot.width {
                    if let WrapSlotKind::Message(idx) = &slot.kind {
                        return Some(*idx);
                    }
                    return None; // IDLE上 → 選択しない
                }
            }
            None
        };

        if area_resp.double_clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mi) = hit_slot_match(pos) {
                    toggle_idx = Some(mi);
                }
            }
        } else if area_resp.clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mi) = hit_slot_match(pos) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    if shift {
                        app.ui_state.protocol_selection.extend(mi);
                    } else {
                        app.ui_state.protocol_selection.start(mi);
                    }
                }
            }
        }
        if area_resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mi) = hit_slot_match(pos) {
                    app.ui_state.protocol_selection.start(mi);
                }
            }
        }
        if area_resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if let Some(mi) = hit_slot_match(pos) {
                    app.ui_state.protocol_selection.extend(mi);
                }
            }
        }

        // 右クリックコンテキストメニュー
        if app.ui_state.protocol_selection.range().is_some() {
            let copy_label = app.t.copy;
            let sel_range = app.ui_state.protocol_selection.range().unwrap();
            let matches_ref = &app.protocol_state.matches;
            let proto_ref = app.loaded_protocol.as_ref();
            area_resp.context_menu(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if let Some(proto) = proto_ref {
                    if ui.button(copy_label).clicked() {
                        let indices: Vec<usize> = (sel_range.0..=sel_range.1).collect();
                        let text = selection::format_protocol_copy(matches_ref, proto, &indices);
                        if !text.is_empty() {
                            ui.ctx().copy_text(text);
                        }
                        ui.close();
                    }
                }
            });
        }
    }

    let painter = ui.painter_at(rect);
    let cursor_row = app.ui_state.wrap.cursor;

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
            let slots = app.ui_state.wrap.slots[row].clone();
            paint_wrap_slots(&painter, app, &slots, rect.min.x, &row_rect, row_h);
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

    let lines = app.ui_state.wrap.stopped_lines.clone();

    if lines.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme::TEXT_MUTED, app.t.protocol_no_match);
        });
        return;
    }

    let total_rows = lines.len();
    let total_height = total_rows as f32 * row_h;
    let mut toggle_idx: Option<usize> = None;

    // スクロールターゲットの行を特定
    let scroll_to_row = app.protocol_search.take_scroll_target().and_then(|match_idx| {
        lines.iter().position(|line| {
            line.iter().any(|slot| matches!(&slot.kind, WrapSlotKind::Message(idx) if *idx == match_idx))
        })
    });

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() })
        .show(ui, |ui| {
            let (rect, area_resp) = ui.allocate_exact_size(
                Vec2::new(available_width, total_height),
                Sense::click_and_drag(),
            );

            // ドラッグ選択処理 — メッセージスロット上のみヒット
            let hit_slot_match = |pos: egui::Pos2| -> Option<usize> {
                if !rect.contains(pos) { return None; }
                let row = ((pos.y - rect.min.y) / row_h).floor() as usize;
                if row >= total_rows { return None; }
                let local_x = pos.x - rect.min.x;
                for slot in &lines[row] {
                    if local_x >= slot.x && local_x <= slot.x + slot.width {
                        if let WrapSlotKind::Message(idx) = &slot.kind {
                            return Some(*idx);
                        }
                        return None; // IDLE上 → 選択しない
                    }
                }
                None
            };

            if area_resp.double_clicked() {
                if let Some(pos) = area_resp.interact_pointer_pos() {
                    if let Some(mi) = hit_slot_match(pos) {
                        toggle_idx = Some(mi);
                    }
                }
            } else if area_resp.clicked() {
                if let Some(pos) = area_resp.interact_pointer_pos() {
                    if let Some(mi) = hit_slot_match(pos) {
                        let shift = ui.input(|i| i.modifiers.shift);
                        if shift {
                            app.ui_state.protocol_selection.extend(mi);
                        } else {
                            app.ui_state.protocol_selection.start(mi);
                            }
                    }
                }
            }
            if area_resp.drag_started_by(egui::PointerButton::Primary) {
                if let Some(pos) = area_resp.interact_pointer_pos() {
                    if let Some(mi) = hit_slot_match(pos) {
                        app.ui_state.protocol_selection.start(mi);
                    }
                }
            }
            if area_resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    if let Some(mi) = hit_slot_match(pos) {
                        app.ui_state.protocol_selection.extend(mi);
                    }
                }
            }

            // 右クリックコンテキストメニュー
            if app.ui_state.protocol_selection.range().is_some() {
                let copy_label = app.t.copy;
                let copy_text = app.loaded_protocol.as_ref().map(|proto| {
                    let (lo, hi) = app.ui_state.protocol_selection.range().unwrap();
                    let indices: Vec<usize> = (lo..=hi).collect();
                    selection::format_protocol_copy(&app.protocol_state.matches, proto, &indices)
                });
                area_resp.context_menu(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    if let Some(text) = &copy_text {
                        if !text.is_empty() && ui.button(copy_label).clicked() {
                            ui.ctx().copy_text(text.clone());
                            ui.close();
                        }
                    }
                });
            }

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

                if scroll_to_row == Some(row) {
                    ui.scroll_to_rect(row_rect, Some(Align::Center));
                }

                paint_wrap_slots(&painter, app, &lines[row], rect.min.x, &row_rect, row_h);
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
