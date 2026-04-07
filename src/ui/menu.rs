//! メニューUI共通ヘルパー
//!
//! 右クリックの `context_menu` とボタン式の `menu_button` の中身を統一的に
//! 描画するためのユーティリティ。各項目は **アイコン (任意) / タイトル (必須)
//! / ショートカット (任意)** の3要素で構成され、列の開始位置が揃うように
//! 描画される。幅は中身に合わせて自動調整される。

use egui::{Button, Ui};

#[derive(Clone, Copy)]
pub struct MenuItem<'a> {
    icon: Option<&'a str>,
    title: &'a str,
    shortcut: Option<&'a str>,
    enabled: bool,
}

impl<'a> MenuItem<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            icon: None,
            title,
            shortcut: None,
            enabled: true,
        }
    }

    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// メニューを描画する。クリックされた項目のインデックスを返す。
///
/// クリック後にメニューを閉じるかどうかは呼び出し側で `ui.close()` を呼ぶ。
/// 無効な項目はクリックされても `Some(idx)` を返さない。
pub fn show(ui: &mut Ui, items: &[MenuItem]) -> Option<usize> {
    let any_icon = items.iter().any(|i| i.icon.is_some());
    // アイコンなし項目のタイトル列を揃えるための擬似パディング
    const ICON_PAD: &str = "      ";

    // ラベルを一度だけ生成して測定と描画で使い回す
    let labels: Vec<String> = items
        .iter()
        .map(|i| build_label(i, any_icon, ICON_PAD))
        .collect();

    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let painter = ui.painter().clone();
    let mut max_label_w: f32 = 0.0;
    let mut max_shortcut_w: f32 = 0.0;
    for (item, label) in items.iter().zip(labels.iter()) {
        let galley = painter.layout_no_wrap(label.clone(), font_id.clone(), egui::Color32::WHITE);
        max_label_w = max_label_w.max(galley.rect.width());
        if let Some(sc) = item.shortcut {
            let galley =
                painter.layout_no_wrap(sc.to_string(), font_id.clone(), egui::Color32::WHITE);
            max_shortcut_w = max_shortcut_w.max(galley.rect.width());
        }
    }

    let gap = if max_shortcut_w > 0.0 { 24.0 } else { 0.0 };
    let inner_w = max_label_w + gap + max_shortcut_w;
    let button_padding = ui.spacing().button_padding.x * 2.0;
    let total_w = inner_w + button_padding;

    // 親 Ui のキャッシュ済みサイズに引きずられないよう、毎フレーム
    // 必要な幅で固定した子 Ui を確保してその中で描画する
    let mut clicked = None;
    ui.allocate_ui_with_layout(
        egui::vec2(total_w, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(total_w);
            ui.set_max_width(total_w);
            ui.spacing_mut().item_spacing.y = 10.0;
            for (idx, (item, label)) in items.iter().zip(labels).enumerate() {
                let mut btn = Button::new(label).min_size(egui::vec2(inner_w, 0.0));
                if let Some(sc) = item.shortcut {
                    btn = btn.shortcut_text(sc);
                }
                if ui.add_enabled(item.enabled, btn).clicked() {
                    clicked = Some(idx);
                }
            }
        },
    );
    clicked
}

fn build_label(item: &MenuItem, any_icon: bool, pad: &str) -> String {
    match item.icon {
        Some(icon) => format!("{}  {}", icon, item.title),
        None if any_icon => format!("{}{}", pad, item.title),
        None => item.title.to_string(),
    }
}
