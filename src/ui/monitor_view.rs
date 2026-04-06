use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui::epaint::TextShape;

use crate::app::{DisplayMode, GlassApp, MonitorState};
use crate::model::grid::DisplayCell;
use crate::settings::MonitorColors;
use crate::ui::search::SearchState;
use crate::ui::selection;
use crate::ui::theme;

// === フォントサイズ ===
const MAIN_FONT_SIZE: f32 = 20.0;
const ROTATED_FONT_SIZE: f32 = 11.0;

// === 制御コード略称 ===
const CONTROL_CODES: [&str; 33] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL",
    "BS",  "HT",  "LF",  "VT",  "FF",  "CR",  "SO",  "SI",
    "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB",
    "CAN", "EM",  "SUB", "ESC", "FS",  "GS",  "RS",  "US",
    "SP",  // 0x20 スペース
];

/// セル寸法を計算
fn calc_layout(ui: &Ui) -> (f32, f32, usize) {
    let char_w = MAIN_FONT_SIZE * 0.6;
    let cell_w = char_w + 6.0;
    let cell_h = MAIN_FONT_SIZE * 1.8;
    let cols = (ui.available_width() / cell_w).floor().max(1.0) as usize;
    (cell_w, cell_h, cols)
}

/// セルのRectを取得
fn cell_rect(grid_rect: Rect, idx: usize, cols: usize, cell_w: f32, cell_h: f32) -> Rect {
    let col = idx % cols;
    let row = idx / cols;
    Rect::from_min_size(
        Pos2::new(
            grid_rect.min.x + col as f32 * cell_w,
            grid_rect.min.y + row as f32 * cell_h,
        ),
        Vec2::new(cell_w, cell_h),
    )
}

/// 行区切り線を描画
fn draw_row_lines(
    painter: &egui::Painter,
    rect: Rect,
    cols: usize,
    rows: usize,
    cell_w: f32,
    cell_h: f32,
) {
    let width = cols as f32 * cell_w;
    for row in 1..rows {
        let y = rect.min.y + row as f32 * cell_h;
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.min.x + width, y)],
            Stroke::new(1.0, theme::GRID_LINE),
        );
    }
}

/// リングバッファのセルインデックス→表示バッファインデックス変換
fn map_cell_to_entry(cell_idx: usize, buf_len: usize, total_cells: usize) -> Option<usize> {
    if buf_len == 0 || total_cells == 0 {
        return None;
    }
    if buf_len <= total_cells {
        if cell_idx < buf_len {
            Some(cell_idx)
        } else {
            None
        }
    } else {
        let write_pos = buf_len % total_cells;
        if cell_idx < write_pos {
            Some(buf_len - write_pos + cell_idx)
        } else {
            Some(buf_len - total_cells + (cell_idx - write_pos))
        }
    }
}

/// 検索ハイライトの背景色を取得（点滅アニメーション付き）
fn search_highlight_bg(search: &SearchState, entry_idx: usize, time: f64) -> Option<Color32> {
    if search.is_current_highlight(entry_idx) {
        Some(theme::SEARCH_CURRENT_BG)
    } else if search.is_highlighted(entry_idx) {
        // sin波で点滅（0.3〜1.0の範囲でアルファ変動、周期2秒）
        let alpha = 0.65 + 0.35 * (time * std::f64::consts::PI).sin();
        let bg = theme::SEARCH_HIGHLIGHT_BG;
        Some(Color32::from_rgba_premultiplied(
            (bg.r() as f64 * alpha) as u8,
            (bg.g() as f64 * alpha) as u8,
            (bg.b() as f64 * alpha) as u8,
            (255.0 * alpha * 0.7) as u8,
        ))
    } else {
        None
    }
}

/// マウス位置からセルインデックスを取得
fn hit_test_cell(pos: Pos2, rect: Rect, cols: usize, cell_w: f32, cell_h: f32, max_idx: usize) -> Option<usize> {
    if !rect.contains(pos) {
        return None;
    }
    let col = ((pos.x - rect.min.x) / cell_w) as usize;
    let row = ((pos.y - rect.min.y) / cell_h) as usize;
    let idx = row * cols + col;
    if col < cols && idx < max_idx {
        Some(idx)
    } else {
        None
    }
}

