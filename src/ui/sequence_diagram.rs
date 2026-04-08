use std::sync::{Arc, OnceLock};

use crossbeam_channel::{Receiver, Sender};
use egui::{ColorImage, ScrollArea, TextureHandle, TextureOptions, Vec2};
use egui_phosphor::regular;

use crate::app::GlassApp;
use crate::protocol::definition::{FieldDef, MessageDef, ProtocolFile, SequenceConfig};
use crate::protocol::engine::MatchedMessage;
use crate::ui::japanese_font;
use crate::ui::protocol_panel::extract_ascii;
use crate::ui::theme;

/// バックグラウンド生成の結果
struct GenerateResult {
    svg: String,
    image: ColorImage,
}

/// シーケンス図の状態
pub struct SequenceDiagramState {
    pub open: bool,
    /// コンテキストメニューからの生成要求（借用制約によりフラグ経由で遅延実行）
    pub generate_requested: bool,
    generating: bool,
    svg: String,
    /// PNG保存用のラスタライズ済み画像データ
    png_rgba: Vec<u8>,
    png_size: [u32; 2],
    texture: Option<TextureHandle>,
    /// プレビュー用テクスチャの実サイズ (等倍。GPU上限超過時は切り出し後のサイズ)
    image_size: [usize; 2],
    /// 元画像の等倍サイズ (省略表示用)
    full_size: [usize; 2],
    result_rx: Option<Receiver<Result<GenerateResult, String>>>,
}

impl SequenceDiagramState {
    pub fn new() -> Self {
        Self {
            open: false,
            generate_requested: false,
            generating: false,
            svg: String::new(),
            png_rgba: Vec::new(),
            png_size: [0, 0],
            texture: None,
            image_size: [0, 0],
            full_size: [0, 0],
            result_rx: None,
        }
    }
}

// ===== 式評価 =====

/// 式を評価してフィールド値を取得
/// - "=Literal" → リテラル値
/// - "{field1}:{field2}" → テンプレー��置換
/// - "fieldname" → フィールド値をそのまま取得
fn eval_expr(expr: &str, bytes: &[u8], fields: &[FieldDef]) -> Option<String> {
    if let Some(literal) = expr.strip_prefix('=') {
        return Some(literal.to_string());
    }
    if expr.contains('{') {
        let mut result = expr.to_string();
        while let Some(start) = result.find('{') {
            if let Some(end) = result[start..].find('}') {
                let field_name = &result[start + 1..start + end];
                if let Some(field) = fields.iter().find(|f| f.name == field_name) {
                    let value = extract_ascii(bytes, field.offset, field.size);
                    if value == "—" {
                        return None;
                    }
                    result = format!(
                        "{}{}{}",
                        &result[..start],
                        value,
                        &result[start + end + 1..]
                    );
                } else {
                    return None;
                }
            } else {
                break;
            }
        }
        return Some(result);
    }
    // 単純フィールド名
    if let Some(field) = fields.iter().find(|f| f.name == expr) {
        let value = extract_ascii(bytes, field.offset, field.size);
        if value != "—" {
            return Some(value);
        }
    }
    None
}

/// メッセージの送信元・宛先を解決
fn resolve_endpoints(
    matched: &MatchedMessage,
    msg_def: Option<&MessageDef>,
    seq_config: &SequenceConfig,
) -> (Option<String>, Option<String>) {
    let bytes = &matched.frame.bytes;
    let fields = msg_def.map(|d| d.fields.as_slice()).unwrap_or(&[]);

    let source_expr = msg_def
        .and_then(|d| d.sequence_source.as_deref())
        .or(seq_config.source.as_deref());
    let source = source_expr.and_then(|expr| eval_expr(expr, bytes, fields));

    let dest_expr = msg_def
        .and_then(|d| d.sequence_destination.as_deref())
        .or(seq_config.destination.as_deref());
    let dest = dest_expr.and_then(|expr| eval_expr(expr, bytes, fields));

    (source, dest)
}

// ===== Mermaid構文生成 =====

fn escape_mermaid(s: &str) -> String {
    s.replace('#', "#35;").replace(';', "#59;")
}

fn build_label(matched: &MatchedMessage, msg_def: Option<&MessageDef>) -> String {
    let title = msg_def.map(|d| d.title.as_str()).unwrap_or("Unmatched");
    let mut parts = Vec::new();
    if let Some(def) = msg_def {
        for field in def.fields.iter().filter(|f| f.inline) {
            let value = extract_ascii(&matched.frame.bytes, field.offset, field.size);
            if value != "—" {
                parts.push(format!("{}:{}", field.name, value));
            }
        }
    }
    if parts.is_empty() {
        escape_mermaid(title)
    } else {
        escape_mermaid(&format!("{} ({})", title, parts.join(", ")))
    }
}

