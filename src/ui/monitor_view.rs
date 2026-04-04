use egui::{Align2, Color32, FontId, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::app::{DisplayMode, GlassApp, MonitorState};
use crate::model::grid::DisplayCell;
use crate::ui::theme;

// === フォントサイズ ===
const MAIN_FONT_SIZE: f32 = 20.0;
const STACKED_FONT_SIZE: f32 = 11.0;

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
    let cell_h = MAIN_FONT_SIZE * 2.2;
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

/// メインモニタビュー描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    if app.state != MonitorState::Paused {
        app.display_buffer
            .sync_entries(app.buffer.entries(), app.idle_threshold_ms);
    }

    let (cell_w, cell_h, cols) = calc_layout(ui);

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
fn draw_ring_buffer(ui: &mut Ui, app: &GlassApp, cell_w: f32, cell_h: f32, cols: usize) {
    let available = ui.available_size();
    let rows = (available.y / cell_h).floor().max(1.0) as usize;
    let total_cells = cols * rows;

    let desired = Vec2::new(cols as f32 * cell_w, rows as f32 * cell_h);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, theme::GRID_BG);
    draw_row_lines(&painter, rect, cols, rows, cell_w, cell_h);

    let buf_len = app.display_buffer.len();

    for cell_idx in 0..total_cells {
        if let Some(entry_idx) = map_cell_to_entry(cell_idx, buf_len, total_cells) {
            let cell = &app.display_buffer.cells()[entry_idx];
            let cr = cell_rect(rect, cell_idx, cols, cell_w, cell_h);
            draw_cell(&painter, cr, cell, &app.display_mode);
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
                    draw_idle_char(&painter, cr, ch);
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
fn draw_scrollable(ui: &mut Ui, app: &GlassApp, cell_w: f32, cell_h: f32, cols: usize) {
    let total_cells = app.display_buffer.len();
    if total_cells == 0 {
        ui.colored_label(
            theme::TEXT_MUTED,
            "データなし — COMポートを選択して開始してください",
        );
        return;
    }
    let total_rows = (total_cells + cols - 1) / cols;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let desired = Vec2::new(cols as f32 * cell_w, total_rows as f32 * cell_h);
            let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, 0.0, theme::GRID_BG);
            draw_row_lines(&painter, rect, cols, total_rows, cell_w, cell_h);

            for (i, cell) in app.display_buffer.cells().iter().enumerate() {
                let cr = cell_rect(rect, i, cols, cell_w, cell_h);
                if cr.max.y < ui.clip_rect().min.y || cr.min.y > ui.clip_rect().max.y {
                    continue;
                }
                draw_cell(&painter, cr, cell, &app.display_mode);
            }
        });
}

/// セルを1つ描画
fn draw_cell(painter: &egui::Painter, rect: Rect, cell: &DisplayCell, mode: &DisplayMode) {
    match cell {
        DisplayCell::Data(byte) => {
            draw_data_byte(painter, rect, *byte, mode);
        }
        DisplayCell::IdleChar(ch) => {
            draw_idle_char(painter, rect, *ch);
        }
    }
}

/// IDLEカウンタ文字を描画（背景色で区別、縦積み表示）
fn draw_idle_char(painter: &egui::Painter, rect: Rect, ch: char) {
    painter.rect_filled(rect, 0.0, theme::IDLE_BG);
    let font_id = FontId::monospace(MAIN_FONT_SIZE);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        ch.to_string(),
        font_id,
        theme::IDLE_TEXT,
    );
}

/// データバイトを描画
fn draw_data_byte(painter: &egui::Painter, rect: Rect, byte: u8, mode: &DisplayMode) {
    match mode {
        DisplayMode::Hex => {
            // 2桁HEXを縦積み表示
            let hi = format!("{:X}", byte >> 4);
            let lo = format!("{:X}", byte & 0x0F);
            draw_stacked(painter, rect, &[&hi, &lo], theme::DATA_COLOR);
        }
        DisplayMode::Ascii => {
            if byte >= 0x21 && byte <= 0x7E {
                // 印字可能文字 (0x21-0x7E): 通常表示
                let font_id = FontId::monospace(MAIN_FONT_SIZE);
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    String::from(byte as char),
                    font_id,
                    theme::DATA_COLOR,
                );
            } else if byte <= 0x20 {
                // 制御コード + スペース (0x00-0x20): 縦積み表示
                let name = CONTROL_CODES[byte as usize];
                draw_stacked_str(painter, rect, name, theme::CONTROL_COLOR);
            } else if byte == 0x7F {
                draw_stacked_str(painter, rect, "DEL", theme::CONTROL_COLOR);
            } else {
                // ASCII範囲外 (0x80-0xFF): HEX縦積み表示
                let hi = format!("{:X}", byte >> 4);
                let lo = format!("{:X}", byte & 0x0F);
                draw_stacked(painter, rect, &[&hi, &lo], theme::HIGH_BYTE_COLOR);
            }
        }
    }
}

/// 文字列を縦積み表示
fn draw_stacked_str(painter: &egui::Painter, rect: Rect, text: &str, color: Color32) {
    let chars: Vec<String> = text.chars().map(|c| c.to_string()).collect();
    let refs: Vec<&str> = chars.iter().map(|s| s.as_str()).collect();
    draw_stacked(painter, rect, &refs, color);
}

/// 複数文字を縦積みで描画
fn draw_stacked(painter: &egui::Painter, rect: Rect, lines: &[&str], color: Color32) {
    let font_id = FontId::monospace(STACKED_FONT_SIZE);
    let n = lines.len() as f32;
    let line_h = STACKED_FONT_SIZE * 1.2;
    let total_h = n * line_h;
    let start_y = rect.center().y - total_h / 2.0 + line_h / 2.0;

    for (i, line) in lines.iter().enumerate() {
        let pos = Pos2::new(rect.center().x, start_y + i as f32 * line_h);
        painter.text(pos, Align2::CENTER_CENTER, *line, font_id.clone(), color);
    }
}
