# Glass - シリアルモニタ

## 技術スタック
- Rust + eframe/egui 0.34
- Windows Win32 API (シリアル通信)

## UI方針
- モダンデザイン
- アイコンは `egui-phosphor` (Phosphor Icons) を使用する。絵文字やUnicode記号は使わない
  - `use egui_phosphor::regular;` でインポート
  - 例: `regular::PLAY`, `regular::GEAR_SIX`, `regular::MAGNIFYING_GLASS`
- コメント・UIテキスト���日本語

## ビルド
```
cargo build
cargo run
```