struct Arrow {
    src: String,
    dest: String,
    label: String,
    is_response: bool,
    idle_ms: Option<f64>,
    is_broadcast: bool,
}

fn build_mermaid(
    matches: &[MatchedMessage],
    proto: &ProtocolFile,
    seq_config: &SequenceConfig,
    range: (usize, usize),
) -> String {
    let mut lines = Vec::new();
    lines.push("sequenceDiagram".to_string());

    let mut participants: Vec<String> = Vec::new();
    let mut add_participant = |name: &str| {
        if !participants.contains(&name.to_string()) {
            participants.push(name.to_string());
        }
    };

    let mut arrows: Vec<Arrow> = Vec::new();
    let mut last_src: Option<String> = None;
    let mut last_dest: Option<String> = None;

    let end = range.1.min(matches.len().saturating_sub(1));
    for matched in &matches[range.0..=end] {
        let msg_def = matched.message_def_idx.map(|i| &proto.messages[i]);
        let (source, dest) = resolve_endpoints(matched, msg_def, seq_config);
        let label = build_label(matched, msg_def);
        let idle_ms = matched.preceding_idle_ms.filter(|&ms| ms >= 1.0);

        let is_broadcast = if let (Some(d), Some(bc)) = (&dest, &seq_config.broadcast) {
            d == bc
        } else {
            false
        };

        if is_broadcast {
            let bc_src = source
                .clone()
                .or_else(|| last_dest.clone())
                .unwrap_or_else(|| "?".to_string());
            if bc_src != "?" {
                add_participant(&bc_src);
            }
            arrows.push(Arrow {
                src: bc_src.clone(),
                dest: String::new(),
                label,
                is_response: false,
                idle_ms,
                is_broadcast: true,
            });
            last_src = Some(bc_src);
        } else {
            match (&source, &dest) {
                (Some(s), Some(d)) => {
                    add_participant(s);
                    add_participant(d);
                    arrows.push(Arrow {
                        src: s.clone(),
                        dest: d.clone(),
                        label,
                        is_response: false,
                        idle_ms,
                        is_broadcast: false,
                    });
                    last_src = Some(s.clone());
                    last_dest = Some(d.clone());
                }
                (Some(s), None) => {
                    add_participant(s);
                    let response_dest = last_src.clone().unwrap_or_else(|| "?".to_string());
                    if response_dest != "?" {
                        add_participant(&response_dest);
                    }
                    arrows.push(Arrow {
                        src: s.clone(),
                        dest: response_dest.clone(),
                        label,
                        is_response: true,
                        idle_ms,
                        is_broadcast: false,
                    });
                    last_src = Some(s.clone());
                    last_dest = Some(response_dest);
                }
                (None, Some(d)) => {
                    add_participant(d);
                    let inferred_src = last_dest.clone().unwrap_or_else(|| "?".to_string());
                    if inferred_src != "?" {
                        add_participant(&inferred_src);
                    }
                    arrows.push(Arrow {
                        src: inferred_src.clone(),
                        dest: d.clone(),
                        label,
                        is_response: false,
                        idle_ms,
                        is_broadcast: false,
                    });
                    last_src = Some(inferred_src);
                    last_dest = Some(d.clone());
                }
                (None, None) => {
                    let resp_src = last_dest.clone().unwrap_or_else(|| "?".to_string());
                    let resp_dest = last_src.clone().unwrap_or_else(|| "?".to_string());
                    if resp_src != "?" {
                        add_participant(&resp_src);
                    }
                    if resp_dest != "?" {
                        add_participant(&resp_dest);
                    }
                    arrows.push(Arrow {
                        src: resp_src.clone(),
                        dest: resp_dest.clone(),
                        label,
                        is_response: true,
                        idle_ms,
                        is_broadcast: false,
                    });
                    last_src = Some(resp_src);
                    last_dest = Some(resp_dest);
                }
            }
        }
    }

    participants.sort();
    for p in &participants {
        lines.push(format!("    participant {}", p));
    }

    let first_p = participants.first().cloned().unwrap_or_default();
    let last_p = participants.last().cloned().unwrap_or_default();

    for arrow in &arrows {
        if let Some(ms) = arrow.idle_ms
            && !first_p.is_empty()
            && !last_p.is_empty()
        {
            lines.push(format!(
                "    Note over {},{}: IDLE {}ms",
                first_p, last_p, ms as u64
            ));
        }

        if arrow.is_broadcast {
            if !first_p.is_empty() && !last_p.is_empty() {
                lines.push(format!(
                    "    Note over {},{}: [Broadcast] {}",
                    first_p, last_p, arrow.label
                ));
            }
        } else if arrow.is_response {
            lines.push(format!(
                "    {}-->>{}:  {}",
                arrow.src, arrow.dest, arrow.label
            ));
        } else {
            lines.push(format!(
                "    {}->>{}:  {}",
                arrow.src, arrow.dest, arrow.label
            ));
        }
    }

    lines.join("\n")
}

