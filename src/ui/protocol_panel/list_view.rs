use egui::{Align, Rect, Sense, Ui, Vec2};
use egui_phosphor::regular;

use crate::app::GlassApp;
use crate::ui::menu::{self, MenuItem};
use crate::ui::selection;
use crate::ui::theme;

use super::{
    FONT, MONO_FONT, ROW_HEIGHT, RowEntry, draw_expanded_windows, extract_ascii,
    handle_expand_toggle, paint_idle_text,
};

/// マッチ結果一覧描画（仮想スクロール）
pub(super) fn draw_match_list(
    ui: &mut Ui,
    app: &mut GlassApp,
    rows: &[RowEntry],
    latest_only: bool,
    scroll_to_row: Option<usize>,
) {
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

    let sense = if latest_only {
        Sense::hover()
    } else {
        Sense::click_and_drag()
    };
    let (rect, area_resp) = ui.allocate_exact_size(Vec2::new(available_width, total_height), sense);

    let mut toggle_id: Option<u64> = None;

    if !latest_only {
        let matches_ref0 = &app.protocol_state.matches;
        let hit_row_match = |pos: egui::Pos2| -> Option<u64> {
            if !rect.contains(pos) {
                return None;
            }
            let row_idx = ((pos.y - rect.min.y) / row_h).floor() as usize;
            if row_idx >= total_rows {
                return None;
            }
            match &rows[row_idx] {
                RowEntry::Message(idx, _) => matches_ref0.get(*idx).map(|m| m.id),
                RowEntry::Idle(_) => None,
            }
        };

        if area_resp.double_clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mid) = hit_row_match(pos) {
                    toggle_id = Some(mid);
                }
            }
        } else if area_resp.clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mid) = hit_row_match(pos) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    if shift {
                        app.ui_state.protocol_selection.extend(mid);
                    } else {
                        app.ui_state.protocol_selection.start(mid);
                    }
                }
            }
        }
        if area_resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = area_resp.interact_pointer_pos() {
                if let Some(mid) = hit_row_match(pos) {
                    app.ui_state.protocol_selection.start(mid);
                }
            }
        }
        if area_resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if let Some(mid) = hit_row_match(pos) {
                    app.ui_state.protocol_selection.extend(mid);
                }
            }
        }

        if app.ui_state.protocol_selection.range().is_some() {
            let copy_label = app.t.copy;
            let seq_label = app.t.sequence_diagram;
            let has_seq = proto.protocol.sequence.is_some();
            let (lo_id, hi_id) = app.ui_state.protocol_selection.range().unwrap();
            let lo = app.protocol_state.position_by_id(lo_id).unwrap_or(0);
            let hi = app
                .protocol_state
                .position_by_id(hi_id)
                .unwrap_or_else(|| app.protocol_state.matches.len().saturating_sub(1));
            let matches_ref = &app.protocol_state.matches;
            area_resp.context_menu(|ui| {
                let items = [
                    MenuItem::new(copy_label),
                    MenuItem::new(seq_label).enabled(has_seq),
                ];
                if let Some(idx) = menu::show(ui, &items) {
                    match idx {
                        0 => {
                            if lo <= hi && !matches_ref.is_empty() {
                                let indices: Vec<usize> = (lo..=hi).collect();
                                let text =
                                    selection::format_protocol_copy(matches_ref, proto, &indices);
                                if !text.is_empty() {
                                    ui.ctx().copy_text(text);
                                }
                            }
                        }
                        1 => {
                            app.ui_state.sequence_diagram.generate_requested = true;
                        }
                        _ => {}
                    }
                    ui.close();
                }
            });
        }
    }

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
                if let Some((sel_lo, sel_hi)) = app.ui_state.protocol_selection.range() {
                    let prev = rows[..row_idx].iter().rev().find_map(|r| match r {
                        RowEntry::Message(idx, _) => Some(app.protocol_state.matches[*idx].id),
                        _ => None,
                    });
                    let next = rows[row_idx + 1..].iter().find_map(|r| match r {
                        RowEntry::Message(idx, _) => Some(app.protocol_state.matches[*idx].id),
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
                let match_id = matched.id;

                let bg = if app.protocol_search.is_current_hit(match_id) {
                    theme::PROTO_SEARCH_CURRENT_BG
                } else if app.protocol_search.is_hit(match_id) {
                    theme::PROTO_SEARCH_HIGHLIGHT_BG
                } else if *even {
                    theme::PROTOCOL_ROW_EVEN
                } else {
                    theme::PROTOCOL_ROW_ODD
                };
                painter.rect_filled(row_rect, 0.0, bg);

                if scroll_to_row == Some(row_idx) {
                    ui.scroll_to_rect(row_rect, Some(Align::Center));
                }

                let text_x = row_rect.min.x + 8.0;
                let mut cur_x = text_x;

                match matched.message_def_idx {
                    Some(def_idx) => {
                        let msg_def = &proto.messages[def_idx];

                        let title = &msg_def.title;
                        let title_color = msg_def.parsed_color.unwrap_or(egui::Color32::WHITE);
                        let g =
                            painter.layout_no_wrap(title.to_string(), font.clone(), title_color);
                        let w = g.rect.width();
                        painter.galley(
                            egui::pos2(cur_x, center_y - g.rect.height() / 2.0),
                            g,
                            title_color,
                        );
                        cur_x += w + 8.0;

                        for field in msg_def.fields.iter().filter(|f| f.inline) {
                            let name = &field.name;
                            let ascii =
                                extract_ascii(&matched.frame.bytes, field.offset, field.size);
                            let text = format!("{}:{}", name, ascii);
                            let g =
                                painter.layout_no_wrap(text, mono_font.clone(), theme::TEXT_MUTED);
                            let w = g.rect.width();
                            painter.galley(
                                egui::pos2(cur_x, center_y - g.rect.height() / 2.0),
                                g,
                                theme::TEXT_MUTED,
                            );
                            cur_x += w + 8.0;
                        }
                    }
                    None => {
                        let text = format!("{} {}", regular::QUESTION, app.t.protocol_unmatched);
                        let g =
                            painter.layout_no_wrap(text, font.clone(), theme::PROTOCOL_UNMATCHED);
                        painter.galley(
                            egui::pos2(cur_x, center_y - g.rect.height() / 2.0),
                            g,
                            theme::PROTOCOL_UNMATCHED,
                        );
                    }
                }

                let size_text = format!("{}B", matched.frame.bytes.len());
                let g = painter.layout_no_wrap(size_text, mono_font.clone(), theme::TEXT_MUTED);
                let right_x = row_rect.max.x - g.rect.width() - 8.0;
                painter.galley(
                    egui::pos2(right_x, center_y - g.rect.height() / 2.0),
                    g,
                    theme::TEXT_MUTED,
                );

                if app.ui_state.protocol_selection.contains(match_id) {
                    painter.rect_filled(row_rect, 0.0, theme::SELECTION_BG);
                }
            }
        }
    }

    handle_expand_toggle(app, toggle_id);
    draw_expanded_windows(ui, app);
}
