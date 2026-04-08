use egui::{Rect, ScrollArea, Sense, Ui, Vec2};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState, WrapSlot, WrapSlotKind, WrapViewState};
use crate::ui::menu::{self, MenuItem};
use crate::ui::selection;
use crate::ui::theme;

use super::{
    FONT, MONO_FONT, ROW_HEIGHT, compute_filter_hash, draw_expanded_windows, extract_ascii,
    handle_expand_toggle, paint_idle_text,
};

/// IDLE上クリック時に同じ行の最寄りメッセージ ID を返す
fn nearest_message_in_row(slots: &[WrapSlot], local_x: f32) -> Option<u64> {
    let mut best: Option<(f32, u64)> = None;
    for s in slots {
        if let WrapSlotKind::Message { id, .. } = &s.kind {
            let center = s.x + s.width / 2.0;
            let dist = (center - local_x).abs();
            if best.is_none() || dist < best.unwrap().0 {
                best = Some((dist, *id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// IDLE スロットが選択範囲内にあるか判定（前後のメッセージが両方選択内）
fn is_idle_selected(app: &GlassApp, slots: &[WrapSlot], slot_index: usize) -> bool {
    let (sel_lo, sel_hi) = match app.ui_state.protocol_selection.range() {
        Some(r) => r,
        None => return false,
    };
    let prev = slots[..slot_index].iter().rev().find_map(|s| {
        if let WrapSlotKind::Message { id, .. } = &s.kind {
            Some(*id)
        } else {
            None
        }
    });
    let next = slots[slot_index + 1..].iter().find_map(|s| {
        if let WrapSlotKind::Message { id, .. } = &s.kind {
            Some(*id)
        } else {
            None
        }
    });
    match (prev, next) {
        (Some(p), Some(n)) => p >= sel_lo && p <= sel_hi && n >= sel_lo && n <= sel_hi,
        (Some(p), None) => p >= sel_lo && p <= sel_hi,
        (None, Some(n)) => n >= sel_lo && n <= sel_hi,
        (None, None) => false,
    }
}

/// スロット行の描画（メッセージ・IDLE描画＋選択ハイライト）
fn paint_wrap_slots(
    painter: &egui::Painter,
    app: &GlassApp,
    slots: &[WrapSlot],
    rect_min_x: f32,
    row_rect: &Rect,
    row_h: f32,
) {
    let center_y = row_rect.center().y;

    for (i, slot) in slots.iter().enumerate() {
        let slot_x = rect_min_x + slot.x;
        match &slot.kind {
            WrapSlotKind::Message { idx, id } => {
                if *idx < app.protocol_state.matches.len()
                    && app.protocol_state.matches[*idx].id == *id
                {
                    paint_inline_message(painter, app, *idx, slot_x, center_y, slot.width, row_h);
                    if app.ui_state.protocol_selection.contains(*id) {
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
                if is_idle_selected(app, slots, i) {
                    let slot_rect = Rect::from_min_size(
                        egui::pos2(slot_x, row_rect.min.y),
                        Vec2::new(slot.width, row_h),
                    );
                    painter.rect_filled(slot_rect, 4.0, theme::SELECTION_BG);
                }
            }
        }
    }
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
    let mut w = 8.0;

    match matched.message_def_idx {
        Some(def_idx) => {
            let msg_def = &proto.messages[def_idx];
            w += painter
                .layout_no_wrap(msg_def.title.clone(), font.clone(), egui::Color32::WHITE)
                .rect
                .width()
                + 8.0;
            for field in msg_def.fields.iter().filter(|f| f.inline) {
                let ascii = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                let text = format!("{}:{}", field.name, ascii);
                w += painter
                    .layout_no_wrap(text, mono_font.clone(), egui::Color32::WHITE)
                    .rect
                    .width()
                    + 8.0;
            }
        }
        None => {
            let text = format!("{} {}", regular::QUESTION, app.t.protocol_unmatched);
            w += painter
                .layout_no_wrap(text, font.clone(), egui::Color32::WHITE)
                .rect
                .width()
                + 8.0;
        }
    }
    w + 8.0
}

/// IDLE テキストの表示幅を計測
fn measure_idle_width(painter: &egui::Painter, idle_ms: f64) -> f32 {
    let text = format!("IDLE {}ms", idle_ms as u64);
    painter
        .layout_no_wrap(text, MONO_FONT(), egui::Color32::WHITE)
        .rect
        .width()
        + 16.0
}

/// インラインメッセージを描画（ピル背景付き）
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
    let match_id = app.protocol_state.matches[match_idx].id;

    let pill_margin = 2.0;
    let pill_rect = Rect::from_min_size(
        egui::pos2(x + pill_margin, center_y - row_h / 2.0 + pill_margin),
        Vec2::new(width - pill_margin * 2.0, row_h - pill_margin * 2.0),
    );
    let (stroke_width, stroke_color) = if app.protocol_search.is_current_hit(match_id) {
        (2.0, theme::PROTO_SEARCH_CURRENT_BORDER)
    } else if app.protocol_search.is_hit(match_id) {
        (2.0, theme::PROTO_SEARCH_HIGHLIGHT_BORDER)
    } else {
        (1.0, theme::WRAP_PILL_BORDER)
    };
    painter.rect(
        pill_rect,
        4.0,
        theme::WRAP_PILL_BG,
        egui::Stroke::new(stroke_width, stroke_color),
        egui::StrokeKind::Inside,
    );

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
            painter.galley(
                egui::pos2(cur_x, center_y - g.rect.height() / 2.0),
                g,
                title_color,
            );
            cur_x += w + 8.0;
            cur_x += super::paint_checksum_ng_badge(
                painter,
                font.clone(),
                cur_x,
                center_y,
                matched.checksum,
            );
            for field in msg_def.fields.iter().filter(|f| f.inline) {
                let ascii = extract_ascii(&matched.frame.bytes, field.offset, field.size);
                let text = format!("{}:{}", field.name, ascii);
                let g = painter.layout_no_wrap(text, mono_font.clone(), theme::TEXT_MUTED);
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
            let g = painter.layout_no_wrap(text, font.clone(), theme::PROTOCOL_UNMATCHED);
            painter.galley(
                egui::pos2(cur_x, center_y - g.rect.height() / 2.0),
                g,
                theme::PROTOCOL_UNMATCHED,
            );
        }
    }
}

/// ラップ表示にスロットを追加（行送り・ラップアラウンド処理）
fn wrap_push_slot(
    wrap: &mut WrapViewState,
    max_rows: usize,
    available_width: f32,
    kind: WrapSlotKind,
    width: f32,
) {
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

/// ラップ表示描画（live / stopped 切替）
pub(super) fn draw_wrap_view(ui: &mut Ui, app: &mut GlassApp) {
    if app.loaded_protocol.is_none() {
        return;
    }

    if app.state == MonitorState::Stopped {
        draw_wrap_view_stopped(ui, app);
        return;
    }

    let row_h = ROW_HEIGHT;
    let available_width = ui.available_width();
    let available_height = ui.available_height();
    let max_rows = (available_height / row_h).floor().max(1.0) as usize;

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

    if app.ui_state.wrap.slots.len() != max_rows {
        app.ui_state.wrap.slots.resize(max_rows, Vec::new());
    }

    let total_matches = app.protocol_state.matches.len();
    if total_matches < app.ui_state.wrap.rendered_count {
        app.ui_state.wrap.reset();
        app.ui_state.wrap.max_rows = max_rows;
        app.ui_state.wrap.available_width = available_width;
        app.ui_state.wrap.filter_hash = filter_hash;
        app.ui_state.wrap.slots.resize(max_rows, Vec::new());
    }

    let proto = app.loaded_protocol.as_ref().unwrap();
    let show_idle = app.ui_state.protocol_show_idle;
    let start = app.ui_state.wrap.rendered_count;
    for i in start..total_matches {
        let matched = &app.protocol_state.matches[i];
        if let Some(def_idx) = matched.message_def_idx
            && app
                .ui_state
                .protocol_hidden_ids
                .contains(&proto.messages[def_idx].id)
        {
            continue;
        }

        if show_idle && let Some(idle_ms) = matched.preceding_idle_ms {
            let idle_width = measure_idle_width(ui.painter(), idle_ms);
            wrap_push_slot(
                &mut app.ui_state.wrap,
                max_rows,
                available_width,
                WrapSlotKind::Idle(idle_ms),
                idle_width,
            );
        }

        let msg_width = measure_message_width(ui, app, i);
        let mid = app.protocol_state.matches[i].id;
        wrap_push_slot(
            &mut app.ui_state.wrap,
            max_rows,
            available_width,
            WrapSlotKind::Message { idx: i, id: mid },
            msg_width,
        );
    }
    app.ui_state.wrap.rendered_count = total_matches;

    let is_paused = app.state == MonitorState::Paused;
    let total_height = max_rows as f32 * row_h;
    let sense = if is_paused {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, area_resp) = ui.allocate_exact_size(Vec2::new(available_width, total_height), sense);

    let mut toggle_id: Option<u64> = None;

    if is_paused {
        let slots_ref = &app.ui_state.wrap.slots;
        let hit_slot_match = |pos: egui::Pos2| -> Option<u64> {
            if !rect.contains(pos) {
                return None;
            }
            let row = ((pos.y - rect.min.y) / row_h).floor() as usize;
            if row >= slots_ref.len() {
                return None;
            }
            let local_x = pos.x - rect.min.x;
            for slot in &slots_ref[row] {
                if local_x >= slot.x && local_x <= slot.x + slot.width {
                    if let WrapSlotKind::Message { id, .. } = &slot.kind {
                        return Some(*id);
                    }
                    return nearest_message_in_row(&slots_ref[row], local_x);
                }
            }
            None
        };

        if area_resp.double_clicked() {
            if let Some(pos) = area_resp.interact_pointer_pos()
                && let Some(mid) = hit_slot_match(pos)
            {
                toggle_id = Some(mid);
            }
        } else if area_resp.clicked()
            && let Some(pos) = area_resp.interact_pointer_pos()
            && let Some(mid) = hit_slot_match(pos)
        {
            let shift = ui.input(|i| i.modifiers.shift);
            if shift {
                app.ui_state.protocol_selection.extend(mid);
            } else {
                app.ui_state.protocol_selection.start(mid);
            }
        }
        if area_resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = area_resp.interact_pointer_pos()
            && let Some(mid) = hit_slot_match(pos)
        {
            app.ui_state.protocol_selection.start(mid);
        }
        if area_resp.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            && let Some(mid) = hit_slot_match(pos)
        {
            app.ui_state.protocol_selection.extend(mid);
        }

        if app.ui_state.protocol_selection.range().is_some() {
            let copy_label = app.t.copy;
            let seq_label = app.t.sequence_diagram;
            let (lo_id, hi_id) = app.ui_state.protocol_selection.range().unwrap();
            let lo = app.protocol_state.position_by_id(lo_id).unwrap_or(0);
            let hi = app
                .protocol_state
                .position_by_id(hi_id)
                .unwrap_or_else(|| app.protocol_state.matches.len().saturating_sub(1));
            let matches_ref = &app.protocol_state.matches;
            let proto_ref = app.loaded_protocol.as_ref();
            let has_seq = proto_ref
                .map(|p| p.protocol.sequence.is_some())
                .unwrap_or(false);
            area_resp.context_menu(|ui| {
                let items = [
                    MenuItem::new(copy_label).enabled(proto_ref.is_some()),
                    MenuItem::new(seq_label).enabled(has_seq),
                ];
                if let Some(idx) = menu::show(ui, &items) {
                    match idx {
                        0 => {
                            if let Some(proto) = proto_ref
                                && lo <= hi
                                && !matches_ref.is_empty()
                            {
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
            let bg = if row % 2 == 0 {
                theme::PROTOCOL_ROW_EVEN
            } else {
                theme::PROTOCOL_ROW_ODD
            };
            painter.rect_filled(row_rect, 0.0, bg);
        }

        if row < app.ui_state.wrap.slots.len() {
            let slots = app.ui_state.wrap.slots[row].clone();
            paint_wrap_slots(&painter, app, &slots, rect.min.x, &row_rect, row_h);
        }

        if row == cursor_row && app.ui_state.wrap.current_x > 0.0 {
            let caret_x = rect.min.x + app.ui_state.wrap.current_x;
            let caret_top = row_rect.min.y + 3.0;
            let caret_bottom = row_rect.max.y - 3.0;
            painter.line_segment(
                [
                    egui::pos2(caret_x, caret_top),
                    egui::pos2(caret_x, caret_bottom),
                ],
                egui::Stroke::new(2.0, theme::WRAP_CURSOR_CARET),
            );
        }
    }

    handle_expand_toggle(app, toggle_id);
    draw_expanded_windows(ui, app);
}

/// 停止中のラップ表示（キャッシュ済みレイアウトでスクロール表示）
fn draw_wrap_view_stopped(ui: &mut Ui, app: &mut GlassApp) {
    if app.loaded_protocol.is_none() {
        return;
    }

    let row_h = ROW_HEIGHT;
    let available_width = ui.available_width();

    let filter_hash = compute_filter_hash(app);
    let total_matches = app.protocol_state.matches.len();
    let wrap = &app.ui_state.wrap;
    if wrap.stopped_match_count != total_matches
        || wrap.stopped_filter_hash != filter_hash
        || (wrap.stopped_width - available_width).abs() > 1.0
    {
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
    let mut toggle_id: Option<u64> = None;

    let scroll_to_row = app.protocol_search.take_scroll_target().and_then(|target_id| {
        lines.iter().position(|line| {
            line.iter().any(|slot| matches!(&slot.kind, WrapSlotKind::Message { id, .. } if *id == target_id))
        })
    });

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_source(egui::scroll_area::ScrollSource {
            drag: false,
            ..Default::default()
        })
        .show(ui, |ui| {
            let (rect, area_resp) = ui.allocate_exact_size(
                Vec2::new(available_width, total_height),
                Sense::click_and_drag(),
            );

            let hit_slot_match = |pos: egui::Pos2| -> Option<u64> {
                if !rect.contains(pos) {
                    return None;
                }
                let row = ((pos.y - rect.min.y) / row_h).floor() as usize;
                if row >= total_rows {
                    return None;
                }
                let local_x = pos.x - rect.min.x;
                for slot in &lines[row] {
                    if local_x >= slot.x && local_x <= slot.x + slot.width {
                        if let WrapSlotKind::Message { id, .. } = &slot.kind {
                            return Some(*id);
                        }
                        return nearest_message_in_row(&lines[row], local_x);
                    }
                }
                None
            };

            if area_resp.double_clicked() {
                if let Some(pos) = area_resp.interact_pointer_pos()
                    && let Some(mid) = hit_slot_match(pos)
                {
                    toggle_id = Some(mid);
                }
            } else if area_resp.clicked()
                && let Some(pos) = area_resp.interact_pointer_pos()
                && let Some(mid) = hit_slot_match(pos)
            {
                let shift = ui.input(|i| i.modifiers.shift);
                if shift {
                    app.ui_state.protocol_selection.extend(mid);
                } else {
                    app.ui_state.protocol_selection.start(mid);
                }
            }
            if area_resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(pos) = area_resp.interact_pointer_pos()
                && let Some(mid) = hit_slot_match(pos)
            {
                app.ui_state.protocol_selection.start(mid);
            }
            if area_resp.dragged_by(egui::PointerButton::Primary)
                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                && let Some(mid) = hit_slot_match(pos)
            {
                app.ui_state.protocol_selection.extend(mid);
            }

            if app.ui_state.protocol_selection.range().is_some() {
                let copy_label = app.t.copy;
                let seq_label = app.t.sequence_diagram;
                let has_seq = app
                    .loaded_protocol
                    .as_ref()
                    .map(|p| p.protocol.sequence.is_some())
                    .unwrap_or(false);
                let copy_text = app.loaded_protocol.as_ref().map(|proto| {
                    let (lo_id, hi_id) = app.ui_state.protocol_selection.range().unwrap();
                    let lo = app.protocol_state.position_by_id(lo_id).unwrap_or(0);
                    let hi = app
                        .protocol_state
                        .position_by_id(hi_id)
                        .unwrap_or_else(|| app.protocol_state.matches.len().saturating_sub(1));
                    if lo > hi || app.protocol_state.matches.is_empty() {
                        String::new()
                    } else {
                        let indices: Vec<usize> = (lo..=hi).collect();
                        selection::format_protocol_copy(
                            &app.protocol_state.matches,
                            proto,
                            &indices,
                        )
                    }
                });
                area_resp.context_menu(|ui| {
                    let copy_enabled = copy_text.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
                    let items = [
                        MenuItem::new(copy_label).enabled(copy_enabled),
                        MenuItem::new(seq_label).enabled(has_seq),
                    ];
                    if let Some(idx) = menu::show(ui, &items) {
                        match idx {
                            0 => {
                                if let Some(text) = &copy_text {
                                    ui.ctx().copy_text(text.clone());
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

            if let Some(row) = scroll_to_row {
                super::scroll_to_virtual_row(ui, rect, row, row_h, available_width);
            }

            let clip = ui.clip_rect();
            let visible_top = (clip.min.y - rect.min.y).max(0.0);
            let visible_bottom = (clip.max.y - rect.min.y).max(0.0);
            let first_row = (visible_top / row_h).floor() as usize;
            let last_row = ((visible_bottom / row_h).ceil() as usize).min(total_rows);

            let painter = ui.painter_at(rect);

            for (row, line) in lines.iter().enumerate().take(last_row).skip(first_row) {
                let y_top = rect.min.y + row as f32 * row_h;
                let row_rect = Rect::from_min_size(
                    egui::pos2(rect.min.x, y_top),
                    Vec2::new(available_width, row_h),
                );
                let bg = if row % 2 == 0 {
                    theme::PROTOCOL_ROW_EVEN
                } else {
                    theme::PROTOCOL_ROW_ODD
                };
                painter.rect_filled(row_rect, 0.0, bg);

                paint_wrap_slots(&painter, app, line, rect.min.x, &row_rect, row_h);
            }
        });

    handle_expand_toggle(app, toggle_id);
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
        if let Some(def_idx) = matched.message_def_idx
            && app
                .ui_state
                .protocol_hidden_ids
                .contains(&proto.messages[def_idx].id)
        {
            continue;
        }

        if show_idle && let Some(idle_ms) = matched.preceding_idle_ms {
            let idle_width = measure_idle_width(painter, idle_ms);
            if current_x + idle_width > available_width && current_x > 0.0 {
                lines.push(std::mem::take(&mut current_line));
                current_x = 0.0;
            }
            current_line.push(WrapSlot {
                kind: WrapSlotKind::Idle(idle_ms),
                x: current_x,
                width: idle_width,
            });
            current_x += idle_width;
        }

        let msg_width = measure_message_width(ui, app, i);
        if current_x + msg_width > available_width && current_x > 0.0 {
            lines.push(std::mem::take(&mut current_line));
            current_x = 0.0;
        }
        let mid = app.protocol_state.matches[i].id;
        current_line.push(WrapSlot {
            kind: WrapSlotKind::Message { idx: i, id: mid },
            x: current_x,
            width: msg_width,
        });
        current_x += msg_width;
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    app.ui_state.wrap.stopped_lines = lines;
}
