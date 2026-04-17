//! 例外・異常のログ出力。
//!
//! - 出力先: exe と同階層の `log/` サブディレクトリ配下に日次ローテーションで
//!   `glass.YYYY-MM-DD.log` を作る（`tracing-appender` の rolling appender）。
//! - 書き込みは `non_blocking` により専用スレッド経由で行うため、シリアル受信
//!   スレッド等のホットパスはブロッキングされない。
//! - `log` クレート経由の出力（eframe / egui / serialport / windows-sys など
//!   上流ライブラリ）も `tracing_log::LogTracer` で橋渡しされ、同じログに集約。
//! - `std::panic::set_hook` で全スレッドの panic を捕捉し、バックトレース付きで
//!   記録する。保険として `log/glass_panic_fallback.log` にも追記する。
//! - 起動時に保持日数を超えた古いログファイルを削除する。

use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// ログファイルの保持日数。これより古い `glass.*.log` は起動時に削除する。
const LOG_RETENTION_DAYS: u64 = 7;

/// ログ出力ディレクトリ名（exe 隣）
const LOG_DIR_NAME: &str = "log";
/// ローテーション対象のファイル名プレフィクス
const LOG_FILE_PREFIX: &str = "glass";
/// ローテーション対象のファイル名サフィクス
const LOG_FILE_SUFFIX: &str = "log";
/// tracing の target: アプリ本体イベント用
const TARGET_APP: &str = "glass";
/// tracing の target: panic フック用
const TARGET_PANIC: &str = "panic";
/// パニック時の保険ログファイル名
const PANIC_FALLBACK_FILENAME: &str = "glass_panic_fallback.log";

fn log_dir() -> PathBuf {
    crate::util::exe_dir().join(LOG_DIR_NAME)
}

/// ロガーを初期化し、パニックフックを設置する。
///
/// 戻り値の `WorkerGuard` は `main` のスコープ末尾まで保持すること（drop で
/// 未書き込みバッファをフラッシュするため、早期に drop すると直前のログが
/// 失われる）。何らかの理由で初期化に失敗しても panic させず `None` を返し、
/// 呼び出し側はそのまま実行を継続できる。
pub fn init() -> Option<WorkerGuard> {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);

    prune_old_logs(&dir, Duration::from_secs(LOG_RETENTION_DAYS * 24 * 60 * 60));

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix(LOG_FILE_SUFFIX)
        .build(&dir)
        .ok()?;

    let (writer, guard) = tracing_appender::non_blocking(appender);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .with_level(true);

    if tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init()
        .is_err()
    {
        return None;
    }

    let _ = tracing_log::LogTracer::init();

    tracing::info!(
        target: TARGET_APP,
        version = env!("CARGO_PKG_VERSION"),
        "=== Glass started ==="
    );

    install_panic_hook();

    Some(guard)
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".to_string()
        };

        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());

        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>").to_string();

        let backtrace = Backtrace::force_capture();

        tracing::error!(
            target: TARGET_PANIC,
            payload = %payload,
            location = %location,
            thread = %thread_name,
            "panic occurred\n{}",
            backtrace
        );

        let _ = write_fallback(&payload, &location, &thread_name, &backtrace);
    }));
}

fn write_fallback(
    payload: &str,
    location: &str,
    thread: &str,
    backtrace: &Backtrace,
) -> std::io::Result<()> {
    let path = log_dir().join(PANIC_FALLBACK_FILENAME);
    let mut f = OpenOptions::new().append(true).create(true).open(path)?;
    writeln!(
        f,
        "---\npanic at {} (thread {}):\n{}\nbacktrace:\n{}",
        location, thread, payload, backtrace
    )
}

/// ローテーションされたログファイルかどうかを判定する。
///
/// `glass.<date>.log` 形式（例: `glass.2026-04-17.log`）のみを対象とし、
/// `glass_panic_fallback.log` や無関係なファイルには触らない。
fn is_rotated_log_file(file_name: &str) -> bool {
    let Some(rest) = file_name
        .strip_prefix(LOG_FILE_PREFIX)
        .and_then(|r| r.strip_prefix('.'))
    else {
        return false;
    };
    let Some(middle) = rest
        .strip_suffix(LOG_FILE_SUFFIX)
        .and_then(|m| m.strip_suffix('.'))
    else {
        return false;
    };
    // 中間部分は非空で、ドット・スラッシュ・バックスラッシュを含まない
    !middle.is_empty() && !middle.contains('.') && !middle.contains('/') && !middle.contains('\\')
}

/// `dir` 配下のローテーション済みログファイルのうち、mtime が `retention`
/// より古いものを削除する。失敗しても panic させず、静かにスキップする。
fn prune_old_logs(dir: &Path, retention: Duration) {
    let Some(threshold) = SystemTime::now().checked_sub(retention) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_rotated_log_file(file_name) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = metadata.modified() else {
            continue;
        };
        if mtime < threshold {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_log_file_is_recognized() {
        assert!(is_rotated_log_file("glass.2026-04-17.log"));
        assert!(is_rotated_log_file("glass.2020-01-01.log"));
    }

    #[test]
    fn non_rotated_files_are_ignored() {
        assert!(!is_rotated_log_file("glass.log"));
        assert!(!is_rotated_log_file("glass_panic_fallback.log"));
        assert!(!is_rotated_log_file("other.2026-04-17.log"));
        assert!(!is_rotated_log_file("glass.2026-04-17.txt"));
        assert!(!is_rotated_log_file("glass..log"));
        assert!(!is_rotated_log_file("glass.2026.04.17.log"));
        assert!(!is_rotated_log_file(""));
    }
}