// ===== SVGレンダリング =====

/// Color32をHEX文字列に変換
fn color32_to_hex(c: egui::Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// アプリの配色に合わせたダークテーマ
fn dark_theme() -> mermaid_rs_renderer::Theme {
    let mut t = mermaid_rs_renderer::Theme::modern();
    if let Some((_, family)) = japanese_font::chosen_font() {
        t.font_family = format!("{}, sans-serif", family);
    }
    t.background = color32_to_hex(theme::GRID_BG);
    t.text_color = "#C8CDD5".to_string();
    t.primary_text_color = "#C8CDD5".to_string();
    t.line_color = color32_to_hex(theme::TEXT_MUTED);
    t.sequence_actor_fill = color32_to_hex(theme::GRID_LINE);
    t.sequence_actor_border = "#5A6080".to_string();
    t.sequence_actor_line = "#5A6080".to_string();
    t.sequence_note_fill = color32_to_hex(theme::IDLE_BG);
    t.sequence_note_border = color32_to_hex(theme::IDLE_TEXT);
    t.sequence_activation_fill = color32_to_hex(theme::GRID_LINE);
    t.sequence_activation_border = "#5A6080".to_string();
    t.edge_label_background = color32_to_hex(theme::GRID_BG);
    t
}

fn render_svg(mermaid_text: &str) -> Result<String, String> {
    let options = mermaid_rs_renderer::RenderOptions {
        theme: dark_theme(),
        layout: mermaid_rs_renderer::LayoutConfig::default(),
    };
    mermaid_rs_renderer::render_with_options(mermaid_text, options)
        .map_err(|e| format!("Mermaid render error: {}", e))
}

/// キャッシュ済みフォントDB（UIと同じ日本語フォントのみロード）
fn cached_fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    static FONTDB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTDB
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            if let Some((path, _)) = japanese_font::chosen_font() {
                let _ = db.load_font_file(path);
            }
            // フォールバック: 日本語フォントが見つからない場合はシステムフォント
            if db.is_empty() {
                db.load_system_fonts();
            }
            Arc::new(db)
        })
        .clone()
}

fn rasterize_svg(svg_data: &str) -> Result<ColorImage, String> {
    let mut opt = resvg::usvg::Options::default();
    *opt.fontdb_mut() = (*cached_fontdb()).clone();
    let tree = resvg::usvg::Tree::from_str(svg_data, &opt)
        .map_err(|e| format!("SVG parse error: {}", e))?;
    let size = tree.size();
    let w = size.width().ceil() as u32;
    let h = size.height().ceil() as u32;
    if w == 0 || h == 0 {
        return Err("SVG size is zero".to_string());
    }
    // 異常に大きな画像は OOM やレンダラ崩壊の原因になるため事前に弾く
    // (32768px は一般的な GPU max_texture_side の最大値の目安)
    const MAX_DIM: u32 = 32768;
    if w > MAX_DIM || h > MAX_DIM {
        return Err(format!(
            "シーケンス図が大きすぎます ({}x{}px)。範囲を狭めて再生成してください",
            w, h
        ));
    }
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| "Failed to create pixmap".to_string())?;
    let bg = theme::GRID_BG;
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(
        bg.r(),
        bg.g(),
        bg.b(),
        0xFF,
    ));
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let pixels: Vec<egui::Color32> = pixmap
        .pixels()
        .iter()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p.red(), p.green(), p.blue(), p.alpha()))
        .collect();
    Ok(ColorImage {
        size: [w as usize, h as usize],
        pixels,
        source_size: Vec2::new(w as f32, h as f32),
    })
}

// ===== ファイル保存 =====

