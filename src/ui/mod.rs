/// 画面サイズの微小変動によるレイアウトジッターを防止するキャッシュ付きカウント計算。
/// `available` が前回値から ±1px 以内ならキャッシュ値を返す。
pub fn stable_count(available: f32, unit: f32, cached_size: &mut f32, cached: &mut usize) -> usize {
    if *cached > 0 && (available - *cached_size).abs() <= 1.0 {
        *cached
    } else {
        let c = (available / unit).floor().max(1.0) as usize;
        *cached_size = available;
        *cached = c;
        c
    }
}

pub mod dialog;
pub mod header_bar;
pub mod japanese_font;
pub mod menu;
pub mod monitor_view;
pub mod protocol_panel;
pub mod protocol_search;
pub mod search;
pub mod search_bar;
pub mod selection;
pub mod sequence_diagram;
pub mod settings_window;
pub mod status_bar;
pub mod theme;
pub mod trigger_window;