/// 選択のマウスイベント処理
fn handle_selection_input(
    ui: &Ui,
    response: &egui::Response,
    app: &mut GlassApp,
    rect: Rect,
    cols: usize,
    cell_w: f32,
    cell_h: f32,
    max_idx: usize,
    cell_to_disp: impl Fn(usize) -> Option<usize>,
) {
    let shift = ui.input(|i| i.modifiers.shift);

    // クリック: 選択開始/クリア
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(cell_idx) = hit_test_cell(pos, rect, cols, cell_w, cell_h, max_idx) {
                if let Some(disp_idx) = cell_to_disp(cell_idx) {
                    if shift {
                        app.ui_state.monitor_selection.extend(disp_idx);
                    } else {
                        app.ui_state.monitor_selection.start(disp_idx);
                    }
                }
            } else {
                // グリッド外クリック: 選択解除
                app.ui_state.monitor_selection.clear();
            }
        }
    }

    // ドラッグ開始: 選択開始
    if response.drag_started_by(PointerButton::Primary) {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(cell_idx) = hit_test_cell(pos, rect, cols, cell_w, cell_h, max_idx) {
                if let Some(disp_idx) = cell_to_disp(cell_idx) {
                    if !shift {
                        app.ui_state.monitor_selection.start(disp_idx);
                    }
                }
            }
        }
    }

    // ドラッグ中: 選択範囲を拡張
    if response.dragged_by(PointerButton::Primary) {
        if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
            if let Some(cell_idx) = hit_test_cell(pos, rect, cols, cell_w, cell_h, max_idx) {
                if let Some(disp_idx) = cell_to_disp(cell_idx) {
                    app.ui_state.monitor_selection.extend(disp_idx);
                }
            }
        }
    }

}

/// コンテキストメニュー（右クリック）の描画 — 選択範囲がある場合のみ表示
fn draw_context_menu(response: &egui::Response, _ui: &mut Ui, app: &mut GlassApp) {
    if app.ui_state.monitor_selection.range().is_none() {
        return;
    }
    response.clone().context_menu(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        if ui.button(app.t.copy_mixed).clicked() {
            if let Some(range) = app.ui_state.monitor_selection.range() {
                let text = selection::format_monitor_mixed(app.display_buffer.cells(), range);
                ui.ctx().copy_text(text);
            }
            ui.close();
        }
        if ui.button(app.t.copy_hex).clicked() {
            if let Some(range) = app.ui_state.monitor_selection.range() {
                let text = selection::format_monitor_hex(app.display_buffer.cells(), range);
                ui.ctx().copy_text(text);
            }
            ui.close();
        }
        if ui.button(app.t.copy_binary).clicked() {
            if let Some(range) = app.ui_state.monitor_selection.range() {
                let text = selection::format_monitor_binary(app.display_buffer.cells(), range);
                ui.ctx().copy_text(text);
            }
            ui.close();
        }
    });
}

/// メインモニタビュー描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    if app.state != MonitorState::Paused {
        app.display_buffer
            .sync_entries(app.buffer.entries(), app.idle_threshold_ms);
    }

    let (cell_w, cell_h, cols) = calc_layout(ui);

    // 検索ハイライトがある場合は再描画（点滅アニメーション用、30fps制限）
    if app.search.has_highlights() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }

    match app.state {
        MonitorState::Running | MonitorState::Paused => {
            draw_ring_buffer(ui, app, cell_w, cell_h, cols);
        }
        MonitorState::Stopped => {
            draw_scrollable(ui, app, cell_w, cell_h, cols);
        }
    }
}