fn save_svg(svg: &str) -> Result<(), String> {
    let path = rfd::FileDialog::new()
        .add_filter("SVG", &["svg"])
        .set_file_name("sequence.svg")
        .save_file();
    if let Some(path) = path {
        std::fs::write(&path, svg).map_err(|e| format!("SVG save error: {}", e))?;
    }
    Ok(())
}

fn save_png(rgba: &[u8], size: [u32; 2]) -> Result<(), String> {
    let path = rfd::FileDialog::new()
        .add_filter("PNG", &["png"])
        .set_file_name("sequence.png")
        .save_file();
    if let Some(path) = path {
        let img = image::RgbaImage::from_raw(size[0], size[1], rgba.to_vec())
            .ok_or_else(|| "Failed to create image".to_string())?;
        img.save(&path)
            .map_err(|e| format!("PNG save error: {}", e))?;
    }
    Ok(())
}

// ===== バックグラウンド生成 =====

fn start_generate(
    matches: &[MatchedMessage],
    proto: &ProtocolFile,
    seq_config: &SequenceConfig,
    range: (usize, usize),
) -> Receiver<Result<GenerateResult, String>> {
    let (tx, rx): (Sender<Result<GenerateResult, String>>, _) = crossbeam_channel::bounded(1);

    let end = range.1.min(matches.len().saturating_sub(1));
    let start = range.0.min(end);
    let matches_clone: Vec<MatchedMessage> = matches[start..=end].to_vec();
    let proto_clone = proto.clone();
    let seq_config_clone = seq_config.clone();
    let adjusted_range = (0, matches_clone.len().saturating_sub(1));

    std::thread::spawn(move || {
        let mermaid_text = build_mermaid(
            &matches_clone,
            &proto_clone,
            &seq_config_clone,
            adjusted_range,
        );
        let result = render_svg(&mermaid_text).and_then(|svg| {
            let image = rasterize_svg(&svg)?;
            Ok(GenerateResult { svg, image })
        });
        let _ = tx.send(result);
    });

    rx
}

// ===== UI描画 =====

