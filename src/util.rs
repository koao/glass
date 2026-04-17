//! 共通ユーティリティ。

use std::path::{Path, PathBuf};

/// 実行ファイル (exe) のあるディレクトリ。解決できない場合はカレントディレクトリを返す。
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}
