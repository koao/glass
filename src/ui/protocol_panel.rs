use egui::{Align, Rect, ScrollArea, Sense, Ui, Vec2};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState};
use crate::protocol::definition;
use crate::protocol::engine::ProtocolEngine;
use crate::ui::theme;

/// 行の高さ
const ROW_HEIGHT: f32 = 28.0;

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

    if rows.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.colored_label(theme::TEXT_MUTED, app.t.protocol_no_match);
        });
        return;
    }

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
    let font = egui::FontId::proportional(15.0);
    let mono_font = egui::FontId::monospace(13.0);

    for row_idx in draw_f..draw_l {
        let y_offset = (row_idx - row_offset) as f32 * row_h;
        let row_rect = Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + y_offset),
            Vec2::new(available_width, ROW_HEIGHT),
        );
        let center_y = row_rect.center().y;

        match &rows[row_idx] {
            RowEntry::Idle(idle_ms) => {
                // IDLE行: 背景なし、中央にテキスト
                let text = format!("IDLE {:.1}ms", idle_ms);
                let g = painter.layout_no_wrap(text, mono_font.clone(), theme::PROTOCOL_IDLE);
                painter.galley(
                    egui::pos2(row_rect.min.x + 16.0, center_y - g.rect.height() / 2.0),
                    g,
                    theme::PROTOCOL_IDLE,
                );
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
                            let (icon, color, label) = match dir.as_str() {
                                "send" => (regular::ARROW_RIGHT, theme::PROTOCOL_SEND, app.t.protocol_send),
                                "receive" => (regular::ARROW_LEFT, theme::PROTOCOL_RECV, app.t.protocol_recv),
                                _ => (regular::MINUS, theme::TEXT_MUTED, ""),
                            };
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

    // 展開状態の更新
    if let Some(idx) = toggle_idx {
        if app.ui_state.protocol_expanded.contains(&idx) {
            app.ui_state.protocol_expanded.remove(&idx);
        } else {
            app.ui_state.protocol_expanded.insert(idx);
        }
    }

    // 展開中の詳細をフローティングウィンドウで表示
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