pub fn draw(ctx: &egui::Context, app: &mut GlassApp) {
    // コンテキストメニューからの生成要求 → バックグラウンドスレッド起動
    if app.ui_state.sequence_diagram.generate_requested {
        app.ui_state.sequence_diagram.generate_requested = false;

        let proto = app.loaded_protocol.as_ref();
        let seq_config = proto.and_then(|p| p.protocol.sequence.as_ref());
        // 選択 ID 範囲 → 現在のインデックス範囲に解決
        let range = app
            .ui_state
            .protocol_selection
            .range()
            .and_then(|(lo_id, hi_id)| {
                let lo = app.protocol_state.position_by_id(lo_id)?;
                let hi = app
                    .protocol_state
                    .position_by_id(hi_id)
                    .unwrap_or_else(|| app.protocol_state.matches.len().saturating_sub(1));
                if lo > hi { None } else { Some((lo, hi)) }
            });

        if let (Some(proto), Some(seq_config), Some(range)) = (proto, seq_config, range) {
            let rx = start_generate(&app.protocol_state.matches, proto, seq_config, range);
            app.ui_state.sequence_diagram.result_rx = Some(rx);
            app.ui_state.sequence_diagram.generating = true;
            app.ui_state.sequence_diagram.texture = None;
            app.ui_state.sequence_diagram.svg.clear();
            app.ui_state.sequence_diagram.png_rgba.clear();
            app.ui_state.sequence_diagram.open = true;
        }
    }

    // バックグラウンド結果の受信
    if app.ui_state.sequence_diagram.generating {
        let done = if let Some(ref rx) = app.ui_state.sequence_diagram.result_rx {
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    let [w, h] = result.image.size;
                    app.ui_state.sequence_diagram.full_size = [w, h];
                    // PNG保存用に等倍RGBAデータを保持
                    app.ui_state.sequence_diagram.png_size = [w as u32, h as u32];
                    app.ui_state.sequence_diagram.png_rgba = result
                        .image
                        .pixels
                        .iter()
                        .flat_map(|c| c.to_array())
                        .collect();

                    // GPU の最大テクスチャサイズを超える場合は load_texture が
                    // wgpu 内部で panic するため、等倍のまま左上から切り出して
                    // 表示可能な範囲のみアップロードする (縮小はしない)。
                    let max_side = ctx.input(|i| i.max_texture_side);
                    let cw = w.min(max_side);
                    let ch = h.min(max_side);
                    let preview_image = if cw == w && ch == h {
                        result.image
                    } else {
                        let mut pixels = Vec::with_capacity(cw * ch);
                        for y in 0..ch {
                            let row_start = y * w;
                            pixels
                                .extend_from_slice(&result.image.pixels[row_start..row_start + cw]);
                        }
                        ColorImage {
                            size: [cw, ch],
                            pixels,
                            source_size: Vec2::new(cw as f32, ch as f32),
                        }
                    };
                    app.ui_state.sequence_diagram.image_size = [cw, ch];
                    let texture =
                        ctx.load_texture("sequence_diagram", preview_image, TextureOptions::LINEAR);
                    app.ui_state.sequence_diagram.texture = Some(texture);
                    app.ui_state.sequence_diagram.svg = result.svg;
                    true
                }
                Ok(Err(e)) => {
                    app.show_error(&format!("シーケンス図生成エラー: {}", e));
                    app.ui_state.sequence_diagram.open = false;
                    true
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    ctx.request_repaint();
                    false
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    app.show_error("シーケンス図生成スレッドが異常終了しました");
                    app.ui_state.sequence_diagram.open = false;
                    true
                }
            }
        } else {
            true
        };
        if done {
            app.ui_state.sequence_diagram.generating = false;
            app.ui_state.sequence_diagram.result_rx = None;
        }
    }

    if !app.ui_state.sequence_diagram.open {
        return;
    }

    let generating = app.ui_state.sequence_diagram.generating;
    let viewport_id = egui::ViewportId::from_hash_of("sequence_diagram_viewport");

    ctx.show_viewport_immediate(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title(app.t.sequence_diagram)
            .with_inner_size([800.0, 600.0]),
        |ctx, _class| {
            if ctx.input(|i| i.viewport().close_requested()) {
                app.ui_state.sequence_diagram.open = false;
            }

            #[allow(deprecated)]
            egui::CentralPanel::default().show(ctx, |ui| {
                if generating {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.spinner();
                        ui.add_space(12.0);
                        ui.label(app.t.sequence_generating);
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui
                            .button(format!("{}  {}", regular::FLOPPY_DISK, app.t.save_svg))
                            .clicked()
                            && let Err(e) = save_svg(&app.ui_state.sequence_diagram.svg)
                        {
                            app.show_error(&e);
                        }
                        if ui
                            .button(format!("{}  {}", regular::IMAGE, app.t.save_png))
                            .clicked()
                            && let Err(e) = save_png(
                                &app.ui_state.sequence_diagram.png_rgba,
                                app.ui_state.sequence_diagram.png_size,
                            )
                        {
                            app.show_error(&e);
                        }
                    });
                    ui.separator();

                    if let Some(ref texture) = app.ui_state.sequence_diagram.texture {
                        let [w, h] = app.ui_state.sequence_diagram.image_size;
                        let [fw, fh] = app.ui_state.sequence_diagram.full_size;
                        if w < fw || h < fh {
                            ui.label(format!(
                                "プレビューは {}x{}px のみ表示しています (全体 {}x{}px)。全体は SVG / PNG 保存で確認してください",
                                w, h, fw, fh
                            ));
                            ui.separator();
                        }
                        ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.image(egui::load::SizedTexture::new(
                                    texture.id(),
                                    Vec2::new(w as f32, h as f32),
                                ));
                            });
                    }
                }
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, offset: usize, size: usize) -> FieldDef {
        FieldDef {
            name: name.to_string(),
            offset,
            size,
            description: None,
            inline: false,
        }
    }

    #[test]
    fn literal_expression() {
        let r = eval_expr("=Device1", &[], &[]);
        assert_eq!(r.as_deref(), Some("Device1"));
    }

    #[test]
    fn template_expression_extracts_field() {
        let bytes = b"ABCD";
        let fields = vec![field("ID", 0, 2)];
        let r = eval_expr("DEV_{ID}", bytes, &fields);
        assert_eq!(r.as_deref(), Some("DEV_AB"));
    }

    #[test]
    fn plain_field_name_lookup() {
        let bytes = b"XYZ";
        let fields = vec![field("Tag", 0, 3)];
        let r = eval_expr("Tag", bytes, &fields);
        assert_eq!(r.as_deref(), Some("XYZ"));
    }

    #[test]
    fn missing_field_returns_none() {
        let r = eval_expr("{Missing}", b"AB", &[]);
        assert_eq!(r, None);
    }

    #[test]
    fn out_of_range_field_returns_none() {
        let fields = vec![field("ID", 10, 2)];
        let r = eval_expr("{ID}", b"AB", &fields);
        assert_eq!(r, None);
    }
}