/// 取得中: 1画面リングバッファ上書き表示
fn draw_ring_buffer(ui: &mut Ui, app: &mut GlassApp, cell_w: f32, cell_h: f32, cols: usize) {
    let available = ui.available_size();
    let rows = (available.y / cell_h).floor().max(1.0) as usize;
    let total_cells = cols * rows;

    // Paused時はクリック&ドラッグ可、Running時はhoverのみ
    let sense = if app.state == MonitorState::Paused {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let desired = Vec2::new(cols as f32 * cell_w, rows as f32 * cell_h);
    let (rect, response) = ui.allocate_exact_size(desired, sense);
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, theme::GRID_BG);
    draw_row_lines(&painter, rect, cols, rows, cell_w, cell_h);

    let buf_len = app.display_buffer.len();
    let time = ui.input(|i| i.time);

    // Running時は選択をクリア
    if app.state == MonitorState::Running {
        app.ui_state.monitor_selection.clear();
    }

    // Paused時: 選択処理
    if app.state == MonitorState::Paused {
        let bl = buf_len;
        let tc = total_cells;
        handle_selection_input(
            ui, &response, app, rect, cols, cell_w, cell_h, total_cells,
            |cell_idx| map_cell_to_entry(cell_idx, bl, tc),
        );
        draw_context_menu(&response, ui, app);
    }

    for cell_idx in 0..total_cells {
        if let Some(disp_idx) = map_cell_to_entry(cell_idx, buf_len, total_cells) {
            let cell = &app.display_buffer.cells()[disp_idx];
            let cr = cell_rect(rect, cell_idx, cols, cell_w, cell_h);

            // 検索ハイライト背景
            let entry_idx = app.display_buffer.entry_indices()[disp_idx];
            if let Some(bg) = search_highlight_bg(&app.search, entry_idx, time) {
                painter.rect_filled(cr, 0.0, bg);
            }

            draw_cell(&painter, cr, cell, &app.display_mode, &app.monitor_colors);

            // 選択ハイライト（半透明オーバーレイ）
            if app.ui_state.monitor_selection.contains(disp_idx) {
                painter.rect_filled(cr, 0.0, theme::SELECTION_BG);
            }
        }
    }

    // ライブIDLEカウンタ (Running時のみ)
    let mut cursor_pos = buf_len % total_cells;
    if app.state == MonitorState::Running {
        if let Some(last_time) = app.last_byte_time {
            let elapsed_ms = last_time.elapsed().as_millis() as u64;
            let threshold = app.idle_threshold_ms as u64;
            if threshold > 0 && elapsed_ms > threshold {
                // 0埋め4桁カウンタ
                let count = (elapsed_ms / threshold).min(9999);
                let live_text = format!("{:04}", count);
                for (i, ch) in live_text.chars().enumerate() {
                    let idx = (buf_len + i) % total_cells;
                    let cr = cell_rect(rect, idx, cols, cell_w, cell_h);
                    draw_idle_char(&painter, cr, ch, &app.monitor_colors);
                }
                cursor_pos = (buf_len + live_text.len()) % total_cells;
            }
        }
    }

    // カーソル（書き込み位置）
    if buf_len > 0 || app.last_byte_time.is_some() {
        let cr = cell_rect(rect, cursor_pos, cols, cell_w, cell_h);
        painter.rect_filled(cr, 0.0, theme::CURSOR_FILL);
        painter.rect_stroke(cr, 0.0, Stroke::new(2.0, theme::CURSOR_STROKE), StrokeKind::Inside);
    }
}

/// 停止時: スクロールで全体を表示
fn draw_scrollable(ui: &mut Ui, app: &mut GlassApp, cell_w: f32, cell_h: f32, cols: usize) {
    let total_cells = app.display_buffer.len();
    if total_cells == 0 {
        ui.colored_label(
            theme::TEXT_MUTED,
            app.t.no_data,
        );
        return;
    }
    let total_rows = (total_cells + cols - 1) / cols;

    // スクロール先セルインデックスを計算
    let scroll_to_cell: Option<usize> = app.search.take_scroll_target().and_then(|entry_idx| {
        app.display_buffer.entry_indices().iter().position(|&ei| ei == entry_idx)
    });

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let desired = Vec2::new(cols as f32 * cell_w, total_rows as f32 * cell_h);
            let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, 0.0, theme::GRID_BG);
            draw_row_lines(&painter, rect, cols, total_rows, cell_w, cell_h);

            // 選択処理
            handle_selection_input(
                ui, &response, app, rect, cols, cell_w, cell_h, total_cells,
                |cell_idx| if cell_idx < total_cells { Some(cell_idx) } else { None },
            );
            draw_context_menu(&response, ui, app);

            let time = ui.input(|i| i.time);

            for (i, cell) in app.display_buffer.cells().iter().enumerate() {
                let cr = cell_rect(rect, i, cols, cell_w, cell_h);
                if cr.max.y < ui.clip_rect().min.y || cr.min.y > ui.clip_rect().max.y {
                    // スクロール先セルの場合はスキップしない（描画は必要）
                    if scroll_to_cell != Some(i) {
                        continue;
                    }
                }

                // 検索ハイライト背景
                let entry_idx = app.display_buffer.entry_indices()[i];
                if let Some(bg) = search_highlight_bg(&app.search, entry_idx, time) {
                    painter.rect_filled(cr, 0.0, bg);
                }

                draw_cell(&painter, cr, cell, &app.display_mode, &app.monitor_colors);

                // 選択ハイライト（半透明オーバーレイ）
                if app.ui_state.monitor_selection.contains(i) {
                    painter.rect_filled(cr, 0.0, theme::SELECTION_BG);
                }
            }

            // スクロール先にジャンプ
            if let Some(cell_idx) = scroll_to_cell {
                let target = cell_rect(rect, cell_idx, cols, cell_w, cell_h);
                ui.scroll_to_rect(target, Some(egui::Align::Center));
            }
        });
}

/// セルを1つ描画
fn draw_cell(painter: &egui::Painter, rect: Rect, cell: &DisplayCell, mode: &DisplayMode, colors: &MonitorColors) {
    match cell {
        DisplayCell::Data(byte) => {
            draw_data_byte(painter, rect, *byte, mode, colors);
        }
        DisplayCell::IdleChar(ch) => {
            draw_idle_char(painter, rect, *ch, colors);
        }
    }
}

/// IDLEカウンタ文字を描画（背景色で区別、縦積み表示）
fn draw_idle_char(painter: &egui::Painter, rect: Rect, ch: char, colors: &MonitorColors) {
    painter.rect_filled(rect, 0.0, colors.idle_bg_color32());
    let font_id = FontId::monospace(MAIN_FONT_SIZE);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        ch.to_string(),
        font_id,
        colors.idle_text_color32(),
    );
}

/// バイト値を "x00" 形式の文字列に変換
fn hex_label(byte: u8) -> String {
    format!("x{:02X}", byte)
}

/// データバイトを描画
fn draw_data_byte(painter: &egui::Painter, rect: Rect, byte: u8, mode: &DisplayMode, colors: &MonitorColors) {
    let rotated_font = FontId::monospace(ROTATED_FONT_SIZE);

    match mode {
        DisplayMode::Hex => {
            draw_rotated(painter, rect, &hex_label(byte), &rotated_font, colors.high_byte_color32());
        }
        DisplayMode::Ascii => {
            if byte >= 0x21 && byte <= 0x7E {
                let font_id = FontId::monospace(MAIN_FONT_SIZE);
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    String::from(byte as char),
                    font_id,
                    colors.data_color32(),
                );
            } else if byte <= 0x20 {
                let name = CONTROL_CODES[byte as usize];
                draw_rotated(painter, rect, name, &rotated_font, colors.control_color32());
            } else if byte == 0x7F {
                draw_rotated(painter, rect, "DEL", &rotated_font, colors.control_color32());
            } else {
                draw_rotated(painter, rect, &hex_label(byte), &rotated_font, colors.high_byte_color32());
            }
        }
    }
}

/// テキストを90°回転(反時計回り)してセル中央に描画
fn draw_rotated(painter: &egui::Painter, rect: Rect, text: &str, font_id: &FontId, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_string(), font_id.clone(), color);

    let w = galley.rect.width();
    let h = galley.rect.height();

    // 90°CCW回転後の中心をセル中央に合わせる
    let pos = Pos2::new(
        rect.center().x - h / 2.0,
        rect.center().y + w / 2.0,
    );

    let mut text_shape = TextShape::new(pos, galley, color);
    text_shape.angle = -std::f32::consts::FRAC_PI_2;
    painter.add(text_shape);
}
